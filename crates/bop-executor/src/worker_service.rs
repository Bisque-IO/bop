use crate::mpsc;
use crate::worker_message::{MessageSlot, WorkerMessage};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_QUEUE_SEG_SHIFT: usize = 7;
const DEFAULT_QUEUE_NUM_SEGS_SHIFT: usize = 10;

#[derive(Clone, Copy, Debug)]
pub struct WorkerServiceConfig {
    pub tick_duration: Duration,
}

impl Default for WorkerServiceConfig {
    fn default() -> Self {
        Self {
            tick_duration: Duration::from_micros(500),
        }
    }
}

pub struct WorkerRegistration {
    pub worker_id: u32,
    pub sender: mpsc::Sender<MessageSlot>,
    pub receiver: mpsc::Receiver<MessageSlot>,
    pub now_ns: Arc<AtomicU64>,
    pub tick_duration_ns: u64,
}

struct WorkerEntry {
    id: u32,
    now_ns: Arc<AtomicU64>,
    sender: mpsc::Sender<MessageSlot>,
}

pub struct WorkerService {
    tick_duration: Duration,
    tick_duration_ns: u64,
    clock_ns: Arc<AtomicU64>,
    workers: Mutex<Vec<WorkerEntry>>,
    shutdown: AtomicBool,
    next_worker_id: AtomicU32,
    tick_thread: Mutex<Option<thread::JoinHandle<()>>>,
}

impl WorkerService {
    pub fn start(config: WorkerServiceConfig) -> Arc<Self> {
        let tick_duration_ns = normalize_tick_duration_ns(config.tick_duration);
        let service = Arc::new(Self {
            tick_duration: config.tick_duration,
            tick_duration_ns,
            clock_ns: Arc::new(AtomicU64::new(0)),
            workers: Mutex::new(Vec::new()),
            shutdown: AtomicBool::new(false),
            next_worker_id: AtomicU32::new(0),
            tick_thread: Mutex::new(None),
        });

        Self::spawn_tick_thread(&service);
        service
    }

    pub fn register_worker(&self) -> WorkerRegistration {
        let worker_id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::new(DEFAULT_QUEUE_SEG_SHIFT, DEFAULT_QUEUE_NUM_SEGS_SHIFT);
        let now_ns = Arc::new(AtomicU64::new(self.clock_ns.load(Ordering::Acquire)));

        {
            let mut workers = self.workers.lock().expect("workers mutex poisoned");
            workers.push(WorkerEntry {
                id: worker_id,
                now_ns: Arc::clone(&now_ns),
                sender: sender.clone(),
            });
        }

        WorkerRegistration {
            worker_id,
            sender,
            receiver,
            now_ns,
            tick_duration_ns: self.tick_duration_ns,
        }
    }

    pub fn post_message(
        &self,
        worker_id: u32,
        message: WorkerMessage,
    ) -> Result<(), WorkerMessage> {
        let mut workers = self.workers.lock().expect("workers mutex poisoned");
        if let Some(entry) = workers.iter_mut().find(|entry| entry.id == worker_id) {
            crate::worker_message::push_message(&mut entry.sender, message)
        } else {
            Err(message)
        }
    }

    pub fn tick_duration(&self) -> Duration {
        self.tick_duration
    }

    pub fn clock_ns(&self) -> u64 {
        self.clock_ns.load(Ordering::Acquire)
    }

    pub fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::Release) {
            return;
        }

        if let Some(handle) = self.tick_thread.lock().unwrap().take() {
            let _ = handle.join();
        }
    }

    fn spawn_tick_thread(service: &Arc<Self>) {
        let weak = Arc::downgrade(service);
        let tick_duration = service.tick_duration;
        let handle = thread::spawn(move || {
            let start = Instant::now();
            while let Some(service) = weak.upgrade() {
                if service.shutdown.load(Ordering::Acquire) {
                    break;
                }

                let now_ns = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                service.clock_ns.store(now_ns, Ordering::Release);

                {
                    let workers = service.workers.lock().expect("workers mutex poisoned");
                    for worker in workers.iter() {
                        worker.now_ns.store(now_ns, Ordering::Release);
                    }
                }

                thread::sleep(tick_duration);
            }
        });

        *service.tick_thread.lock().unwrap() = Some(handle);
    }
}

impl Drop for WorkerService {
    fn drop(&mut self) {
        if !self.shutdown.load(Ordering::Acquire) {
            self.shutdown();
        }
    }
}

fn normalize_tick_duration_ns(duration: Duration) -> u64 {
    let nanos = duration.as_nanos().max(1).min(u128::from(u64::MAX)) as u64;
    nanos.next_power_of_two()
}
