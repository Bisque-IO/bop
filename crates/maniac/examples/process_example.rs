//! Example demonstrating the async process API.
//!
//! This example shows how to:
//! - Spawn a process and wait for it
//! - Capture process output
//! - Use piped stdio for interactive communication

use maniac::future::block_on;
use maniac::process::Command;
use maniac::runtime::DefaultExecutor;
use std::error::Error;

async fn run_examples() -> std::io::Result<()> {
    // Example 1: Simple process with inherited stdio
    println!("=== Example 1: Run 'echo' command ===");
    #[cfg(unix)]
    let status = Command::new("echo")
        .arg("Hello from maniac!")
        .status()
        .await?;
    #[cfg(windows)]
    let status = Command::new("cmd")
        .args(["/C", "echo", "Hello from maniac!"])
        .status()
        .await?;
    println!("Exit status: {:?}", status);

    // Example 2: Capture output
    println!("\n=== Example 2: Capture output ===");
    #[cfg(unix)]
    let output = Command::new("echo")
        .arg("Captured output!")
        .output()
        .await?;
    #[cfg(windows)]
    let output = Command::new("cmd")
        .args(["/C", "echo", "Captured output!"])
        .output()
        .await?;
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("Exit status: {:?}", output.status);

    // Example 3: Check exit code
    println!("\n=== Example 3: Exit code ===");
    #[cfg(unix)]
    let status = Command::new("sh").args(["-c", "exit 42"]).status().await?;
    #[cfg(windows)]
    let status = Command::new("cmd")
        .args(["/C", "exit", "42"])
        .status()
        .await?;
    println!("Exit code: {:?}", status.code());

    println!("\n=== All examples completed! ===");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    // Create the maniac runtime
    let runtime = DefaultExecutor::new_single_threaded();

    // Spawn our async task
    let handle = runtime.spawn(async move {
        if let Err(e) = run_examples().await {
            eprintln!("Error: {}", e);
        }
    })?;

    // Wait for completion
    block_on(handle);
    Ok(())
}
