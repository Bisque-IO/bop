//! High-level Database API
//!
//! This module provides the main entry point for creating and managing SQLite databases
//! backed by our branched storage engine with Virtual WAL.
//!
//! # Architecture
//!
//! ```text
//! DatabaseManager
//!     |-> Store (metadata in libmdbx)
//!     |-> VirtualWalManager (WAL state per branch)
//!     |-> AsyncChunkStorage (optional, for remote storage)
//!
//! Database
//!     |-> DatabaseManager reference
//!     |-> Database ID (encoded with page_size, chunk_size)
//!
//! Branch
//!     |-> Database reference
//!     |-> Branch ID
//!     |-> Connection pool (libsql connections)
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use maniac_libsql::{DatabaseManager, DatabaseConfig, BranchConfig};
//!
//! // Create a database manager
//! let manager = DatabaseManager::open("/path/to/store")?;
//!
//! // Create a database
//! let db = manager.create_database("mydb", DatabaseConfig::default())?;
//!
//! // Create a branch (or use default "main")
//! let branch = db.create_branch("feature-x", None)?;
//!
//! // Get a connection
//! let conn = branch.connection()?;
//!
//! // Execute queries
//! conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")?;
//! ```

use crate::async_io::{AsyncChunkStorage, SyncAsyncBridge};
use crate::page_manager::ChunkConfig;
use crate::raft_log::BranchRef;
use crate::schema::{BranchInfo, DatabaseInfo};
use crate::store::Store;
use crate::virtual_wal::{NoopAsyncStorage, VirtualWal, VirtualWalManager};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default page size for new databases.
pub const DEFAULT_PAGE_SIZE: u32 = 4096;

/// Default chunk size (64 pages = 256KB with 4KB pages).
pub const DEFAULT_CHUNK_SIZE: u32 = 64 * DEFAULT_PAGE_SIZE;

/// Default pages per chunk.
pub const DEFAULT_PAGES_PER_CHUNK: u32 = 64;

/// Configuration for creating a new database.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// SQLite page size in bytes (must be power of 2, >= 1024).
    pub page_size: u32,
    /// Chunk size in bytes (must be multiple of page_size).
    pub chunk_size: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            page_size: DEFAULT_PAGE_SIZE,
            chunk_size: DEFAULT_CHUNK_SIZE,
        }
    }
}

impl DatabaseConfig {
    /// Create a new database configuration.
    pub fn new(page_size: u32, chunk_size: u32) -> Self {
        Self {
            page_size,
            chunk_size,
        }
    }

    /// Create a configuration with custom page size (chunk size auto-calculated).
    pub fn with_page_size(page_size: u32) -> Self {
        Self {
            page_size,
            chunk_size: page_size * DEFAULT_PAGES_PER_CHUNK,
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        if !self.page_size.is_power_of_two() {
            anyhow::bail!("Page size must be a power of two");
        }
        if self.page_size < 512 {
            anyhow::bail!("Page size must be at least 512 bytes");
        }
        if self.page_size > 65536 {
            anyhow::bail!("Page size must be at most 64KB");
        }
        if self.chunk_size % self.page_size != 0 {
            anyhow::bail!("Chunk size must be a multiple of page size");
        }
        let pages_per_chunk = self.chunk_size / self.page_size;
        if !pages_per_chunk.is_power_of_two() {
            anyhow::bail!("Pages per chunk must be a power of two");
        }
        Ok(())
    }

    /// Get the number of pages per chunk.
    pub fn pages_per_chunk(&self) -> u32 {
        self.chunk_size / self.page_size
    }
}

/// Configuration for chunk storage and compression.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Enable LZ4 compression for chunks.
    pub compression_enabled: bool,
    /// Compression level (1-12).
    pub compression_level: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            compression_enabled: true,
            compression_level: 4,
        }
    }
}

/// The main database manager.
///
/// This is the entry point for all database operations. It manages:
/// - The metadata store (libmdbx)
/// - WAL state for all branches
/// - Optional async storage backend
pub struct DatabaseManager<S: AsyncChunkStorage = NoopAsyncStorage> {
    /// Root path for the database files.
    root_path: PathBuf,
    /// Metadata store.
    store: Store,
    /// Virtual WAL manager.
    wal_manager: Arc<VirtualWalManager<S>>,
    /// Storage configuration.
    #[allow(dead_code)]
    storage_config: StorageConfig,
}

