//! Storage quota management for checkpoint operations.
//!
//! This module implements T7a from the checkpointing plan: hierarchical
//! quota enforcement to prevent disk exhaustion.
//!
//! # Architecture
//!
//! Three-level quota hierarchy:
//! - Global: Total disk space available
//! - Per-tenant: Quota for each tenant
//! - Per-job: Quota for individual checkpoint jobs
//!
//! Uses atomic operations and CAS loops for thread-safe quota management.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;

use crate::manifest::{DbId, JobId};

/// Errors that can occur during quota operations.
#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    /// Global quota exhausted.
    #[error("global quota exhausted: used {used}, limit {limit}")]
    GlobalExhausted { used: u64, limit: u64 },

    /// Tenant quota exhausted.
    #[error("tenant {tenant_id} quota exhausted: used {used}, limit {limit}")]
    TenantExhausted {
        tenant_id: DbId,
        used: u64,
        limit: u64,
    },

    /// Job quota exhausted.
    #[error("job {job_id} quota exhausted: used {used}, limit {limit}")]
    JobExhausted {
        job_id: JobId,
        used: u64,
        limit: u64,
    },

    /// Cannot release more quota than reserved.
    #[error("quota underflow: attempting to release {amount} but only {current} used")]
    Underflow { amount: u64, current: u64 },
}

/// Quota configuration.
#[derive(Debug, Clone)]
pub struct QuotaConfig {
    /// Global quota limit in bytes.
    pub global_limit: u64,

    /// Default per-tenant quota limit in bytes.
    pub default_tenant_limit: u64,

    /// Default per-job quota limit in bytes.
    pub default_job_limit: u64,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            global_limit: 100 * 1024 * 1024 * 1024,        // 100 GiB
            default_tenant_limit: 10 * 1024 * 1024 * 1024, // 10 GiB per tenant
            default_job_limit: 1024 * 1024 * 1024,         // 1 GiB per job
        }
    }
}

/// Per-tenant quota tracking.
#[derive(Debug)]
struct TenantQuota {
    limit: AtomicU64,
    used: AtomicU64,
    jobs: DashMap<JobId, Arc<JobQuota>>,
}

impl TenantQuota {
    fn new(limit: u64) -> Self {
        Self {
            limit: AtomicU64::new(limit),
            used: AtomicU64::new(0),
            jobs: DashMap::new(),
        }
    }
}

/// Per-job quota tracking.
#[derive(Debug)]
struct JobQuota {
    limit: AtomicU64,
    used: AtomicU64,
}

impl JobQuota {
    fn new(limit: u64) -> Self {
        Self {
            limit: AtomicU64::new(limit),
            used: AtomicU64::new(0),
        }
    }
}

/// RAII guard that releases quota when dropped.
///
/// # Example
///
/// ```ignore
/// let guard = quota.reserve(tenant_id, job_id, 1024)?;
/// // Use reserved quota...
/// // Quota automatically released when guard is dropped
/// ```
pub struct ReservationGuard {
    quota: Arc<StorageQuota>,
    tenant_id: DbId,
    job_id: JobId,
    amount: u64,
}

impl Drop for ReservationGuard {
    fn drop(&mut self) {
        if let Err(e) = self.quota.release(self.tenant_id, self.job_id, self.amount) {
            // Log error but don't panic in drop
            eprintln!("Failed to release quota: {}", e);
        }
    }
}

/// Hierarchical storage quota manager.
///
/// # T7a: Hierarchical Quota Enforcement
///
/// Manages three levels of quota:
/// - Global: Total available disk space
/// - Tenant: Per-tenant quota limits
/// - Job: Per-checkpoint-job quota limits
///
/// All operations are thread-safe using atomic operations and CAS loops.
pub struct StorageQuota {
    config: QuotaConfig,
    global_used: AtomicU64,
    tenants: DashMap<DbId, Arc<TenantQuota>>,
}

