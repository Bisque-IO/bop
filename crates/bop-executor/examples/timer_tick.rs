use bop_executor::runtime::Runtime;
use bop_executor::task::{ArenaConfig, ArenaOptions};
use bop_executor::timer::Timer;
use futures_lite::future::block_on;
use std::error::Error;
use std::time::Duration;

async fn tick_printer(ticks: usize, interval: Duration) {
    let timer = Timer::new();
    for _ in 0..ticks {
        timer.delay(interval).await;
        println!("tick");
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let runtime: Runtime<10, 6> = Runtime::new(ArenaConfig::new(2, 16)?, ArenaOptions::default(), 1)?;

    let tick_future = tick_printer(5, Duration::from_millis(500));
    let handle = runtime.spawn(tick_future)?;

    block_on(handle);
    Ok(())
}
