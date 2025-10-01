#![allow(dead_code)]

//! Append-only truncation implementation.
//!
//! This module implements T5 from the checkpointing plan: coordinating
//! head/tail truncation for append-only workloads with lease-based
//! checkpoint coordination.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::manifest::{ChunkId, ChunkKey, DbId, Generation, JobId, Manifest};

/// Errors that can occur during truncation operations.
#[derive(Debug, thiserror::Error)]
pub enum TruncationError {
    /// Truncation blocked by active checkpoint lease.
    #[error(
        "truncation blocked by lease on chunk {chunk_id}, generation {generation}, job {job_id}"
    )]
    LeaseBlocked {
        chunk_id: ChunkId,
        generation: Generation,
        job_id: JobId,
    },

    /// Truncation request timed out waiting for leases.
    #[error("truncation timeout after {elapsed:?}")]
    Timeout { elapsed: Duration },

    /// Manifest update failed.
    #[error("manifest error: {0}")]
    Manifest(String),

    /// Invalid truncation bounds.
    #[error("invalid truncation: {0}")]
    Invalid(String),

    /// Chunk is still referenced by snapshots.
    #[error("chunk {chunk_id} referenced by {snapshot_count} snapshots")]
    ChunkReferenced {
        chunk_id: ChunkId,
        snapshot_count: usize,
    },
}

/// Direction for truncation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TruncateDirection {
    /// Remove oldest chunks (head truncation).
    Head,

    /// Remove newest chunks (tail truncation).
    Tail,
}

/// A request to truncate chunks from an append-only database.
///
/// # T5: Append-Only Truncation
///
/// Truncation requests specify the direction (head or tail) and the
/// boundary chunk. The truncation coordinator checks for active leases
/// and waits for them to expire before committing the manifest batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationRequest {
    /// Database to truncate.
    pub db_id: DbId,

    /// Direction of truncation.
    pub direction: TruncateDirection,

    /// Boundary chunk ID.
    ///
    /// - For `Head`: Remove all chunks with `chunk_id < boundary_chunk_id`
    /// - For `Tail`: Remove all chunks with `chunk_id >= boundary_chunk_id`
    pub boundary_chunk_id: ChunkId,

    /// Maximum time to wait for blocking leases.
    pub timeout: Duration,
}

impl TruncationRequest {
    /// Creates a new head truncation request.
    ///
    /// Removes all chunks before (and not including) the boundary.
    pub fn head(db_id: DbId, boundary_chunk_id: ChunkId) -> Self {
        Self {
            db_id,
            direction: TruncateDirection::Head,
            boundary_chunk_id,
            timeout: Duration::from_secs(30),
        }
    }

    /// Creates a new tail truncation request.
    ///
    /// Removes all chunks from (and including) the boundary onward.
    pub fn tail(db_id: DbId, boundary_chunk_id: ChunkId) -> Self {
        Self {
            db_id,
            direction: TruncateDirection::Tail,
            boundary_chunk_id,
            timeout: Duration::from_secs(30),
        }
    }

    /// Sets the timeout for this request.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Checks if a chunk ID would be removed by this truncation.
    pub fn affects_chunk(&self, chunk_id: ChunkId) -> bool {
        match self.direction {
            TruncateDirection::Head => chunk_id < self.boundary_chunk_id,
            TruncateDirection::Tail => chunk_id >= self.boundary_chunk_id,
        }
    }

    /// Returns the list of chunk IDs that would be affected by this truncation.
    ///
    /// # T5b: Truncation Manifest Operations
    ///
    /// Given a list of existing chunks (from manifest query), this method
    /// filters them to produce the set that should be deleted.
    ///
    /// In production, this would query the manifest's `chunk_catalog` directly.
    /// For testing, we accept the chunk list as a parameter.
    pub fn affected_chunks(&self, existing_chunks: &[ChunkId]) -> Vec<ChunkId> {
        existing_chunks
            .iter()
            .copied()
            .filter(|&chunk_id| self.affects_chunk(chunk_id))
            .collect()
    }

    /// Generates the chunk keys for manifest deletion operations.
    ///
    /// # T5b: Manifest Batch Building
    ///
    /// This is used by the truncation executor to build DeleteChunk operations
    /// for submission to the manifest writer.
    pub fn chunk_keys_to_delete(&self, chunk_ids: &[ChunkId]) -> Vec<ChunkKey> {
        chunk_ids
            .iter()
            .map(|&chunk_id| ChunkKey::new(self.db_id, chunk_id))
            .collect()
    }
}

