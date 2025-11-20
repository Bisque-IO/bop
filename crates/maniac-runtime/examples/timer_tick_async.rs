use futures_lite::future::block_on;
use maniac_runtime::runtime::Executor;
use maniac_runtime::runtime::timer::Timer;
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
    let runtime = Executor::<10, 6>::new_single_threaded();

    let handle = runtime.spawn(async move {
        tick_printer(50, Duration::from_millis(100)).await;
    })?;

    block_on(handle);
    Ok(())
}
