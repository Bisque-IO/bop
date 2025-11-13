use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

/// Errors surfaced by [`ChunkStorageQuota`].
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ChunkQuotaError {
    /// Attempted to reserve more space than the remaining global quota allows.
    #[error("chunk quota exceeded: used {used} of {limit} bytes, request {requested}")]
    Exhausted {
        used: u64,
        limit: u64,
        requested: u64,
    },
}

/// RAII guard representing an active reservation against the global chunk quota.
///
/// Dropping the guard returns the reserved bytes to the global pool.
#[derive(Debug)]
pub struct ChunkQuotaGuard {
    quota: Arc<ChunkStorageQuota>,
    amount: u64,
    released: bool,
}

impl ChunkQuotaGuard {
    fn new(quota: Arc<ChunkStorageQuota>, amount: u64) -> Self {
        Self {
            quota,
            amount,
            released: false,
        }
    }

    /// Manually releases the reservation before the guard is dropped.
    pub fn release(mut self) {
        if !self.released {
            self.quota.release(self.amount);
            self.released = true;
        }
    }
}

impl Drop for ChunkQuotaGuard {
    fn drop(&mut self) {
        if !self.released {
            self.quota.release(self.amount);
            self.released = true;
        }
    }
}

/// Global quota shared by all local chunk stores managed by the process.
#[derive(Debug)]
pub struct ChunkStorageQuota {
    limit: u64,
    used: AtomicU64,
}

impl ChunkStorageQuota {
    /// Creates a new quota tracker with a byte limit.
    pub fn new(limit: u64) -> Arc<Self> {
        Arc::new(Self {
            limit,
            used: AtomicU64::new(0),
        })
    }

    /// Attempts to reserve `bytes` from the global budget.
    pub fn try_acquire(self: &Arc<Self>, bytes: u64) -> Result<ChunkQuotaGuard, ChunkQuotaError> {
        loop {
            let current = self.used.load(Ordering::Acquire);
            let new_used = current.saturating_add(bytes);
            if new_used > self.limit {
                return Err(ChunkQuotaError::Exhausted {
                    used: current,
                    limit: self.limit,
                    requested: bytes,
                });
            }

            if self
                .used
                .compare_exchange(current, new_used, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Ok(ChunkQuotaGuard::new(self.clone(), bytes));
            }
        }
    }

    fn release(&self, amount: u64) {
        self.used.fetch_sub(amount, Ordering::AcqRel);
    }

    /// Returns the configured quota limit.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// Returns the current usage tracked by the quota.
    pub fn used(&self) -> u64 {
        self.used.load(Ordering::Acquire)
    }
}