/// A lease on a chunk held by a checkpoint executor.
///
/// # Lease-Based Coordination (T5a)
///
/// Checkpoints register leases when they begin processing a chunk.
/// Truncation operations check for conflicting leases and wait for
/// them to expire before proceeding.
#[derive(Debug, Clone)]
pub struct ChunkLease {
    /// Chunk being processed.
    pub chunk_id: ChunkId,

    /// Generation being written.
    pub generation: Generation,

    /// Job ID holding the lease.
    pub job_id: JobId,

    /// When the lease was acquired.
    pub acquired_at: Instant,

    /// When the lease expires (for timeout handling).
    pub expires_at: Instant,
}

impl ChunkLease {
    /// Creates a new chunk lease.
    pub fn new(chunk_id: ChunkId, generation: Generation, job_id: JobId, ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            chunk_id,
            generation,
            job_id,
            acquired_at: now,
            expires_at: now + ttl,
        }
    }

    /// Checks if this lease has expired.
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    /// Checks if this lease blocks a truncation request.
    pub fn blocks_truncation(&self, request: &TruncationRequest) -> bool {
        request.affects_chunk(self.chunk_id) && !self.is_expired()
    }
}

/// Lease map for coordinating checkpoint execution and truncation.
///
/// # T5a: Lease Map
///
/// The lease map is shared between the checkpoint planner and truncation
/// coordinator. Executors register leases when they begin processing chunks,
/// and release them on completion or cancellation.
#[derive(Debug, Clone)]
pub struct LeaseMap {
    /// Active leases keyed by chunk ID.
    leases: Arc<Mutex<HashMap<ChunkId, ChunkLease>>>,
}

impl LeaseMap {
    /// Creates a new empty lease map.
    pub fn new() -> Self {
        Self {
            leases: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers a lease for a chunk.
    ///
    /// Returns an error if a lease already exists for this chunk.
    #[instrument(skip(self), fields(chunk_id, generation, job_id, ttl_secs = ttl.as_secs()))]
    pub fn register(
        &self,
        chunk_id: ChunkId,
        generation: Generation,
        job_id: JobId,
        ttl: Duration,
    ) -> Result<(), TruncationError> {
        let mut leases = self.leases.lock().unwrap();

        if let Some(existing) = leases.get(&chunk_id) {
            if !existing.is_expired() {
                warn!(
                    existing_generation = existing.generation,
                    existing_job_id = existing.job_id,
                    "lease registration blocked by existing active lease"
                );
                return Err(TruncationError::LeaseBlocked {
                    chunk_id,
                    generation: existing.generation,
                    job_id: existing.job_id,
                });
            }
            debug!("existing lease expired, replacing with new lease");
        }

        let lease = ChunkLease::new(chunk_id, generation, job_id, ttl);
        leases.insert(chunk_id, lease);
        debug!("chunk lease registered successfully");
        Ok(())
    }

    /// Releases a lease for a chunk.
    ///
    /// Idempotent - releasing a non-existent lease is a no-op.
    #[instrument(skip(self))]
    pub fn release(&self, chunk_id: ChunkId) {
        let mut leases = self.leases.lock().unwrap();
        if leases.remove(&chunk_id).is_some() {
            debug!("chunk lease released");
        } else {
            trace!("no lease found to release (already released or never existed)");
        }
    }

    /// Checks if a truncation request is blocked by any active leases.
    ///
    /// Returns the blocking lease if found, or None if truncation can proceed.
    #[instrument(skip(self, request), fields(
        db_id = request.db_id,
        direction = ?request.direction,
        boundary_chunk_id = request.boundary_chunk_id
    ))]
    pub fn check_truncation_blocked(&self, request: &TruncationRequest) -> Option<ChunkLease> {
        let leases = self.leases.lock().unwrap();

        for lease in leases.values() {
            if lease.blocks_truncation(request) {
                debug!(
                    blocking_chunk_id = lease.chunk_id,
                    blocking_generation = lease.generation,
                    blocking_job_id = lease.job_id,
                    "truncation blocked by active lease"
                );
                return Some(lease.clone());
            }
        }

        trace!("no blocking leases found, truncation can proceed");
        None
    }

    /// Waits for truncation to be unblocked, up to the request timeout.
    ///
    /// Returns Ok(()) when truncation can proceed, or an error if the timeout
    /// is reached or another error occurs.
    ///
    /// # T5c: Cancellation Signal
    ///
    /// This method polls the lease map periodically. In a full implementation,
    /// it would also send cancellation signals to blocking jobs via the job queue.
    #[instrument(skip(self, request), fields(
        db_id = request.db_id,
        direction = ?request.direction,
        boundary_chunk_id = request.boundary_chunk_id,
        timeout_secs = request.timeout.as_secs()
    ))]
    pub fn wait_for_truncation(&self, request: &TruncationRequest) -> Result<(), TruncationError> {
        info!("waiting for truncation to be unblocked");
        let start = Instant::now();
        let poll_interval = Duration::from_millis(100);
        let mut poll_count = 0;

        loop {
            // Check if truncation is blocked
            if let Some(_lease) = self.check_truncation_blocked(request) {
                let elapsed = start.elapsed();
                poll_count += 1;

                if elapsed >= request.timeout {
                    error!(
                        elapsed_ms = elapsed.as_millis(),
                        timeout_ms = request.timeout.as_millis(),
                        poll_count,
                        "truncation wait timed out"
                    );
                    return Err(TruncationError::Timeout { elapsed });
                }

                if poll_count % 10 == 0 {
                    debug!(
                        elapsed_ms = elapsed.as_millis(),
                        poll_count,
                        "still waiting for blocking leases to clear"
                    );
                }

                // TODO: T5c - Send cancellation signal to _lease.job_id via job queue

                // Wait and retry
                std::thread::sleep(poll_interval);
            } else {
                // Not blocked - truncation can proceed
                info!(
                    elapsed_ms = start.elapsed().as_millis(),
                    poll_count,
                    "truncation unblocked, can proceed"
                );
                return Ok(());
            }
        }
    }

    /// Removes expired leases from the map.
    ///
    /// Should be called periodically by a janitor task.
    #[instrument(skip(self))]
    pub fn cleanup_expired(&self) {
        let mut leases = self.leases.lock().unwrap();
        let before_count = leases.len();
        leases.retain(|_, lease| !lease.is_expired());
        let after_count = leases.len();
        let removed = before_count - after_count;

        if removed > 0 {
            debug!(removed_count = removed, remaining_count = after_count, "cleaned up expired leases");
        } else {
            trace!("no expired leases to clean up");
        }
    }

    /// Returns the number of active (non-expired) leases.
    pub fn active_count(&self) -> usize {
        let leases = self.leases.lock().unwrap();
        leases.values().filter(|l| !l.is_expired()).count()
    }
}