impl StorageQuota {
    /// Creates a new StorageQuota with the given configuration.
    pub fn new(config: QuotaConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            global_used: AtomicU64::new(0),
            tenants: DashMap::new(),
        })
    }

    /// Reserves quota for a job.
    ///
    /// # Quota Hierarchy
    ///
    /// Checks and reserves quota at three levels:
    /// 1. Global quota
    /// 2. Tenant quota (creates tenant if doesn't exist)
    /// 3. Job quota (creates job if doesn't exist)
    ///
    /// Uses CAS loops to ensure atomic reservation across all levels.
    ///
    /// # Returns
    ///
    /// A `ReservationGuard` that automatically releases quota when dropped.
    pub fn reserve(
        self: &Arc<Self>,
        tenant_id: DbId,
        job_id: JobId,
        bytes: u64,
    ) -> Result<ReservationGuard, QuotaError> {
        // CAS loop for global quota
        loop {
            let current = self.global_used.load(Ordering::Acquire);
            let new_value = match current.checked_add(bytes) {
                Some(v) => v,
                None => {
                    return Err(QuotaError::GlobalExhausted {
                        used: current,
                        limit: self.config.global_limit,
                    });
                }
            };

            if new_value > self.config.global_limit {
                return Err(QuotaError::GlobalExhausted {
                    used: current,
                    limit: self.config.global_limit,
                });
            }

            if self
                .global_used
                .compare_exchange(current, new_value, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
            // Retry CAS
        }

        // Get or create tenant quota
        let tenant = self
            .tenants
            .entry(tenant_id)
            .or_insert_with(|| Arc::new(TenantQuota::new(self.config.default_tenant_limit)))
            .clone();

        // CAS loop for tenant quota
        loop {
            let current = tenant.used.load(Ordering::Acquire);
            let limit = tenant.limit.load(Ordering::Acquire);
            let new_value = match current.checked_add(bytes) {
                Some(v) => v,
                None => {
                    self.global_used.fetch_sub(bytes, Ordering::Release);
                    return Err(QuotaError::TenantExhausted {
                        tenant_id,
                        used: current,
                        limit,
                    });
                }
            };

            if new_value > limit {
                // Rollback global quota
                self.global_used.fetch_sub(bytes, Ordering::Release);
                return Err(QuotaError::TenantExhausted {
                    tenant_id,
                    used: current,
                    limit,
                });
            }

            if tenant
                .used
                .compare_exchange(current, new_value, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
            // Retry CAS
        }

        // Get or create job quota
        let job = tenant
            .jobs
            .entry(job_id)
            .or_insert_with(|| Arc::new(JobQuota::new(self.config.default_job_limit)))
            .clone();

        // CAS loop for job quota
        loop {
            let current = job.used.load(Ordering::Acquire);
            let new_value = current + bytes;
            let limit = job.limit.load(Ordering::Acquire);

            if new_value > limit {
                // Rollback tenant and global quota
                tenant.used.fetch_sub(bytes, Ordering::Release);
                self.global_used.fetch_sub(bytes, Ordering::Release);
                return Err(QuotaError::JobExhausted {
                    job_id,
                    used: current,
                    limit,
                });
            }

            if job
                .used
                .compare_exchange(current, new_value, Ordering::Release, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
            // Retry CAS
        }

        Ok(ReservationGuard {
            quota: self.clone(),
            tenant_id,
            job_id,
            amount: bytes,
        })
    }

    /// Releases quota for a job.
    ///
    /// Called automatically by `ReservationGuard` when dropped.
    pub fn release(&self, tenant_id: DbId, job_id: JobId, bytes: u64) -> Result<(), QuotaError> {
        // Release job quota
        if let Some(tenant) = self.tenants.get(&tenant_id) {
            if let Some(job) = tenant.jobs.get(&job_id) {
                let old_job = job.used.fetch_sub(bytes, Ordering::Release);
                if old_job < bytes {
                    return Err(QuotaError::Underflow {
                        amount: bytes,
                        current: old_job,
                    });
                }
            }

            // Release tenant quota
            let old_tenant = tenant.used.fetch_sub(bytes, Ordering::Release);
            if old_tenant < bytes {
                return Err(QuotaError::Underflow {
                    amount: bytes,
                    current: old_tenant,
                });
            }
        }

        // Release global quota
        let old_global = self.global_used.fetch_sub(bytes, Ordering::Release);
        if old_global < bytes {
            return Err(QuotaError::Underflow {
                amount: bytes,
                current: old_global,
            });
        }

        Ok(())
    }

    /// Gets current global quota usage.
    pub fn global_usage(&self) -> (u64, u64) {
        (
            self.global_used.load(Ordering::Acquire),
            self.config.global_limit,
        )
    }

    /// Gets current tenant quota usage.
    pub fn tenant_usage(&self, tenant_id: DbId) -> Option<(u64, u64)> {
        self.tenants.get(&tenant_id).map(|tenant| {
            (
                tenant.used.load(Ordering::Acquire),
                tenant.limit.load(Ordering::Acquire),
            )
        })
    }

    /// Gets current job quota usage.
    pub fn job_usage(&self, tenant_id: DbId, job_id: JobId) -> Option<(u64, u64)> {
        self.tenants.get(&tenant_id).and_then(|tenant| {
            tenant.jobs.get(&job_id).map(|job| {
                (
                    job.used.load(Ordering::Acquire),
                    job.limit.load(Ordering::Acquire),
                )
            })
        })
    }

    /// Sets tenant quota limit.
    pub fn set_tenant_limit(&self, tenant_id: DbId, limit: u64) {
        let tenant = self
            .tenants
            .entry(tenant_id)
            .or_insert_with(|| Arc::new(TenantQuota::new(limit)))
            .clone();
        tenant.limit.store(limit, Ordering::Release);
    }

    /// Cleans up job quota after job completion.
    pub fn cleanup_job(&self, tenant_id: DbId, job_id: JobId) {
        if let Some(tenant) = self.tenants.get(&tenant_id) {
            tenant.jobs.remove(&job_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_and_release() {
        let config = QuotaConfig {
            global_limit: 1000,
            default_tenant_limit: 500,
            default_job_limit: 100,
        };
        let quota = StorageQuota::new(config);

        // Reserve quota
        let guard = quota.reserve(1, 100, 50).unwrap();
        assert_eq!(quota.global_usage().0, 50);
        assert_eq!(quota.tenant_usage(1).unwrap().0, 50);
        assert_eq!(quota.job_usage(1, 100).unwrap().0, 50);

        // Drop guard to release
        drop(guard);
        assert_eq!(quota.global_usage().0, 0);
        assert_eq!(quota.tenant_usage(1).unwrap().0, 0);
        assert_eq!(quota.job_usage(1, 100).unwrap().0, 0);
    }

    #[test]
    fn global_quota_exhaustion() {
        let config = QuotaConfig {
            global_limit: 100,
            default_tenant_limit: 500,
            default_job_limit: 500,
        };
        let quota = StorageQuota::new(config);

        let _guard1 = quota.reserve(1, 100, 60).unwrap();
        let result = quota.reserve(1, 101, 50);

        assert!(matches!(result, Err(QuotaError::GlobalExhausted { .. })));
    }

    #[test]
    fn tenant_quota_exhaustion() {
        let config = QuotaConfig {
            global_limit: 1000,
            default_tenant_limit: 100,
            default_job_limit: 500,
        };
        let quota = StorageQuota::new(config);

        let _guard1 = quota.reserve(1, 100, 60).unwrap();
        let result = quota.reserve(1, 101, 50);

        assert!(matches!(result, Err(QuotaError::TenantExhausted { .. })));
    }

    #[test]
    fn job_quota_exhaustion() {
        let config = QuotaConfig {
            global_limit: 1000,
            default_tenant_limit: 500,
            default_job_limit: 100,
        };
        let quota = StorageQuota::new(config);

        let _guard1 = quota.reserve(1, 100, 60).unwrap();
        let result = quota.reserve(1, 100, 50);

        assert!(matches!(result, Err(QuotaError::JobExhausted { .. })));
    }

    #[test]
    fn multiple_tenants_and_jobs() {
        let config = QuotaConfig {
            global_limit: 1000,
            default_tenant_limit: 500,
            default_job_limit: 100,
        };
        let quota = StorageQuota::new(config);

        let _g1 = quota.reserve(1, 100, 50).unwrap();
        let _g2 = quota.reserve(1, 101, 40).unwrap();
        let _g3 = quota.reserve(2, 200, 30).unwrap();

        assert_eq!(quota.global_usage().0, 120);
        assert_eq!(quota.tenant_usage(1).unwrap().0, 90);
        assert_eq!(quota.tenant_usage(2).unwrap().0, 30);
    }

    #[test]
    fn concurrent_reservations() {
        use std::thread;

        let config = QuotaConfig {
            global_limit: 10000,
            default_tenant_limit: 5000,
            default_job_limit: 1000,
        };
        let quota = StorageQuota::new(config);

        let mut handles = vec![];
        for i in 0..10 {
            let q = quota.clone();
            let handle = thread::spawn(move || {
                let _guard = q.reserve(1, 100 + i, 100).unwrap();
                thread::sleep(std::time::Duration::from_millis(10));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All released
        assert_eq!(quota.global_usage().0, 0);
    }
}