impl DatabaseManager<NoopAsyncStorage> {
    /// Open or create a database manager at the specified path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let root_path = path.as_ref().to_path_buf();
        let store_path = root_path.join("metadata");

        std::fs::create_dir_all(&store_path)?;

        let store = Store::open(&store_path)?;
        let wal_manager = Arc::new(VirtualWalManager::new(store.clone()));

        Ok(Self {
            root_path,
            store,
            wal_manager,
            storage_config: StorageConfig::default(),
        })
    }
}

impl<S: AsyncChunkStorage + Clone + 'static> DatabaseManager<S> {
    /// Open a database manager with a custom async storage backend.
    pub fn open_with_storage<P: AsRef<Path>>(
        path: P,
        async_bridge: SyncAsyncBridge<S>,
        storage_config: StorageConfig,
    ) -> Result<Self> {
        let root_path = path.as_ref().to_path_buf();
        let store_path = root_path.join("metadata");

        std::fs::create_dir_all(&store_path)?;

        let store = Store::open(&store_path)?;

        let chunk_config = ChunkConfig {
            compression_enabled: storage_config.compression_enabled,
            compression_level: storage_config.compression_level,
            encryption_enabled: false,
            encryption_key: None,
        };

        let wal_manager = Arc::new(VirtualWalManager::with_async_storage(
            store.clone(),
            async_bridge,
            chunk_config,
            DEFAULT_PAGES_PER_CHUNK,
        ));

        Ok(Self {
            root_path,
            store,
            wal_manager,
            storage_config,
        })
    }

    /// Get the root path.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get a reference to the store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Get a reference to the WAL manager.
    pub fn wal_manager(&self) -> &Arc<VirtualWalManager<S>> {
        &self.wal_manager
    }

    /// Create a new database.
    pub fn create_database(&self, name: &str, config: DatabaseConfig) -> Result<Database<'_, S>> {
        config.validate()?;

        let db_id = self
            .store
            .create_database(name.to_string(), config.page_size, config.chunk_size)?;

        // Create the default "main" branch
        let branch_id = self.store.create_branch(db_id, "main".to_string(), None)?;

        Ok(Database {
            manager: self,
            db_id,
            default_branch_id: branch_id,
            config,
        })
    }

    /// Open an existing database by name.
    pub fn open_database(&self, db_id: u64) -> Result<Database<'_, S>> {
        let db_info = self
            .store
            .get_database(db_id)?
            .ok_or_else(|| anyhow::anyhow!("Database {} not found", db_id))?;

        let config = DatabaseConfig {
            page_size: db_info.page_size,
            chunk_size: db_info.chunk_size,
        };

        // Find the default branch (first branch, typically "main")
        // For now, we assume branch_id 1 is the default
        let default_branch_id = 1;

        Ok(Database {
            manager: self,
            db_id,
            default_branch_id,
            config,
        })
    }

    /// Get database info.
    pub fn get_database_info(&self, db_id: u64) -> Result<Option<DatabaseInfo>> {
        self.store.get_database(db_id)
    }
}

/// A database instance.
///
/// Represents a single SQLite database with branching support.
pub struct Database<'a, S: AsyncChunkStorage = NoopAsyncStorage> {
    manager: &'a DatabaseManager<S>,
    db_id: u64,
    default_branch_id: u32,
    config: DatabaseConfig,
}

impl<'a, S: AsyncChunkStorage + Clone + 'static> Database<'a, S> {
    /// Get the database ID.
    pub fn id(&self) -> u64 {
        self.db_id
    }

    /// Get the database configuration.
    pub fn config(&self) -> &DatabaseConfig {
        &self.config
    }

    /// Get the default branch.
    pub fn default_branch(&self) -> Result<Branch<'a, S>> {
        self.branch(self.default_branch_id)
    }

    /// Get a branch by ID.
    pub fn branch(&self, branch_id: u32) -> Result<Branch<'a, S>> {
        let branch_info = self
            .manager
            .store
            .get_branch(self.db_id, branch_id)?
            .ok_or_else(|| anyhow::anyhow!("Branch {} not found", branch_id))?;

        Ok(Branch {
            manager: self.manager,
            db_id: self.db_id,
            branch_id,
            branch_info,
            page_size: self.config.page_size,
        })
    }

    /// Create a new branch.
    pub fn create_branch(
        &self,
        name: &str,
        parent_branch_id: Option<u32>,
    ) -> Result<Branch<'a, S>> {
        let branch_id = self.manager.store.create_branch(
            self.db_id,
            name.to_string(),
            parent_branch_id.or(Some(self.default_branch_id)),
        )?;

        self.branch(branch_id)
    }

    /// Get branch info.
    pub fn get_branch_info(&self, branch_id: u32) -> Result<Option<BranchInfo>> {
        self.manager.store.get_branch(self.db_id, branch_id)
    }
}

