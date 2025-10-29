use bop_executor::runtime::Runtime;
use bop_executor::task::{TaskArenaConfig, TaskArenaOptions};
use bop_executor::timer::Timer;
use futures_lite::future::block_on;
use std::error::Error;
use std::time::Duration;

async fn tick_printer(ticks: usize, interval: Duration) {
    let timer = Timer::new();
    for remaining in (0..ticks).rev() {
        timer.delay(interval).await;
        println!("tick ({} remaining)", remaining);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let runtime: Runtime<10, 6> =
        Runtime::new(TaskArenaConfig::new(2, 16)?, TaskArenaOptions::default(), 1)?;

    let handle = runtime.spawn(async move {
        tick_printer(50, Duration::from_millis(500)).await;
    })?;

    block_on(handle);
    Ok(())
}
