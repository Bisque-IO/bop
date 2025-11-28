//! Asynchronous process management for the maniac runtime.
//!
//! This module provides types for spawning and managing child processes
//! asynchronously on both Unix and Windows platforms.
//!
//! # Overview
//!
//! The main types in this module are:
//!
//! - [`Command`]: A builder for configuring and spawning processes
//! - [`Child`]: A handle to a spawned child process
//! - [`ChildStdin`]: An async write handle for the child's stdin
//! - [`ChildStdout`]: An async read handle for the child's stdout
//! - [`ChildStderr`]: An async read handle for the child's stderr
//! - [`ExitStatus`]: The exit status of a finished process
//!
//! # Examples
//!
//! Spawn a process and wait for it to complete:
//!
//! ```no_run
//! use maniac::process::Command;
//!
//! # async fn example() -> std::io::Result<()> {
//! let status = Command::new("echo")
//!     .arg("Hello, world!")
//!     .status()
//!     .await?;
//!
//! println!("Process exited with: {:?}", status);
//! # Ok(())
//! # }
//! ```
//!
//! Spawn a process and capture its output:
//!
//! ```no_run
//! use maniac::process::Command;
//!
//! # async fn example() -> std::io::Result<()> {
//! let output = Command::new("echo")
//!     .arg("Hello, world!")
//!     .output()
//!     .await?;
//!
//! println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
//! # Ok(())
//! # }
//! ```
//!
//! Spawn a process with piped stdio for interactive communication:
//!
//! ```no_run
//! use maniac::process::{Command, Stdio};
//! use maniac::io::AsyncWriteRentExt;
//!
//! # async fn example() -> std::io::Result<()> {
//! let mut child = Command::new("cat")
//!     .stdin(Stdio::piped())
//!     .stdout(Stdio::piped())
//!     .spawn()?;
//!
//! if let Some(mut stdin) = child.stdin.take() {
//!     stdin.write_all(b"Hello from stdin!").await?;
//! }
//!
//! let status = child.wait().await?;
//! # Ok(())
//! # }
//! ```

mod child;
mod command;

#[cfg(unix)]
mod child_reaper;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use child::{Child, ChildStderr, ChildStdin, ChildStdout, ExitStatus, Output};
pub use command::Command;

// Re-export Stdio for convenience
pub use std::process::Stdio;