impl Default for LeaseMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Cancellation callback for sending signals to blocking checkpoint jobs.
///
/// # T5c: Cancellation Signal
///
/// This callback is invoked when a truncation request is blocked by an
/// active checkpoint lease. The implementation should signal the checkpoint
/// job to gracefully abort or expedite completion.
pub type CancellationCallback = Arc<dyn Fn(JobId) + Send + Sync>;

/// Truncation executor that orchestrates the full truncation flow.
///
/// # T5b: Truncation Execution Pipeline
///
/// The executor coordinates:
/// 1. Waiting for lease unblocking via `LeaseMap`
/// 2. Querying manifest for affected chunks
/// 3. Building DeleteChunk/DeleteChunkDelta operations
/// 4. Submitting to manifest writer atomically
pub struct TruncationExecutor {
    lease_map: LeaseMap,
    manifest: Option<Arc<Manifest>>,
    cancellation_callback: Option<CancellationCallback>,
}

impl TruncationExecutor {
    /// Creates a new truncation executor with the given lease map.
    pub fn new(lease_map: LeaseMap) -> Self {
        Self {
            lease_map,
            manifest: None,
            cancellation_callback: None,
        }
    }

    /// Creates a new truncation executor with manifest integration.
    pub fn with_manifest(lease_map: LeaseMap, manifest: Arc<Manifest>) -> Self {
        Self {
            lease_map,
            manifest: Some(manifest),
            cancellation_callback: None,
        }
    }

    /// Sets a cancellation callback for signaling blocking checkpoint jobs.
    ///
    /// # T5c: Cancellation Signal
    ///
    /// The callback is invoked when truncation is blocked by an active
    /// checkpoint lease. The callback receives the blocking job ID and
    /// should signal the job to gracefully abort or expedite completion.
    pub fn set_cancellation_callback(&mut self, callback: CancellationCallback) {
        self.cancellation_callback = Some(callback);
    }

