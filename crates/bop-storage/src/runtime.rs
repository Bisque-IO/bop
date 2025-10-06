use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, warn};

/// Configuration options for constructing the shared storage runtime.
#[derive(Debug, Clone)]
pub(crate) struct StorageRuntimeOptions {
    pub worker_threads: Option<usize>,
    pub shutdown_timeout: Duration,
}

impl Default for StorageRuntimeOptions {
    fn default() -> Self {
        Self {
            worker_threads: None,
            shutdown_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Error)]
pub(crate) enum StorageRuntimeError {
    #[error("failed to build Tokio runtime: {0}")]
    Build(#[from] io::Error),
}

/// Shared Tokio runtime wrapper that provides graceful shutdown primitives.
#[derive(Debug)]
pub(crate) struct StorageRuntime {
    runtime: Mutex<Option<Arc<Runtime>>>,
    #[allow(dead_code)]
    handle: Handle,
    shutdown_token: CancellationToken,
    shutdown_timeout: Duration,
}

impl StorageRuntime {
    /// Create a runtime using the provided options.
    #[instrument(skip(options), fields(
        worker_threads = ?options.worker_threads,
        shutdown_timeout_secs = options.shutdown_timeout.as_secs()
    ))]
    pub fn create(options: StorageRuntimeOptions) -> Result<Arc<Self>, StorageRuntimeError> {
        info!("creating storage runtime");
        debug!(
            "runtime configuration: worker_threads={:?}, shutdown_timeout={:?}",
            options.worker_threads, options.shutdown_timeout
        );

        let mut builder = Builder::new_multi_thread();
        builder.enable_all();
        if let Some(threads) = options.worker_threads {
            let threads = threads.max(1);
            debug!("configuring runtime with {} worker threads", threads);
            builder.worker_threads(threads);
        }

        let runtime = builder.build().map_err(|e| {
            error!("failed to build tokio runtime: {}", e);
            e
        })?;

        info!("storage runtime created successfully");
        Ok(Self::from_runtime(runtime, options.shutdown_timeout))
    }

    /// Wrap an existing Tokio runtime with storage-specific shutdown handling.
    #[instrument(skip(runtime), fields(shutdown_timeout_secs = shutdown_timeout.as_secs()))]
    pub fn from_runtime(runtime: Runtime, shutdown_timeout: Duration) -> Arc<Self> {
        info!("wrapping existing tokio runtime with storage shutdown handling");
        debug!("shutdown timeout: {:?}", shutdown_timeout);

        let runtime = Arc::new(runtime);
        let handle = runtime.handle().clone();
        Arc::new(Self {
            runtime: Mutex::new(Some(runtime)),
            handle,
            shutdown_token: CancellationToken::new(),
            shutdown_timeout,
        })
    }

    /// Clone the underlying runtime reference if it is still active.
    #[allow(dead_code)]
    pub fn runtime(&self) -> Option<Arc<Runtime>> {
        let guard = self.runtime.lock().expect("storage runtime mutex poisoned");
        guard.as_ref().cloned()
    }

    /// Return a clone of the Tokio handle.
    #[allow(dead_code)]
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    /// Cancellation token signalled during shutdown.
    #[allow(dead_code)]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Returns true once shutdown has been initiated.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_token.is_cancelled()
            || self
                .runtime
                .lock()
                .expect("storage runtime mutex poisoned")
                .is_none()
    }

    /// Trigger a graceful shutdown of the runtime.
    #[instrument(skip(self))]
    pub fn shutdown(&self) {
        if self.is_shutdown() {
            debug!("shutdown called but runtime already shutdown");
            return;
        }

        info!("initiating storage runtime shutdown");
        self.shutdown_token.cancel();
        debug!("shutdown token cancelled");

        let runtime = self
            .runtime
            .lock()
            .expect("storage runtime mutex poisoned")
            .take();

        if let Some(runtime) = runtime {
            let strong_count = Arc::strong_count(&runtime);
            debug!("runtime strong count: {}", strong_count);

            match Arc::try_unwrap(runtime) {
                Ok(runtime) => {
                    if Handle::try_current().is_ok() {
                        info!(
                            "shutting down runtime in background (called from within tokio runtime)"
                        );
                        runtime.shutdown_background();
                    } else {
                        info!(
                            "shutting down runtime with timeout: {:?}",
                            self.shutdown_timeout
                        );
                        runtime.shutdown_timeout(self.shutdown_timeout);
                    }
                    info!("storage runtime shutdown complete");
                }
                Err(runtime) => {
                    let count = Arc::strong_count(&runtime);
                    warn!(
                        "cannot shutdown runtime: still {} strong references exist",
                        count
                    );
                    debug_assert!(
                        Arc::strong_count(&runtime) == 1,
                        "storage runtime shutdown called while runtime still shared",
                    );
                }
            }
        } else {
            debug!("no runtime to shutdown (already taken)");
        }
    }
}

impl Drop for StorageRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
