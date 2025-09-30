//! Scratch directory janitor for cleaning up abandoned checkpoint scratch directories.
//!
//! This module implements T7c from the checkpointing plan: background cleanup
//! of orphaned scratch directories from failed or cancelled checkpoints.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use dashmap::DashMap;
use tokio::time::sleep;

use crate::manifest::JobId;

/// Configuration for the scratch janitor.
#[derive(Debug, Clone)]
pub struct ScratchJanitorConfig {
    /// Base directory for checkpoint scratch files.
    pub scratch_base_dir: PathBuf,

    /// How often to run the janitor (default: 10 minutes).
    pub scan_interval: Duration,

    /// Minimum age before deleting a scratch directory (default: 1 hour).
    pub min_age: Duration,
}

impl Default for ScratchJanitorConfig {
    fn default() -> Self {
        Self {
            scratch_base_dir: PathBuf::from("checkpoint_scratch"),
            scan_interval: Duration::from_secs(600), // 10 minutes
            min_age: Duration::from_secs(3600),      // 1 hour
        }
    }
}

/// Background janitor that cleans up abandoned scratch directories.
///
/// # T7c: Scratch Janitor Implementation
///
/// Periodically scans the checkpoint_scratch/ directory and deletes:
/// - Directories older than `min_age`
/// - Directories for jobs not in the active jobs map
///
/// Active jobs are tracked to prevent deleting scratch directories
/// for in-flight checkpoints.
pub struct ScratchJanitor {
    config: ScratchJanitorConfig,
    active_jobs: DashMap<JobId, SystemTime>,
}

impl ScratchJanitor {
    /// Creates a new scratch janitor.
    pub fn new(config: ScratchJanitorConfig) -> Self {
        Self {
            config,
            active_jobs: DashMap::new(),
        }
    }

    /// Registers a job as active.
    ///
    /// Call this when a checkpoint job starts to prevent its scratch
    /// directory from being cleaned up.
    pub fn register_job(&self, job_id: JobId) {
        self.active_jobs.insert(job_id, SystemTime::now());
    }

    /// Unregisters a job.
    ///
    /// Call this when a checkpoint job completes (success or failure).
    pub fn unregister_job(&self, job_id: JobId) {
        self.active_jobs.remove(&job_id);
    }

    /// Checks if a job is active.
    fn is_job_active(&self, job_id: JobId) -> bool {
        self.active_jobs.contains_key(&job_id)
    }

    /// Runs the janitor loop.
    ///
    /// Scans scratch_base_dir periodically and deletes old directories.
    /// This should be spawned as a tokio task.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let janitor = ScratchJanitor::new(config);
    /// tokio::spawn(async move {
    ///     janitor.run().await;
    /// });
    /// ```
    pub async fn run(&self) {
        loop {
            sleep(self.config.scan_interval).await;

            if let Err(e) = self.scan_and_cleanup() {
                // Log error but continue running
                eprintln!("Scratch janitor error: {}", e);
            }
        }
    }

    /// Scans and cleans up scratch directories.
    ///
    /// Returns the number of directories cleaned up.
    pub fn scan_and_cleanup(&self) -> Result<usize, std::io::Error> {
        let mut cleaned = 0;

        // Ensure scratch base directory exists
        if !self.config.scratch_base_dir.exists() {
            return Ok(0);
        }

        // Scan scratch directories
        for entry in fs::read_dir(&self.config.scratch_base_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // Parse job ID from directory name
            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            let job_id: JobId = match dir_name.parse() {
                Ok(id) => id,
                Err(_) => continue, // Not a valid job ID directory
            };

            // Check if job is active
            if self.is_job_active(job_id) {
                continue;
            }

            // Check directory age
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let modified = match metadata.modified() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let age = match SystemTime::now().duration_since(modified) {
                Ok(d) => d,
                Err(_) => continue,
            };

            if age >= self.config.min_age {
                // Delete the directory
                match fs::remove_dir_all(&path) {
                    Ok(_) => {
                        cleaned += 1;
                        println!("Scratch janitor: cleaned up directory for job {}", job_id);
                    }
                    Err(e) => {
                        eprintln!(
                            "Scratch janitor: failed to delete {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn janitor_cleans_old_directories() {
        let temp = TempDir::new().unwrap();
        let config = ScratchJanitorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            scan_interval: Duration::from_secs(1),
            min_age: Duration::from_millis(100),
        };

        let janitor = ScratchJanitor::new(config);

        // Create some scratch directories
        let old_dir = temp.path().join("100");
        fs::create_dir(&old_dir).unwrap();

        let active_dir = temp.path().join("200");
        fs::create_dir(&active_dir).unwrap();
        janitor.register_job(200);

        // Wait for directories to age
        std::thread::sleep(Duration::from_millis(150));

        // Run cleanup
        let cleaned = janitor.scan_and_cleanup().unwrap();

        // Old directory should be cleaned, active should not
        assert_eq!(cleaned, 1);
        assert!(!old_dir.exists());
        assert!(active_dir.exists());
    }

    #[test]
    fn janitor_respects_min_age() {
        let temp = TempDir::new().unwrap();
        let config = ScratchJanitorConfig {
            scratch_base_dir: temp.path().to_path_buf(),
            scan_interval: Duration::from_secs(1),
            min_age: Duration::from_secs(3600), // 1 hour
        };

        let janitor = ScratchJanitor::new(config);

        // Create a recent directory
        let recent_dir = temp.path().join("100");
        fs::create_dir(&recent_dir).unwrap();

        // Run cleanup immediately
        let cleaned = janitor.scan_and_cleanup().unwrap();

        // Should not be cleaned because it's too recent
        assert_eq!(cleaned, 0);
        assert!(recent_dir.exists());
    }

    #[test]
    fn janitor_handles_missing_directory() {
        let config = ScratchJanitorConfig {
            scratch_base_dir: PathBuf::from("/nonexistent/path"),
            scan_interval: Duration::from_secs(1),
            min_age: Duration::from_secs(1),
        };

        let janitor = ScratchJanitor::new(config);

        // Should not error on missing directory
        let cleaned = janitor.scan_and_cleanup().unwrap();
        assert_eq!(cleaned, 0);
    }
}
