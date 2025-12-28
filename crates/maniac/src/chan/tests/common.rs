use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(miri))]
pub const ROUND: usize = 10000;
#[cfg(miri)]
pub const ROUND: usize = 20;

pub fn _setup_log() {
    #[cfg(feature = "trace_log")]
    {
        use tracing_subscriber::{Registry, fmt, layer::Layer};

        // Create a subscriber that logs to both stdout and a ring buffer
        let stdout_layer = fmt::layer().with_target(false).with_threadnames(false);

        // Use a Vec as in-memory ring buffer for test output
        // In real tests, this would be captured by the test framework
        let subscriber = Registry::default().with(stdout_layer);

        tracing::subscriber::set_global_default(subscriber)
            .expect("setting tracing subscriber failed");
    }
}

pub static TEST_RUNTIME: std::sync::LazyLock<crate::runtime::DefaultRuntime> =
    std::sync::LazyLock::new(|| crate::runtime::new_multi_threaded(16, 16 * 4096).unwrap());

#[allow(dead_code)]
macro_rules! runtime_block_on {
    ($f: expr) => {{
        let joiner = crate::chan::tests::common::TEST_RUNTIME.spawn($f).unwrap();
        crate::future::block_on(joiner)
    }};
}
pub(super) use runtime_block_on;

#[allow(dead_code)]
macro_rules! async_spawn {
    ($f: expr) => {{ crate::spawn($f).unwrap() }};
}
pub(super) use async_spawn;

#[allow(dead_code)]
macro_rules! async_join_result {
    ($th: expr) => {{ $th.await }};
}
pub(super) use async_join_result;

// Global atomic drop counter with mutex for serialization
// Tests using drop counter must acquire DROP_COUNTER_LOCK first
static DROP_COUNTER: AtomicUsize = AtomicUsize::new(0);
pub static DROP_COUNTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub trait TestDropMsg: Unpin + Send + 'static {
    fn new(v: usize) -> Self;

    fn get_value(&self) -> usize;
}

pub struct SmallMsg(pub usize);

impl Drop for SmallMsg {
    fn drop(&mut self) {
        DROP_COUNTER.fetch_add(1, Ordering::SeqCst);
    }
}

impl TestDropMsg for SmallMsg {
    fn new(v: usize) -> Self {
        Self(v)
    }

    fn get_value(&self) -> usize {
        self.0
    }
}

pub struct LargeMsg([usize; 4]);

impl TestDropMsg for LargeMsg {
    fn new(v: usize) -> Self {
        Self([v, v, v, v])
    }

    fn get_value(&self) -> usize {
        self.0[0]
    }
}

impl Drop for LargeMsg {
    fn drop(&mut self) {
        DROP_COUNTER.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn get_drop_counter() -> usize {
    DROP_COUNTER.load(Ordering::SeqCst)
}

pub fn reset_drop_counter() {
    DROP_COUNTER.store(0, Ordering::SeqCst);
}

pub async fn sleep(duration: std::time::Duration) {
    crate::time::sleep(duration).await;
}

pub async fn timeout<F, T>(duration: std::time::Duration, future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    crate::time::timeout(duration, future)
        .await
        .map_err(|_| format!("Test timed out after {:?}", duration))
}
