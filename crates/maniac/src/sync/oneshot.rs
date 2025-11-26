//! Oneshot channel for single-use communication.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use crate::future::event::Event;

pub use super::multishot::{Receiver, RecvError, Recv, Sender};

/// Creates a new oneshot channel.
///
/// # Examples
///
/// ```
/// use maniac::sync::oneshot;
///
/// #[maniac::main]
/// async fn main() {
///     let (tx, mut rx) = oneshot::channel();
///     
///     tx.send(42).unwrap();
///     assert_eq!(rx.await.unwrap(), 42);
/// }
/// ```
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    super::multishot::channel()
}
