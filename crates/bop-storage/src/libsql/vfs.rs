use std::ffi::CString;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Mutex;

use libsql_ffi::{self, sqlite3_vfs};
use thiserror::Error;
use tracing::{debug, error, info, instrument, warn};

const DEFAULT_MAX_PATHNAME: i32 = 1024;

/// Configuration options for building a libsql VFS instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibsqlVfsConfig {
    /// Public name used when registering the VFS with libsql.
    pub name: String,
    /// Maximum pathname supported by the VFS implementation.
    pub max_pathname: i32,
    /// Optional directory used for temporary files.
    pub default_temp_directory: Option<PathBuf>,
    /// Optional base VFS to clone. If `None`, the default VFS is used.
    pub base_vfs_name: Option<String>,
}

impl Default for LibsqlVfsConfig {
    fn default() -> Self {
        Self {
            name: "bop-libsql".to_string(),
            max_pathname: DEFAULT_MAX_PATHNAME,
            default_temp_directory: None,
            base_vfs_name: None,
        }
    }
}

/// Builder for [`LibsqlVfs`] instances.
#[derive(Debug, Default, Clone)]
pub struct LibsqlVfsBuilder {
    config: LibsqlVfsConfig,
}

impl LibsqlVfsBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the VFS name used during registration.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.config.name = name.into();
        self
    }

    /// Override the maximum pathname value.
    pub fn max_pathname(mut self, max_pathname: i32) -> Self {
        self.config.max_pathname = max_pathname;
        self
    }

    /// Set a custom temporary directory root.
    pub fn default_temp_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.default_temp_directory = Some(path.into());
        self
    }

    /// Specify the base VFS to clone. When omitted, the default VFS is reused.
    pub fn base_vfs(mut self, name: impl Into<String>) -> Self {
        self.config.base_vfs_name = Some(name.into());
        self
    }

    /// Finalize the builder into a VFS handle.
    pub fn build(self) -> LibsqlVfs {
        LibsqlVfs::new(self.config)
    }
}

/// Custom libsql virtual file system wrapper.
#[derive(Debug)]
pub struct LibsqlVfs {
    config: LibsqlVfsConfig,
    state: Mutex<VfsState>,
}

#[derive(Debug, Default)]
struct VfsState {
    storage: Option<VfsStorage>,
}

impl LibsqlVfs {
    /// Create a new VFS from the provided configuration.
    #[instrument(skip(config), fields(vfs_name = %config.name))]
    pub fn new(config: LibsqlVfsConfig) -> Self {
        info!(
            vfs_name = %config.name,
            max_pathname = config.max_pathname,
            "Creating new libsql VFS instance"
        );
        Self {
            config,
            state: Mutex::new(VfsState::default()),
        }
    }

    /// Convenience method for starting a new builder chain.
    pub fn builder() -> LibsqlVfsBuilder {
        LibsqlVfsBuilder::new()
    }

    /// Retrieve a reference to the configuration backing this VFS.
    pub fn config(&self) -> &LibsqlVfsConfig {
        &self.config
    }

    /// Return whether the VFS has been registered with libsql.
    pub fn is_registered(&self) -> bool {
        self.state
            .lock()
            .expect("libsql VFS state poisoned")
            .storage
            .is_some()
    }

    /// Expose the raw `sqlite3_vfs` pointer once initialization is complete.
    pub fn raw_vfs(&self) -> Option<NonNull<sqlite3_vfs>> {
        self.state
            .lock()
            .expect("libsql VFS state poisoned")
            .storage
            .as_ref()
            .map(|storage| storage.raw_ptr())
    }

