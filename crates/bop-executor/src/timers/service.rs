use crate::timers::context::{MergeEntry, TimerWorkerShared};
use crate::timers::handle::{TimerHandle, TimerState};
use crate::utils::StripedAtomicU64;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct TimerConfig {
    pub tick_duration: Duration,
}

impl Default for TimerConfig {
    fn default() -> Self {
        Self {
            tick_duration: Duration::from_micros(50),
        }
    }
}

struct WorkerRegistration {
    id: usize,
    shared: Arc<TimerWorkerShared>,
    merge_inbox: Arc<Mutex<Vec<MergeEntry>>>,
    garbage: Arc<StripedAtomicU64>,
    last_tick: AtomicU64,
}

impl WorkerRegistration {
    fn new(
        id: usize,
        shared: Arc<TimerWorkerShared>,
        merge_inbox: Arc<Mutex<Vec<MergeEntry>>>,
        garbage: Arc<StripedAtomicU64>,
    ) -> Self {
        Self {
            id,
            shared,
            merge_inbox,
            garbage,
            last_tick: AtomicU64::new(0),
        }
    }
}

pub struct TimerWorkerHandle {
    service: Arc<TimerService>,
    id: usize,
}

impl TimerWorkerHandle {
    #[inline]
    pub fn id(&self) -> usize {
        self.id
    }
}

impl Drop for TimerWorkerHandle {
    fn drop(&mut self) {
        self.service.unregister_worker(self.id);
    }
}