/// A database branch.
///
/// Represents an isolated branch of a database, with its own WAL state.
pub struct Branch<'a, S: AsyncChunkStorage = NoopAsyncStorage> {
    manager: &'a DatabaseManager<S>,
    db_id: u64,
    branch_id: u32,
    branch_info: BranchInfo,
    page_size: u32,
}

impl<'a, S: AsyncChunkStorage + Clone + 'static> Branch<'a, S> {
    /// Get the database ID.
    pub fn db_id(&self) -> u64 {
        self.db_id
    }

    /// Get the branch ID.
    pub fn branch_id(&self) -> u32 {
        self.branch_id
    }

    /// Get the branch name.
    pub fn name(&self) -> &str {
        &self.branch_info.name
    }

    /// Get the branch info.
    pub fn info(&self) -> &BranchInfo {
        &self.branch_info
    }

    /// Get a BranchRef for this branch.
    pub fn branch_ref(&self) -> BranchRef {
        BranchRef::new(self.db_id, self.branch_id)
    }

    /// Get the database path for this branch.
    ///
    /// This returns a virtual path that encodes the database/branch info
    /// for the VirtualWalManager to parse.
    pub fn db_path(&self) -> String {
        format!("vwal:{}:{}:{}", self.db_id, self.branch_id, self.page_size)
    }

    /// Create a new database connection for this branch.
    ///
    /// This opens a libsql connection using our Virtual WAL.
    #[cfg(feature = "rusqlite")]
    pub fn connection(&self) -> Result<Connection<'a, S>> {
        use maniac_libsql_sys::connection::Connection as LibsqlConnection;

        // Create the database file path
        let db_file_path = self
            .manager
            .root_path
            .join(format!("db_{}", self.db_id))
            .join(format!("branch_{}", self.branch_id))
            .join("main.db");

        // Ensure directory exists
        if let Some(parent) = db_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create an empty database file if it doesn't exist
        if !db_file_path.exists() {
            std::fs::write(&db_file_path, &[])?;
        }

        // Open flags
        let flags = maniac_libsql_sys::connection::OpenFlags::SQLITE_OPEN_READWRITE
            | maniac_libsql_sys::connection::OpenFlags::SQLITE_OPEN_CREATE;

        // Create the connection with our WAL manager
        let wal_manager = (*self.manager.wal_manager).clone();
        let conn = LibsqlConnection::open(
            &db_file_path,
            flags,
            wal_manager,
            0, // auto_checkpoint disabled - we handle checkpointing
            None,
        )?;

        Ok(Connection {
            inner: conn,
            branch: BranchRef::new(self.db_id, self.branch_id),
            _marker: std::marker::PhantomData,
        })
    }

    /// Create a new in-memory connection for this branch.
    ///
    /// Uses the virtual WAL path format that encodes branch info.
    pub fn open_connection_raw(
        &self,
    ) -> Result<VirtualWal<S>> {
        let branch_ref = BranchRef::new(self.db_id, self.branch_id);
        let state = self
            .manager
            .wal_manager
            .get_or_create_state(branch_ref, self.page_size);

        // Generate connection ID
        use std::sync::atomic::{AtomicU64, Ordering};
        static CONN_COUNTER: AtomicU64 = AtomicU64::new(1);
        let conn_id = CONN_COUNTER.fetch_add(1, Ordering::SeqCst);

        Ok(VirtualWal {
            state,
            conn_id,
            in_read_txn: false,
            in_write_txn: false,
            read_mark: 0,
            async_bridge: self.manager.wal_manager.async_bridge().cloned(),
        })
    }
}

/// A database connection.
///
/// Wraps a libsql connection bound to a specific branch.
#[cfg(feature = "rusqlite")]
pub struct Connection<'a, S: AsyncChunkStorage = NoopAsyncStorage> {
    inner: maniac_libsql_sys::connection::Connection<VirtualWal<S>>,
    branch: BranchRef,
    _marker: std::marker::PhantomData<&'a ()>,
}