    /// Register the VFS with libsql by cloning an existing VFS and adjusting its metadata.
    #[instrument(skip(self), fields(vfs_name = %self.config.name))]
    pub fn register(&self) -> Result<(), LibsqlVfsError> {
        info!(vfs_name = %self.config.name, "Registering libsql VFS");

        let mut state = self.state.lock().expect("libsql VFS state poisoned");
        if state.storage.is_some() {
            warn!(vfs_name = %self.config.name, "VFS already registered");
            return Err(LibsqlVfsError::AlreadyRegistered);
        }

        unsafe {
            let rc = libsql_ffi::sqlite3_initialize();
            if rc != libsql_ffi::SQLITE_OK {
                error!(vfs_name = %self.config.name, code = rc, "Failed to initialize sqlite3");
                return Err(LibsqlVfsError::SqliteError { code: rc });
            }
        }

        let base_name_cstr = match self.config.base_vfs_name.as_ref() {
            Some(name) => {
                debug!(vfs_name = %self.config.name, base_vfs = %name, "Using custom base VFS");
                Some(CString::new(name.as_str()).map_err(|_| LibsqlVfsError::InvalidName)?)
            }
            None => {
                debug!(vfs_name = %self.config.name, "Using default base VFS");
                None
            }
        };

        let base_ptr = unsafe {
            match &base_name_cstr {
                Some(name) => libsql_ffi::sqlite3_vfs_find(name.as_ptr()),
                None => libsql_ffi::sqlite3_vfs_find(std::ptr::null()),
            }
        };

        if base_ptr.is_null() {
            error!(
                vfs_name = %self.config.name,
                base_vfs = ?self.config.base_vfs_name,
                "Base VFS not found"
            );
            return Err(LibsqlVfsError::BaseVfsUnavailable {
                name: self.config.base_vfs_name.clone(),
            });
        }

        let mut storage = VfsStorage::new(base_ptr, &self.config)?;
        let raw_ptr = storage.as_mut_ptr();
        let rc = unsafe { libsql_ffi::sqlite3_vfs_register(raw_ptr, 0 as c_int) };
        if rc != libsql_ffi::SQLITE_OK {
            error!(vfs_name = %self.config.name, code = rc, "Failed to register VFS with sqlite3");
            return Err(LibsqlVfsError::SqliteError { code: rc });
        }

        state.storage = Some(storage);
        info!(vfs_name = %self.config.name, "Successfully registered libsql VFS");
        Ok(())
    }

    /// Remove the VFS from libsql.
    #[instrument(skip(self), fields(vfs_name = %self.config.name))]
    pub fn unregister(&self) -> Result<(), LibsqlVfsError> {
        info!(vfs_name = %self.config.name, "Unregistering libsql VFS");

        let mut state = self.state.lock().expect("libsql VFS state poisoned");
        let mut storage = state.storage.take().ok_or_else(|| {
            warn!(vfs_name = %self.config.name, "VFS not registered");
            LibsqlVfsError::NotRegistered
        })?;
        let raw_ptr = storage.as_mut_ptr();
        let rc = unsafe { libsql_ffi::sqlite3_vfs_unregister(raw_ptr) };
        if rc != libsql_ffi::SQLITE_OK {
            error!(vfs_name = %self.config.name, code = rc, "Failed to unregister VFS");
            state.storage = Some(storage);
            return Err(LibsqlVfsError::SqliteError { code: rc });
        }

        debug!(vfs_name = %self.config.name, "Successfully unregistered libsql VFS");
        Ok(())
    }
}

/// Errors that can be produced by [`LibsqlVfs`].
#[derive(Debug, Error)]
pub enum LibsqlVfsError {
    #[error("libsql VFS is already registered")]
    AlreadyRegistered,
    #[error("libsql VFS is not currently registered")]
    NotRegistered,
    #[error("base VFS {name:?} not found")]
    BaseVfsUnavailable { name: Option<String> },
    #[error("VFS name contains a null byte")]
    InvalidName,
    #[error("libsql returned error code {code}")]
    SqliteError { code: i32 },
}

#[derive(Debug)]
struct VfsStorage {
    _name: CString,
    vfs: Box<sqlite3_vfs>,
}

impl VfsStorage {
    fn new(base_ptr: *mut sqlite3_vfs, config: &LibsqlVfsConfig) -> Result<Self, LibsqlVfsError> {
        let name = CString::new(config.name.clone()).map_err(|_| LibsqlVfsError::InvalidName)?;
        let base_ref = unsafe { &*base_ptr };
        let mut vfs = Box::new(*base_ref);
        vfs.zName = name.as_ptr();
        if config.max_pathname > 0 {
            vfs.mxPathname = config.max_pathname;
        }
        vfs.pNext = std::ptr::null_mut();
        Ok(Self { _name: name, vfs })
    }

    fn raw_ptr(&self) -> NonNull<sqlite3_vfs> {
        NonNull::from(self.vfs.as_ref())
    }

    fn as_mut_ptr(&mut self) -> *mut sqlite3_vfs {
        self.vfs.as_mut()
    }
}
