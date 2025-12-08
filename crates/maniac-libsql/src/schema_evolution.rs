//! Schema Evolution Manager
//!
//! Provides zero-downtime schema evolution for SQLite databases.
//!
//! # Evolution Flow
//!
//! 1. **Start Evolution**: Fork a target branch from the source branch
//! 2. **Apply DDL**: Execute DDL statements on the target branch
//! 3. **Replay Changesets**: Apply changesets from source that occurred since fork
//! 4. **Cutover**: Stop writes to source, final sync, switch traffic to target
//!
//! # Usage
//!
//! ```ignore
//! let manager = EvolutionManager::new(store, raft_log_manager);
//!
//! // Start evolution with DDL steps
//! let evolution_id = manager.start_evolution(
//!     db_id,
//!     source_branch_id,
//!     "Add user_status column",
//!     vec!["ALTER TABLE users ADD COLUMN status TEXT DEFAULT 'active'"],
//! ).await?;
//!
//! // Apply DDL (called by background worker)
//! manager.apply_ddl(db_id, evolution_id, ddl_executor).await?;
//!
//! // Replay changesets (called repeatedly until caught up)
//! while manager.replay_changesets(db_id, evolution_id, changeset_applier).await? {
//!     // Continue replaying
//! }
//!
//! // When ready, cutover
//! manager.cutover(db_id, evolution_id).await?;
//! ```

use crate::raft_log::BranchRef;
use crate::schema::{Evolution, EvolutionId, EvolutionState, EvolutionStep};
use crate::store::Store;
use anyhow::Result;
use std::future::Future;