#[cfg(feature = "rusqlite")]
impl<'a, S: AsyncChunkStorage + 'static> Connection<'a, S> {
    /// Get the branch reference.
    pub fn branch(&self) -> BranchRef {
        self.branch
    }

    /// Get the raw connection handle.
    pub fn raw(&self) -> &maniac_libsql_sys::connection::Connection<VirtualWal<S>> {
        &self.inner
    }

    /// Get a mutable reference to the raw connection.
    pub fn raw_mut(&mut self) -> &mut maniac_libsql_sys::connection::Connection<VirtualWal<S>> {
        &mut self.inner
    }
}

#[cfg(feature = "rusqlite")]
impl<'a, S: AsyncChunkStorage + 'static> std::ops::Deref for Connection<'a, S> {
    type Target = maniac_libsql_sys::connection::Connection<VirtualWal<S>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(feature = "rusqlite")]
impl<'a, S: AsyncChunkStorage + 'static> std::ops::DerefMut for Connection<'a, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Helper struct for creating VirtualWal instances outside of WalManager.
///
/// This is used when we need direct access to the WAL state without
/// going through libsql's connection machinery.
pub struct VirtualWalHandle<S: AsyncChunkStorage = NoopAsyncStorage> {
    /// The WAL instance.
    pub wal: VirtualWal<S>,
    /// The branch reference.
    pub branch: BranchRef,
}

impl<S: AsyncChunkStorage + Clone + 'static> VirtualWalHandle<S> {
    /// Create a new WAL handle for a branch.
    pub fn new(
        wal_manager: &VirtualWalManager<S>,
        db_id: u64,
        branch_id: u32,
        page_size: u32,
    ) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CONN_COUNTER: AtomicU64 = AtomicU64::new(1);

        let branch = BranchRef::new(db_id, branch_id);
        let state = wal_manager.get_or_create_state(branch, page_size);
        let conn_id = CONN_COUNTER.fetch_add(1, Ordering::SeqCst);

        let wal = VirtualWal {
            state,
            conn_id,
            in_read_txn: false,
            in_write_txn: false,
            read_mark: 0,
            async_bridge: wal_manager.async_bridge().cloned(),
        };

        Self { wal, branch }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_config_validation() {
        let config = DatabaseConfig::default();
        assert!(config.validate().is_ok());

        let bad_config = DatabaseConfig::new(1000, 4000); // not power of 2
        assert!(bad_config.validate().is_err());

        let bad_config2 = DatabaseConfig::new(256, 512); // page size too small
        assert!(bad_config2.validate().is_err());
    }

    #[test]
    fn test_database_manager_creation() {
        let tmp = TempDir::new().unwrap();
        let manager = DatabaseManager::open(tmp.path()).unwrap();

        assert!(manager.root_path().exists());
    }

    #[test]
    fn test_create_database_and_branch() {
        let tmp = TempDir::new().unwrap();
        let manager = DatabaseManager::open(tmp.path()).unwrap();

        let db = manager
            .create_database("test_db", DatabaseConfig::default())
            .unwrap();

        assert!(db.id() > 0);
        assert_eq!(db.config().page_size, DEFAULT_PAGE_SIZE);

        // Get default branch
        let main_branch = db.default_branch().unwrap();
        assert_eq!(main_branch.name(), "main");

        // Create a feature branch
        let feature = db.create_branch("feature-x", None).unwrap();
        assert_eq!(feature.name(), "feature-x");
        assert!(feature.info().parent_branch_id.is_some());
    }

    #[test]
    fn test_branch_db_path() {
        let tmp = TempDir::new().unwrap();
        let manager = DatabaseManager::open(tmp.path()).unwrap();
        let db = manager
            .create_database("test", DatabaseConfig::default())
            .unwrap();
        let branch = db.default_branch().unwrap();

        let path = branch.db_path();
        assert!(path.starts_with("vwal:"));
        assert!(path.contains(&format!(":{}", DEFAULT_PAGE_SIZE)));
    }

    #[test]
    fn test_virtual_wal_handle() {
        let tmp = TempDir::new().unwrap();
        let manager = DatabaseManager::open(tmp.path()).unwrap();
        let db = manager
            .create_database("test", DatabaseConfig::default())
            .unwrap();

        let handle = VirtualWalHandle::new(&manager.wal_manager, db.id(), 1, DEFAULT_PAGE_SIZE);

        assert_eq!(handle.branch.db_id, db.id());
        assert_eq!(handle.branch.branch_id, 1);
    }
}