    /// Executes a truncation request.
    ///
    /// # T5b: Full Truncation Flow
    ///
    /// 1. Wait for any blocking checkpoint leases to clear
    /// 2. Query manifest for chunks in the truncation range
    /// 3. Build DeleteChunk operations for each affected chunk
    /// 4. Submit batch to manifest writer
    /// 5. Return the list of deleted chunk IDs
    ///
    /// # Parameters
    ///
    /// - `request`: The truncation request specifying direction and boundary
    /// - `existing_chunks`: List of chunk IDs from manifest (in production,
    ///   this would be queried internally)
    ///
    /// # Returns
    ///
    /// The list of chunk IDs that were deleted, or an error if truncation
    /// was blocked or failed.
    #[instrument(skip(self, existing_chunks), fields(
        db_id = request.db_id,
        direction = ?request.direction,
        boundary_chunk_id = request.boundary_chunk_id,
        existing_chunk_count = existing_chunks.len()
    ))]
    pub fn execute_truncation(
        &self,
        request: &TruncationRequest,
        existing_chunks: &[ChunkId],
    ) -> Result<Vec<ChunkId>, TruncationError> {
        info!("starting truncation execution");

        // Step 1: Wait for blocking leases to clear
        debug!("waiting for blocking leases to clear");
        self.wait_for_truncation(request)?;

        // Step 2: Determine affected chunks
        debug!("determining affected chunks");
        let affected = request.affected_chunks(existing_chunks);

        if affected.is_empty() {
            info!("no chunks affected by truncation, nothing to delete");
            return Ok(vec![]);
        }

        info!(
            affected_count = affected.len(),
            affected_chunks = ?affected,
            "identified chunks for truncation"
        );

        // Step 3: Build chunk keys for deletion
        debug!("building chunk keys for deletion");
        let chunk_keys = request.chunk_keys_to_delete(&affected);

        // Step 4: Submit to manifest writer if available
        if let Some(ref manifest) = self.manifest {
            info!("submitting delete operations to manifest");
            let mut txn = manifest.begin();
            for key in &chunk_keys {
                debug!(chunk_id = key.chunk_id, "deleting chunk from manifest");
                txn.delete_chunk(*key);
                // Note: Deltas are stored separately in chunk_delta table
                // In production, would query delta_index to find all deltas
                // and delete them as well. For now, just delete the base chunk.
            }

            match txn.commit() {
                Ok(_) => {
                    info!(deleted_count = affected.len(), "manifest commit successful");
                }
                Err(e) => {
                    error!(error = %e, "manifest commit failed");
                    return Err(TruncationError::Manifest(e.to_string()));
                }
            }
        } else {
            debug!("no manifest configured, skipping manifest operations (test mode)");
        }
        // If no manifest is set, just validate and return (for testing)

        // Note: S3 deletion will be async (user requirement) and handled
        // by a separate GC job that reads the change_log.

        // Step 5: Return affected chunks
        info!(deleted_count = affected.len(), "truncation execution completed successfully");
        Ok(affected)
    }

    /// Waits for truncation to be unblocked, with optional cancellation signaling.
    ///
    /// # T5c: Cancellation Signal Implementation
    ///
    /// This method polls the lease map and invokes the cancellation callback
    /// when blocked by active leases.
    #[instrument(skip(self, request), fields(
        db_id = request.db_id,
        direction = ?request.direction,
        boundary_chunk_id = request.boundary_chunk_id,
        timeout_secs = request.timeout.as_secs()
    ))]
    fn wait_for_truncation(&self, request: &TruncationRequest) -> Result<(), TruncationError> {
        info!("waiting for truncation to be unblocked with cancellation support");
        let start = Instant::now();
        let poll_interval = Duration::from_millis(100);
        let mut signaled_jobs = std::collections::HashSet::new();
        let mut poll_count = 0;

        loop {
            // Check if truncation is blocked
            if let Some(lease) = self.lease_map.check_truncation_blocked(request) {
                let elapsed = start.elapsed();
                poll_count += 1;

                if elapsed >= request.timeout {
                    error!(
                        elapsed_ms = elapsed.as_millis(),
                        timeout_ms = request.timeout.as_millis(),
                        poll_count,
                        blocking_job_id = lease.job_id,
                        "truncation wait timed out"
                    );
                    return Err(TruncationError::Timeout { elapsed });
                }

                // Send cancellation signal if we haven't already for this job
                if !signaled_jobs.contains(&lease.job_id) {
                    if let Some(ref callback) = self.cancellation_callback {
                        info!(
                            job_id = lease.job_id,
                            chunk_id = lease.chunk_id,
                            "sending cancellation signal to blocking job"
                        );
                        callback(lease.job_id);
                        signaled_jobs.insert(lease.job_id);
                    } else {
                        debug!(
                            job_id = lease.job_id,
                            "no cancellation callback configured, waiting for lease expiry"
                        );
                    }
                }

                if poll_count % 10 == 0 {
                    debug!(
                        elapsed_ms = elapsed.as_millis(),
                        poll_count,
                        signaled_jobs_count = signaled_jobs.len(),
                        "still waiting for blocking leases to clear"
                    );
                }

                // Wait and retry
                std::thread::sleep(poll_interval);
            } else {
                // Not blocked - truncation can proceed
                info!(
                    elapsed_ms = start.elapsed().as_millis(),
                    poll_count,
                    signaled_jobs_count = signaled_jobs.len(),
                    "truncation unblocked, can proceed"
                );
                return Ok(());
            }
        }
    }

    /// Returns a reference to the lease map for coordination.
    pub fn lease_map(&self) -> &LeaseMap {
        &self.lease_map
    }
}

