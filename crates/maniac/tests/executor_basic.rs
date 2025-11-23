use maniac::future::block_on;
use maniac::runtime::task::{TaskArenaConfig, TaskArenaOptions};
use maniac::runtime::{DefaultExecutor, Executor};

#[test]
fn test_basic_executor() {
    eprintln!("Creating executor...");
    let executor = DefaultExecutor::new(
        TaskArenaConfig::new(2, 1024).unwrap(),
        TaskArenaOptions::default(),
        2,
        2,
    )
    .expect("Failed to create executor");

    eprintln!("Spawning task...");
    let handle = executor
        .spawn(async move {
            eprintln!("Task started!");
            42
        })
        .expect("spawn failed");

    eprintln!("Blocking on task...");
    let result = block_on(handle);
    eprintln!("Task completed with result: {}", result);
    assert_eq!(result, 42);
}
