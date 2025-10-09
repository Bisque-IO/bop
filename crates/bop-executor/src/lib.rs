//! Low-level executor primitives ported from the Odin implementation in odin/executor.
//!
//! The crate mirrors the layout of the original modules (signal, slot, selector, and waker)
//! and keeps the API surface intentionally low-level so higher level schedulers can be built
//! on top. Some parts of the original Odin sources were stubs (e.g. signal group reservation
//! and executor scheduling), so the matching Rust types currently expose placeholders until
//! the upstream logic is completed.

#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod async_task;
// pub mod async_task_adapter;
pub mod executor;
pub mod selector;
pub mod signal;
pub mod slot;
pub mod waker;

pub use executor::{ExecuteError, Executor};
pub use selector::Selector;
pub use signal::{
    SIGNAL_CAPACITY, Signal, SignalGroup, SignalSetResult, signal_nearest,
    signal_nearest_branchless,
};
pub use slot::{Slot, SlotState, SlotWorker};
pub use waker::Waker;
