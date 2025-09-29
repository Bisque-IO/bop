use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio_util::sync::CancellationToken;

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
    pub fn create(options: StorageRuntimeOptions) -> Result<Arc<Self>, StorageRuntimeError> {
        let mut builder = Builder::new_multi_thread();
        builder.enable_all();
        if let Some(threads) = options.worker_threads {
            builder.worker_threads(threads.max(1));
        }
        let runtime = builder.build()?;
        Ok(Self::from_runtime(runtime, options.shutdown_timeout))
    }

    /// Wrap an existing Tokio runtime with storage-specific shutdown handling.
    pub fn from_runtime(runtime: Runtime, shutdown_timeout: Duration) -> Arc<Self> {
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
    pub fn shutdown(&self) {
        if self.is_shutdown() {
            return;
        }

        self.shutdown_token.cancel();

        let runtime = self
            .runtime
            .lock()
            .expect("storage runtime mutex poisoned")
            .take();

        if let Some(runtime) = runtime {
            match Arc::try_unwrap(runtime) {
                Ok(runtime) => {
                    if Handle::try_current().is_ok() {
                        runtime.shutdown_background();
                    } else {
                        runtime.shutdown_timeout(self.shutdown_timeout);
                    }
                }
                Err(runtime) => {
                    debug_assert!(
                        Arc::strong_count(&runtime) == 1,
                        "storage runtime shutdown called while runtime still shared",
                    );
                }
            }
        }
    }
}

impl Drop for StorageRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