/// Trait for executing DDL statements on a branch.
pub trait DdlExecutor: Send + Sync {
    /// Execute a DDL statement on the target branch.
    fn execute_ddl(
        &self,
        branch: BranchRef,
        sql: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Trait for applying changesets from source to target branch.
pub trait ChangesetApplier: Send + Sync {
    /// Apply a changeset to the target branch.
    /// The changeset is identified by its raft log index.
    fn apply_changeset(
        &self,
        source_branch: BranchRef,
        target_branch: BranchRef,
        log_index: u64,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Manages schema evolution lifecycle.
pub struct EvolutionManager {
    store: Store,
}

impl EvolutionManager {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Start a new schema evolution.
    ///
    /// This will:
    /// 1. Fork a new target branch from the source branch
    /// 2. Create an evolution record tracking the migration
    ///
    /// Returns the evolution ID.
    pub fn start_evolution(
        &self,
        db_id: u64,
        source_branch_id: u32,
        name: &str,
        ddl_statements: Vec<String>,
    ) -> Result<EvolutionId> {
        // Get source branch info
        let source_branch = self
            .store
            .get_branch(db_id, source_branch_id)?
            .ok_or_else(|| anyhow::anyhow!("Source branch {} not found", source_branch_id))?;

        // Create target branch (fork from source)
        let target_branch_name = format!("{}_evolution_{}", source_branch.name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0));

        let target_branch_id = self.store.create_branch(
            db_id,
            target_branch_name,
            Some(source_branch_id),
        )?;

        // Create evolution steps
        let steps: Vec<EvolutionStep> = ddl_statements
            .into_iter()
            .enumerate()
            .map(|(i, sql)| EvolutionStep {
                step_number: i as u32,
                ddl_sql: sql,
                applied: false,
                applied_at: 0,
                error: None,
            })
            .collect();

        // Generate evolution ID
        let evolution_id = self.store.next_evolution_id(db_id)?;

        // Create evolution record
        let evolution = Evolution::new(
            evolution_id,
            name.to_string(),
            source_branch_id,
            target_branch_id,
            source_branch.current_head_version,
            steps,
        );

        self.store.create_evolution(db_id, &evolution)?;

        Ok(evolution_id)
    }

    /// Apply DDL statements to the target branch.
    ///
    /// This transitions the evolution from Created to ApplyingDdl to ReplayingChangesets.
    pub async fn apply_ddl<E: DdlExecutor>(
        &self,
        db_id: u64,
        evolution_id: EvolutionId,
        executor: &E,
    ) -> Result<()> {
        let mut evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        if evolution.state != EvolutionState::Created {
            anyhow::bail!(
                "Cannot apply DDL: evolution is in state {:?}, expected Created",
                evolution.state
            );
        }

        // Transition to ApplyingDdl
        evolution.state = EvolutionState::ApplyingDdl;
        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        let target_branch = BranchRef::new(db_id, evolution.target_branch_id);

        // Apply each DDL step
        for i in 0..evolution.steps.len() {
            if evolution.steps[i].applied {
                continue;
            }

            let sql = evolution.steps[i].ddl_sql.clone();
            let step_number = evolution.steps[i].step_number;

            match executor.execute_ddl(target_branch, &sql).await {
                Ok(()) => {
                    evolution.steps[i].applied = true;
                    evolution.steps[i].applied_at = now();
                    evolution.steps[i].error = None;
                }
                Err(e) => {
                    evolution.steps[i].error = Some(e.to_string());
                    evolution.state = EvolutionState::Failed;
                    evolution.error = Some(format!(
                        "DDL step {} failed: {}",
                        step_number, e
                    ));
                    evolution.updated_at = now();
                    self.store.update_evolution(db_id, &evolution)?;
                    return Err(e);
                }
            }

            // Save progress after each step
            evolution.updated_at = now();
            self.store.update_evolution(db_id, &evolution)?;
        }

        // All DDL applied, transition to ReplayingChangesets
        evolution.state = EvolutionState::ReplayingChangesets;
        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        Ok(())
    }

    /// Replay changesets from source branch to target branch.
    ///
    /// Returns true if there are more changesets to replay, false if caught up.
    pub async fn replay_changesets<A: ChangesetApplier>(
        &self,
        db_id: u64,
        evolution_id: EvolutionId,
        applier: &A,
        batch_size: usize,
    ) -> Result<bool> {
        let mut evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        if evolution.state != EvolutionState::ReplayingChangesets {
            anyhow::bail!(
                "Cannot replay changesets: evolution is in state {:?}",
                evolution.state
            );
        }

        let source_branch = BranchRef::new(db_id, evolution.source_branch_id);
        let target_branch = BranchRef::new(db_id, evolution.target_branch_id);

        // Get source branch's current raft state
        let source_raft_state = self.store.get_raft_state(db_id, evolution.source_branch_id)?;
        let source_commit_index = source_raft_state
            .map(|s| s.commit_index)
            .unwrap_or(0);

        // Determine range to replay
        let start_index = evolution.replayed_log_index + 1;
        let end_index = (start_index + batch_size as u64 - 1).min(source_commit_index);

        if start_index > source_commit_index {
            // Caught up! Transition to ReadyForCutover
            evolution.state = EvolutionState::ReadyForCutover;
            evolution.updated_at = now();
            self.store.update_evolution(db_id, &evolution)?;
            return Ok(false);
        }

        // Replay changesets
        for log_index in start_index..=end_index {
            match applier
                .apply_changeset(source_branch, target_branch, log_index)
                .await
            {
                Ok(()) => {
                    evolution.replayed_log_index = log_index;
                }
                Err(e) => {
                    evolution.state = EvolutionState::Failed;
                    evolution.error = Some(format!(
                        "Changeset replay failed at index {}: {}",
                        log_index, e
                    ));
                    evolution.updated_at = now();
                    self.store.update_evolution(db_id, &evolution)?;
                    return Err(e);
                }
            }
        }

        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        // Return whether there's more to replay
        Ok(evolution.replayed_log_index < source_commit_index)
    }

    /// Check if evolution is ready for cutover.
    pub fn is_ready_for_cutover(&self, db_id: u64, evolution_id: EvolutionId) -> Result<bool> {
        let evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        Ok(evolution.state == EvolutionState::ReadyForCutover)
    }

    /// Begin the cutover process.
    ///
    /// This should be called after stopping writes to the source branch.
    /// It will:
    /// 1. Replay any final changesets
    /// 2. Mark the evolution as completed
    ///
    /// The caller is responsible for:
    /// - Stopping writes to source branch before calling this
    /// - Switching traffic to target branch after this returns
    pub async fn cutover<A: ChangesetApplier>(
        &self,
        db_id: u64,
        evolution_id: EvolutionId,
        applier: &A,
    ) -> Result<()> {
        let mut evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        if evolution.state != EvolutionState::ReadyForCutover {
            anyhow::bail!(
                "Cannot cutover: evolution is in state {:?}, expected ReadyForCutover",
                evolution.state
            );
        }

        // Transition to Cutover
        evolution.state = EvolutionState::Cutover;
        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        // Replay any remaining changesets (should be few since writes are stopped)
        while self
            .replay_changesets_internal(db_id, &mut evolution, applier, 100)
            .await?
        {
            // Continue until caught up
        }

        // Mark as completed
        evolution.state = EvolutionState::Completed;
        evolution.completed_at = now();
        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        Ok(())
    }

    /// Internal replay that works on a mutable evolution reference.
    async fn replay_changesets_internal<A: ChangesetApplier>(
        &self,
        db_id: u64,
        evolution: &mut Evolution,
        applier: &A,
        batch_size: usize,
    ) -> Result<bool> {
        let source_branch = BranchRef::new(db_id, evolution.source_branch_id);
        let target_branch = BranchRef::new(db_id, evolution.target_branch_id);

        let source_raft_state = self.store.get_raft_state(db_id, evolution.source_branch_id)?;
        let source_commit_index = source_raft_state
            .map(|s| s.commit_index)
            .unwrap_or(0);

        let start_index = evolution.replayed_log_index + 1;
        let end_index = (start_index + batch_size as u64 - 1).min(source_commit_index);

        if start_index > source_commit_index {
            return Ok(false);
        }

        for log_index in start_index..=end_index {
            applier
                .apply_changeset(source_branch, target_branch, log_index)
                .await?;
            evolution.replayed_log_index = log_index;
        }

        evolution.updated_at = now();
        self.store.update_evolution(db_id, evolution)?;

        Ok(evolution.replayed_log_index < source_commit_index)
    }

    /// Cancel an active evolution.
    pub fn cancel(&self, db_id: u64, evolution_id: EvolutionId) -> Result<()> {
        let mut evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        if evolution.state.is_terminal() {
            anyhow::bail!(
                "Cannot cancel: evolution is already in terminal state {:?}",
                evolution.state
            );
        }

        evolution.state = EvolutionState::Cancelled;
        evolution.updated_at = now();
        self.store.update_evolution(db_id, &evolution)?;

        Ok(())
    }

    /// Get evolution status.
    pub fn get_status(&self, db_id: u64, evolution_id: EvolutionId) -> Result<EvolutionStatus> {
        let evolution = self
            .store
            .get_evolution(db_id, evolution_id)?
            .ok_or_else(|| anyhow::anyhow!("Evolution {} not found", evolution_id))?;

        // Get source branch commit index for progress calculation
        let source_raft_state = self.store.get_raft_state(db_id, evolution.source_branch_id)?;
        let source_commit_index = source_raft_state
            .map(|s| s.commit_index)
            .unwrap_or(0);

        let ddl_progress = if evolution.steps.is_empty() {
            1.0
        } else {
            evolution.steps.iter().filter(|s| s.applied).count() as f64
                / evolution.steps.len() as f64
        };

        let replay_progress = if source_commit_index == 0 {
            1.0
        } else {
            evolution.replayed_log_index as f64 / source_commit_index as f64
        };

        Ok(EvolutionStatus {
            id: evolution.id,
            name: evolution.name.clone(),
            state: evolution.state,
            source_branch_id: evolution.source_branch_id,
            target_branch_id: evolution.target_branch_id,
            ddl_progress,
            ddl_steps_completed: evolution.steps.iter().filter(|s| s.applied).count(),
            ddl_steps_total: evolution.steps.len(),
            replay_progress,
            replayed_log_index: evolution.replayed_log_index,
            source_commit_index,
            error: evolution.error.clone(),
            created_at: evolution.created_at,
            updated_at: evolution.updated_at,
            completed_at: if evolution.completed_at > 0 {
                Some(evolution.completed_at)
            } else {
                None
            },
        })
    }

    /// List all evolutions for a database with status.
    pub fn list_with_status(&self, db_id: u64) -> Result<Vec<EvolutionStatus>> {
        let evolutions = self.store.list_evolutions(db_id)?;
        evolutions
            .into_iter()
            .map(|e| self.get_status(db_id, e.id))
            .collect()
    }
}

/// Status summary for an evolution.
#[derive(Debug, Clone)]
pub struct EvolutionStatus {
    pub id: EvolutionId,
    pub name: String,
    pub state: EvolutionState,
    pub source_branch_id: u32,
    pub target_branch_id: u32,
    /// DDL application progress (0.0 to 1.0)
    pub ddl_progress: f64,
    pub ddl_steps_completed: usize,
    pub ddl_steps_total: usize,
    /// Changeset replay progress (0.0 to 1.0)
    pub replay_progress: f64,
    pub replayed_log_index: u64,
    pub source_commit_index: u64,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub completed_at: Option<u64>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evolution_state_transitions() {
        assert!(EvolutionState::Created.is_active());
        assert!(EvolutionState::ApplyingDdl.is_active());
        assert!(EvolutionState::ReplayingChangesets.is_active());
        assert!(EvolutionState::ReadyForCutover.is_active());
        assert!(EvolutionState::Cutover.is_active());

        assert!(!EvolutionState::Completed.is_active());
        assert!(!EvolutionState::Failed.is_active());
        assert!(!EvolutionState::Cancelled.is_active());

        assert!(EvolutionState::Completed.is_terminal());
        assert!(EvolutionState::Failed.is_terminal());
        assert!(EvolutionState::Cancelled.is_terminal());
    }
}