impl Default for TruncationExecutor {
    fn default() -> Self {
        Self::new(LeaseMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_request_affects_chunk() {
        let head_req = TruncationRequest::head(1, 100);
        assert!(head_req.affects_chunk(50));
        assert!(head_req.affects_chunk(99));
        assert!(!head_req.affects_chunk(100));
        assert!(!head_req.affects_chunk(101));

        let tail_req = TruncationRequest::tail(1, 100);
        assert!(!tail_req.affects_chunk(50));
        assert!(!tail_req.affects_chunk(99));
        assert!(tail_req.affects_chunk(100));
        assert!(tail_req.affects_chunk(101));
    }

    #[test]
    fn lease_expiration() {
        let lease = ChunkLease::new(42, 1, 100, Duration::from_millis(50));
        assert!(!lease.is_expired());

        std::thread::sleep(Duration::from_millis(60));
        assert!(lease.is_expired());
    }

    #[test]
    fn lease_map_register_and_release() {
        let map = LeaseMap::new();

        // Register a lease
        map.register(42, 1, 100, Duration::from_secs(10)).unwrap();
        assert_eq!(map.active_count(), 1);

        // Cannot register duplicate
        let err = map
            .register(42, 2, 101, Duration::from_secs(10))
            .unwrap_err();
        assert!(matches!(err, TruncationError::LeaseBlocked { .. }));

        // Release lease
        map.release(42);
        assert_eq!(map.active_count(), 0);

        // Can register again after release
        map.register(42, 2, 101, Duration::from_secs(10)).unwrap();
        assert_eq!(map.active_count(), 1);
    }

    #[test]
    fn lease_map_blocks_truncation() {
        let map = LeaseMap::new();

        // Register lease on chunk 50
        map.register(50, 1, 100, Duration::from_secs(10)).unwrap();

        // Head truncation to 100 is blocked
        let head_req = TruncationRequest::head(1, 100);
        assert!(map.check_truncation_blocked(&head_req).is_some());

        // Head truncation to 50 is not blocked (chunk 50 not affected)
        let head_req = TruncationRequest::head(1, 50);
        assert!(map.check_truncation_blocked(&head_req).is_none());

        // Tail truncation from 50 is blocked
        let tail_req = TruncationRequest::tail(1, 50);
        assert!(map.check_truncation_blocked(&tail_req).is_some());

        // Tail truncation from 51 is not blocked
        let tail_req = TruncationRequest::tail(1, 51);
        assert!(map.check_truncation_blocked(&tail_req).is_none());
    }

    #[test]
    fn lease_map_cleanup_expired() {
        let map = LeaseMap::new();

        // Register short-lived lease
        map.register(42, 1, 100, Duration::from_millis(50)).unwrap();
        assert_eq!(map.active_count(), 1);

        std::thread::sleep(Duration::from_millis(60));

        // Still in map but expired
        assert_eq!(map.active_count(), 0);

        // Cleanup removes it
        map.cleanup_expired();
        assert_eq!(map.active_count(), 0);

        // Can register new lease on same chunk
        map.register(42, 2, 101, Duration::from_secs(10)).unwrap();
        assert_eq!(map.active_count(), 1);
    }

    #[test]
    fn wait_for_truncation_timeout() {
        let map = LeaseMap::new();

        // Register blocking lease
        map.register(50, 1, 100, Duration::from_secs(60)).unwrap();

        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_millis(200));

        let start = Instant::now();
        let result = map.wait_for_truncation(&req);
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(TruncationError::Timeout { .. })));
        assert!(elapsed >= Duration::from_millis(200));
        assert!(elapsed < Duration::from_millis(500)); // Should timeout quickly
    }

    #[test]
    fn wait_for_truncation_succeeds_when_unblocked() {
        let map = LeaseMap::new();

        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_secs(1));

        // No blocking leases - should succeed immediately
        let result = map.wait_for_truncation(&req);
        assert!(result.is_ok());
    }

    // T5b: Truncation manifest integration tests
    #[test]
    fn truncation_request_affected_chunks() {
        let req = TruncationRequest::head(1, 100);
        let existing = vec![50, 75, 99, 100, 150, 200];

        let affected = req.affected_chunks(&existing);
        assert_eq!(affected, vec![50, 75, 99]); // < 100

        let req = TruncationRequest::tail(1, 100);
        let affected = req.affected_chunks(&existing);
        assert_eq!(affected, vec![100, 150, 200]); // >= 100
    }

    #[test]
    fn truncation_request_chunk_keys_to_delete() {
        let req = TruncationRequest::head(5, 100);
        let chunk_ids = vec![10, 20, 30];

        let keys = req.chunk_keys_to_delete(&chunk_ids);
        assert_eq!(keys.len(), 3);

        assert_eq!(keys[0], ChunkKey::new(5, 10));
        assert_eq!(keys[1], ChunkKey::new(5, 20));
        assert_eq!(keys[2], ChunkKey::new(5, 30));
    }

    #[test]
    fn truncation_executor_executes_head_truncation() {
        let executor = TruncationExecutor::default();

        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_secs(1));

        let existing_chunks = vec![50, 75, 99, 100, 150];

        let deleted = executor.execute_truncation(&req, &existing_chunks).unwrap();
        assert_eq!(deleted, vec![50, 75, 99]);
    }

    #[test]
    fn truncation_executor_executes_tail_truncation() {
        let executor = TruncationExecutor::default();

        let req = TruncationRequest::tail(1, 100).with_timeout(Duration::from_secs(1));

        let existing_chunks = vec![50, 75, 99, 100, 150];

        let deleted = executor.execute_truncation(&req, &existing_chunks).unwrap();
        assert_eq!(deleted, vec![100, 150]);
    }

    #[test]
    fn truncation_executor_handles_empty_result() {
        let executor = TruncationExecutor::default();

        // All chunks are >= 100, so head truncation to 100 affects nothing
        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_secs(1));

        let existing_chunks = vec![100, 150, 200];

        let deleted = executor.execute_truncation(&req, &existing_chunks).unwrap();
        assert_eq!(deleted, Vec::<ChunkId>::new());
    }

    #[test]
    fn truncation_executor_waits_for_lease_release() {
        let lease_map = LeaseMap::new();
        let executor = TruncationExecutor::new(lease_map.clone());

        // Register blocking lease
        lease_map
            .register(50, 1, 100, Duration::from_millis(100))
            .unwrap();

        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_millis(200));

        let existing_chunks = vec![50, 75, 99];

        // Should wait for lease to expire
        let start = Instant::now();
        let deleted = executor.execute_truncation(&req, &existing_chunks).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(deleted, vec![50, 75, 99]);
        assert!(elapsed >= Duration::from_millis(100)); // Waited for lease
    }

    #[test]
    fn truncation_executor_fails_on_timeout() {
        let lease_map = LeaseMap::new();
        let executor = TruncationExecutor::new(lease_map.clone());

        // Register long-lived blocking lease
        lease_map
            .register(50, 1, 100, Duration::from_secs(60))
            .unwrap();

        let req = TruncationRequest::head(1, 100).with_timeout(Duration::from_millis(100));

        let existing_chunks = vec![50, 75, 99];

        let result = executor.execute_truncation(&req, &existing_chunks);
        assert!(matches!(result, Err(TruncationError::Timeout { .. })));
    }
}