pub struct TimerService {
    config: TimerConfig,
    workers: Mutex<Vec<WorkerRegistration>>,
    next_worker_id: AtomicUsize,
    merge_queue: Mutex<Vec<MergeEntry>>,
    cancellations: AtomicU64,
    shutdown: AtomicBool,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl TimerService {
    pub fn start(config: TimerConfig) -> Arc<Self> {
        let service = Arc::new(Self {
            config,
            workers: Mutex::new(Vec::new()),
            next_worker_id: AtomicUsize::new(0),
            merge_queue: Mutex::new(Vec::new()),
            cancellations: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            thread: Mutex::new(None),
        });

        let cloned = Arc::clone(&service);
        let handle = thread::Builder::new()
            .name("bop-timer-thread".into())
            .spawn(move || TimerService::run(cloned))
            .expect("failed to spawn timer thread");

        service.thread.lock().unwrap().replace(handle);
        service
    }

    fn run(service: Arc<Self>) {
        let tick_duration = service.config.tick_duration;
        let mut next_tick = Instant::now();
        let mut tick: u64 = 0;

        while !service.shutdown.load(Ordering::Acquire) {
            let now = Instant::now();
            if now < next_tick {
                thread::sleep(next_tick - now);
                continue;
            }

            tick = tick.wrapping_add(1);
            service.update_workers(tick);
            service.drain_merge_queue();

            next_tick += tick_duration;
            if next_tick <= now {
                next_tick = now + tick_duration;
            }
        }
    }

    fn update_workers(&self, tick: u64) {
        let mut workers = self.workers.lock().unwrap();
        for registration in workers.iter_mut() {
            registration.shared.update_now(tick);
            registration.shared.mark_needs_poll();
            registration.last_tick.store(tick, Ordering::Relaxed);
        }
    }

    fn drain_merge_queue(&self) {
        let pending = {
            let mut queue = self.merge_queue.lock().unwrap();
            if queue.is_empty() {
                return;
            }
            std::mem::take(&mut *queue)
        };

        let (inboxes, shareds) = {
            let workers = self.workers.lock().unwrap();
            if workers.is_empty() {
                let mut queue = self.merge_queue.lock().unwrap();
                queue.extend(pending);
                return;
            }
            let inboxes = workers
                .iter()
                .map(|worker| Arc::clone(&worker.merge_inbox))
                .collect::<Vec<_>>();
            let shareds = workers
                .iter()
                .map(|worker| Arc::clone(&worker.shared))
                .collect::<Vec<_>>();
            (inboxes, shareds)
        };

        let worker_count = inboxes.len();
        let mut next = 0usize;
        for entry in pending {
            let inbox = &inboxes[next % worker_count];
            {
                let mut guard = inbox.lock().unwrap();
                guard.push(entry);
            }
            shareds[next % worker_count].mark_needs_poll();
            next = (next + 1) % worker_count;
        }
    }

    fn mark_all_workers_need_poll(&self) {
        let workers = self.workers.lock().unwrap();
        for registration in workers.iter() {
            registration.shared.mark_needs_poll();
        }
    }

    pub(crate) fn notify_cancelled(
        &self,
        stripe_hint: usize,
        home_worker: Option<u32>,
        had_entry: bool,
    ) {
        self.cancellations.fetch_add(1, Ordering::Relaxed);
        if !had_entry {
            let workers = self.workers.lock().unwrap();
            if let Some(worker_id) = home_worker.map(|id| id as usize) {
                if let Some(registration) = workers.iter().find(|reg| reg.id == worker_id) {
                    registration.shared.mark_needs_poll();
                    return;
                }
            }
            for registration in workers.iter() {
                registration.shared.mark_needs_poll();
            }
            return;
        }

        let stripe = if stripe_hint == 0 { 1 } else { stripe_hint };
        let workers = self.workers.lock().unwrap();
        if let Some(worker_id) = home_worker.map(|id| id as usize) {
            if let Some(registration) = workers.iter().find(|reg| reg.id == worker_id) {
                registration
                    .garbage
                    .fetch_add_hint(stripe, 1, Ordering::Relaxed);
                registration.shared.mark_needs_poll();
                return;
            }
        }
        for registration in workers.iter() {
            registration.shared.mark_needs_poll();
        }
    }

    pub fn tick_duration(&self) -> Duration {
        self.config.tick_duration
    }

    pub fn schedule_handle(&self, handle: &TimerHandle, deadline_tick: u64) -> u64 {
        let inner = handle.inner();
        let generation = inner.bump_generation();
        inner.record_deadline(deadline_tick);
        inner.set_home_worker(None);
        inner.store_state(TimerState::Migrating, Ordering::Release);
        let entry = MergeEntry::new(
            deadline_tick,
            generation,
            inner.stripe_hint(),
            Arc::clone(inner),
        );
        self.enqueue_merge(entry);
        generation
    }

    pub fn cancellation_count(&self) -> u64 {
        self.cancellations.load(Ordering::Relaxed)
    }

    pub fn register_worker(
        self: &Arc<Self>,
        shared: Arc<TimerWorkerShared>,
        merge_inbox: Arc<Mutex<Vec<MergeEntry>>>,
        garbage: Arc<StripedAtomicU64>,
    ) -> TimerWorkerHandle {
        let id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
        let mut workers = self.workers.lock().unwrap();
        workers.push(WorkerRegistration::new(id, shared, merge_inbox, garbage));
        TimerWorkerHandle {
            service: Arc::clone(self),
            id,
        }
    }

    fn unregister_worker(&self, id: usize) {
        let mut workers = self.workers.lock().unwrap();
        workers.retain(|registration| registration.id != id);
    }

    pub fn enqueue_merge(&self, entry: MergeEntry) {
        {
            let mut queue = self.merge_queue.lock().unwrap();
            queue.push(entry);
        }
        self.mark_all_workers_need_poll();
    }

    pub fn shutdown(self: &Arc<Self>) {
        if !self.shutdown.swap(true, Ordering::Release) {
            let handle = {
                let mut guard = self.thread.lock().unwrap();
                guard.take()
            };
            if let Some(handle) = handle {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for TimerService {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let handle = {
            let mut guard = self.thread.lock().unwrap();
            guard.take()
        };
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timers::context::{MergeEntry, TimerWheelConfig};
    use crate::timers::handle::TimerInner;
    use crate::utils::StripedAtomicU64;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn timer_service_updates_workers() {
        let service = TimerService::start(TimerConfig {
            tick_duration: Duration::from_millis(5),
        });
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let garbage = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );
        service.enqueue_merge(MergeEntry::new(1, 1, 0, Arc::new(TimerInner::new())));

        thread::sleep(Duration::from_millis(25));

        assert!(shared.now() > 0);
        assert!(shared.needs_poll());
        assert_eq!(inbox.lock().unwrap().len(), 1);

        shared.take_needs_poll();
        inbox.lock().unwrap().clear();
        service.shutdown();
    }

    #[test]
    fn merge_queue_drains_round_robin() {
        let service = TimerService::start(TimerConfig {
            tick_duration: Duration::from_millis(1),
        });

        let shared_a = Arc::new(TimerWorkerShared::new());
        let inbox_a = Arc::new(Mutex::new(Vec::new()));
        let garbage_a = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token_a = service.register_worker(
            Arc::clone(&shared_a),
            Arc::clone(&inbox_a),
            Arc::clone(&garbage_a),
        );

        let shared_b = Arc::new(TimerWorkerShared::new());
        let inbox_b = Arc::new(Mutex::new(Vec::new()));
        let garbage_b = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token_b = service.register_worker(
            Arc::clone(&shared_b),
            Arc::clone(&inbox_b),
            Arc::clone(&garbage_b),
        );

        for tick in 1..=6 {
            service.enqueue_merge(MergeEntry::new(tick, tick, 0, Arc::new(TimerInner::new())));
        }

        for _ in 0..50 {
            thread::sleep(Duration::from_millis(2));
            let a = inbox_a.lock().unwrap().len();
            let b = inbox_b.lock().unwrap().len();
            if a + b == 6 {
                assert!((a as isize - b as isize).abs() <= 1);
                break;
            }
        }

        service.shutdown();
    }

    #[test]
    fn notify_cancelled_updates_worker_state() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let garbage = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let handle = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );

        service.notify_cancelled(3, Some(handle.id() as u32), true);

        assert!(shared.needs_poll());
        assert_eq!(garbage.total(), 1);
        assert!(service.cancellation_count() >= 1);

        shared.take_needs_poll();
        garbage.clear(Ordering::AcqRel);
        service.shutdown();
    }

    #[test]
    fn notify_cancelled_without_entry_marks_all_workers() {
        let service = TimerService::start(TimerConfig::default());
        let shared_a = Arc::new(TimerWorkerShared::new());
        let inbox_a = Arc::new(Mutex::new(Vec::new()));
        let garbage = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token_a = service.register_worker(
            Arc::clone(&shared_a),
            Arc::clone(&inbox_a),
            Arc::clone(&garbage),
        );

        let shared_b = Arc::new(TimerWorkerShared::new());
        let inbox_b = Arc::new(Mutex::new(Vec::new()));
        let _token_b = service.register_worker(
            Arc::clone(&shared_b),
            Arc::clone(&inbox_b),
            Arc::clone(&garbage),
        );

        service.notify_cancelled(0, None, false);

        assert!(shared_a.needs_poll());
        assert!(shared_b.needs_poll());
        assert_eq!(garbage.total(), 0);

        service.shutdown();
    }

    #[test]
    fn enqueue_merge_before_worker_registration_delays_delivery() {
        let service = TimerService::start(TimerConfig {
            tick_duration: Duration::from_millis(1),
        });
        let handle = Arc::new(TimerInner::new());
        handle.store_state(TimerState::Migrating, Ordering::Release);
        let generation = handle.bump_generation();
        service.enqueue_merge(MergeEntry::new(4, generation, 0, Arc::clone(&handle)));

        thread::sleep(Duration::from_millis(20));

        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let garbage = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );

        let start = Instant::now();
        loop {
            if inbox.lock().unwrap().len() == 1 {
                assert!(shared.needs_poll());
                break;
            }
            if start.elapsed() > Duration::from_millis(200) {
                panic!("merge entry was not delivered after worker registration");
            }
            thread::sleep(Duration::from_millis(5));
        }

        service.shutdown();
    }

    #[test]
    fn shutdown_is_idempotent() {
        let service = TimerService::start(TimerConfig::default());
        service.shutdown();
        service.shutdown();
    }

    #[test]
    fn schedule_handle_bumps_generation_and_enqueues_merge() {
        let service = TimerService::start(TimerConfig::default());
        let shared = Arc::new(TimerWorkerShared::new());
        let inbox = Arc::new(Mutex::new(Vec::new()));
        let garbage = Arc::new(StripedAtomicU64::new(
            TimerWheelConfig::DEFAULT_GARBAGE_STRIPES,
        ));
        let _token = service.register_worker(
            Arc::clone(&shared),
            Arc::clone(&inbox),
            Arc::clone(&garbage),
        );

        let handle = TimerHandle::new();
        let generation = service.schedule_handle(&handle, 5);
        assert_eq!(handle.generation(), generation);
        assert_eq!(handle.state(), TimerState::Migrating);

        for _ in 0..50 {
            thread::sleep(Duration::from_millis(2));
            if !inbox.lock().unwrap().is_empty() {
                break;
            }
        }
        assert!(!inbox.lock().unwrap().is_empty());

        service.shutdown();
    }
}
