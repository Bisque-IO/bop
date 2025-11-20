// pub mod allocator {
// 	pub use maniac_allocator::*;
// }

pub mod runtime {
    pub use maniac_runtime::runtime::*;
}

pub mod sync {
    pub use maniac_runtime::sync::*;
}

// pub mod usockets {
// 	pub use maniac_usockets::usockets::*;
// }

#[cfg(test)]
mod tests {
    use super::*;
    use maniac_runtime::future::block_on;
    use std::time::Duration;

    #[test]
    fn it_works() {
        let rt = runtime::Executor::<10, 10>::new_single_threaded();
        let join = rt.spawn(async move {
            let timer = runtime::timer::Timer::new();
            println!("waiting 1 second...");
            timer.delay(Duration::from_secs(1)).await;
            println!("done!");
        });
        block_on(join.expect(""));
    }
}
