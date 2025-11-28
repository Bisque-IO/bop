use maniac::future::block_on;
use maniac::runtime::{DefaultExecutor, timer::Timer};
use maniac::time::Instant;
use std::error::Error;
use std::time::Duration;

async fn tick_printer(ticks: usize, interval: Duration) {
    maniac::runtime::worker::pin_stack();
    let timer = Timer::new();
    for remaining in (0..ticks).rev() {
        let now = Instant::now_high_frequency();
        timer.delay(interval).await;
        println!("tick ({} remaining) {:?}", remaining, now.elapsed());
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let runtime = DefaultExecutor::new_single_threaded();

    let handle = runtime.spawn(async move {
        tick_printer(50, Duration::from_millis(25)).await;
    })?;

    block_on(handle);
    Ok(())
}
