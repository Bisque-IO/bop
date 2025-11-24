use std::{
    collections::HashSet,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

use crate::driver::op::OpCanceller;

/// CancelHandle is used to pass to io actions with CancelableAsyncReadRent.
/// Create a CancelHandle with Canceller::handle.
/// Thread-safe and can be sent across threads.
#[derive(Clone)]
pub struct CancelHandle {
    shared: Arc<Shared>,
}

/// Canceller is a user-hold struct to cancel io operations.
/// A canceller can associate with multiple io operations.
/// Thread-safe and can be sent across threads.
#[derive(Default)]
pub struct Canceller {
    shared: Arc<Shared>,
}

pub(crate) struct AssociateGuard {
    op_canceller: OpCanceller,
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    canceled: AtomicBool,
    slot_ref: Mutex<HashSet<OpCanceller>>,
}

impl Canceller {
    /// Create a new Canceller.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Cancel all related operations.
    pub fn cancel(self) -> Self {
        // Set the canceled flag atomically
        self.shared
            .canceled
            .store(true, std::sync::atomic::Ordering::Release);

        // Take all pending operations
        let mut slot = HashSet::new();
        {
            let mut guard = self.shared.slot_ref.lock().unwrap();
            std::mem::swap(&mut slot, &mut *guard);
        }

        // Cancel all operations
        for op_canceller in slot.iter() {
            unsafe { op_canceller.cancel() };
        }
        slot.clear();

        // Create a new canceller with fresh state
        Canceller {
            shared: Arc::new(Shared {
                canceled: AtomicBool::new(false),
                slot_ref: Mutex::new(slot),
            }),
        }
    }

    /// Create a CancelHandle which can be used to pass to io operation.
    #[inline]
    pub fn handle(&self) -> CancelHandle {
        CancelHandle {
            shared: self.shared.clone(),
        }
    }
}

impl CancelHandle {
    pub(crate) fn canceled(&self) -> bool {
        self.shared
            .canceled
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn associate_op(self, op_canceller: OpCanceller) -> AssociateGuard {
        {
            let mut guard = self.shared.slot_ref.lock().unwrap();
            guard.insert(op_canceller.clone());
        }
        AssociateGuard {
            op_canceller,
            shared: self.shared,
        }
    }
}

impl Drop for AssociateGuard {
    fn drop(&mut self) {
        let mut guard = self.shared.slot_ref.lock().unwrap();
        guard.remove(&self.op_canceller);
    }
}

pub(crate) fn operation_canceled() -> std::io::Error {
    std::io::Error::from_raw_os_error(125)
}
