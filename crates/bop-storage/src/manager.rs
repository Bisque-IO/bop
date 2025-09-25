use std::thread::{self, JoinHandle};

use crossfire::{MTx, Rx, mpsc};

const DEFAULT_QUEUE_CAPACITY: usize = 64;

enum ManagerCommand {
    Execute(Box<dyn FnOnce() + Send + 'static>),
    Shutdown,
}

/// Basic storage manager with a dedicated worker thread processing queued jobs.
pub struct Manager {
    sender: MTx<ManagerCommand>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManagerClosedError;

impl Manager {
    /// Create a manager with the default queue capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    /// Create a manager using a bounded crossfire queue sized to `capacity`.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::bounded_blocking(capacity.max(1));
        let worker = spawn_worker(receiver);
        Self {
            sender,
            worker: Some(worker),
        }
    }

    /// Submit a job to be executed on the manager thread.
    pub fn submit<F>(&self, job: F) -> Result<(), ManagerClosedError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.sender
            .send(ManagerCommand::Execute(Box::new(job)))
            .map_err(|_| ManagerClosedError)
    }

    /// Signal the worker thread to stop and block until it terminates.
    pub fn shutdown(mut self) {
        self.stop_worker();
    }

    fn stop_worker(&mut self) {
        if let Some(handle) = self.worker.take() {
            // Best-effort shutdown signal; ignore failures if worker already exited.
            let _ = self.sender.send(ManagerCommand::Shutdown);
            let _ = handle.join();
        }
    }
}

impl Drop for Manager {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

fn spawn_worker(receiver: Rx<ManagerCommand>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("bop-storage-manager".into())
        .spawn(move || {
            while let Ok(command) = receiver.recv() {
                match command {
                    ManagerCommand::Execute(job) => job(),
                    ManagerCommand::Shutdown => break,
                }
            }
        })
        .expect("failed to spawn storage manager thread")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    #[test]
    fn runs_jobs_on_worker_thread() {
        let manager = Manager::new();
        let barrier = Arc::new(Barrier::new(2));
        let counter = Arc::new(AtomicUsize::new(0));

        let job_barrier = barrier.clone();
        let job_counter = counter.clone();
        manager
            .submit(move || {
                job_counter.fetch_add(1, Ordering::SeqCst);
                job_barrier.wait();
            })
            .unwrap();

        barrier.wait();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shutdown_joins_worker() {
        let manager = Manager::new();
        manager.submit(|| {}).unwrap();
        manager.shutdown();
    }
}
