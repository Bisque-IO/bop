use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

use maniac_sys::{
    MDBX_env_flags_MDBX_LIFORECLAIM, MDBX_env_flags_MDBX_NOMEMINIT,
    MDBX_env_flags_MDBX_NOSTICKYTHREADS, MDBX_env_flags_MDBX_NOSUBDIR,
    MDBX_env_flags_MDBX_SYNC_DURABLE, bop_raft_log_store_ptr, bop_raft_logger_ptr,
    bop_raft_mdbx_log_store_open, bop_raft_mdbx_state_mgr_open,
};

use crate::config::ServerConfig;
use crate::error::{RaftError, RaftResult};
use crate::storage::{
    LogStoreBuild, RawLogStore, RawStateManager, StateManagerBuild, StorageBackendKind,
    StorageComponents,
};

const DEFAULT_MDBX_FLAGS: u32 = (MDBX_env_flags_MDBX_NOSTICKYTHREADS
    | MDBX_env_flags_MDBX_NOSUBDIR
    | MDBX_env_flags_MDBX_NOMEMINIT
    | MDBX_env_flags_MDBX_SYNC_DURABLE
    | MDBX_env_flags_MDBX_LIFORECLAIM) as u32;
const DEFAULT_MDBX_MODE: u16 = 0o600;
const DEFAULT_COMPACT_BATCH_SIZE: usize = 1024;

#[derive(Debug, Clone)]
pub struct MdbxGeometry {
    pub size_lower: usize,
    pub size_now: usize,
    pub size_upper: usize,
    pub growth_step: usize,
    pub shrink_threshold: usize,
    pub page_size: usize,
}

impl Default for MdbxGeometry {
    fn default() -> Self {
        Self {
            size_lower: 64 * 1024 * 1024,
            size_now: 256 * 1024 * 1024,
            size_upper: 4 * 1024 * 1024 * 1024,
            growth_step: 64 * 1024 * 1024,
            shrink_threshold: 32 * 1024 * 1024,
            page_size: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MdbxOptions {
    geometry: MdbxGeometry,
    flags: u32,
    mode: u16,
    compact_batch_size: usize,
    data_dir: PathBuf,
}

impl MdbxOptions {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            geometry: MdbxGeometry::default(),
            flags: DEFAULT_MDBX_FLAGS,
            mode: DEFAULT_MDBX_MODE,
            compact_batch_size: DEFAULT_COMPACT_BATCH_SIZE,
            data_dir: data_dir.into(),
        }
    }

    pub fn geometry(&self) -> &MdbxGeometry {
        &self.geometry
    }

    pub fn geometry_mut(&mut self) -> &mut MdbxGeometry {
        &mut self.geometry
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn set_flags(&mut self, flags: u32) {
        self.flags = flags;
    }

    pub fn mode(&self) -> u16 {
        self.mode
    }

    pub fn set_mode(&mut self, mode: u16) {
        self.mode = mode;
    }

    pub fn compact_batch_size(&self) -> usize {
        self.compact_batch_size
    }

    pub fn set_compact_batch_size(&mut self, size: usize) {
        self.compact_batch_size = size;
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn set_data_dir(&mut self, data_dir: impl Into<PathBuf>) {
        self.data_dir = data_dir.into();
    }
}

#[derive(Debug)]
pub struct MdbxStorage {
    server_config: ServerConfig,
    options: MdbxOptions,
}

impl MdbxStorage {
    pub fn new(server_config: ServerConfig, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            server_config,
            options: MdbxOptions::new(data_dir),
        }
    }

    pub fn options(&self) -> &MdbxOptions {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut MdbxOptions {
        &mut self.options
    }

    pub fn into_components(self) -> StorageComponents {
        let MdbxStorage {
            server_config,
            options,
        } = self;
        let log_store = MdbxLogStore {
            options: options.clone(),
        };
        let state_manager = MdbxStateManager {
            options,
            server_config,
        };
        StorageComponents {
            state_manager: StateManagerBuild::Mdbx(state_manager),
            log_store: LogStoreBuild::Mdbx(log_store),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MdbxLogStore {
    options: MdbxOptions,
}

impl MdbxLogStore {
    pub fn into_raw(self, logger_ptr: *mut bop_raft_logger_ptr) -> RaftResult<RawLogStore> {
        let MdbxLogStore { options } = self;
        fs::create_dir_all(options.data_dir()).map_err(|err| {
            RaftError::ConfigError(format!(
                "failed to create MDBX directory {}: {err}",
                options.data_dir().display()
            ))
        })?;
        let path = options.data_dir().join("raft_logs.db");
        let path_c = path_to_cstring(&path)?;
        let bytes = path_c.as_bytes();
        let geom = options.geometry();
        let ptr = unsafe {
            bop_raft_mdbx_log_store_open(
                path_c.as_ptr(),
                bytes.len(),
                logger_ptr,
                geom.size_lower,
                geom.size_now,
                geom.size_upper,
                geom.growth_step,
                geom.shrink_threshold,
                geom.page_size,
                options.flags,
                options.mode,
                options.compact_batch_size,
            )
        };
        if ptr.is_null() {
            return Err(RaftError::LogStoreError(
                "Failed to open MDBX log store; check path and feature flags".into(),
            ));
        }
        unsafe { RawLogStore::from_raw(ptr, StorageBackendKind::Mdbx) }
    }
}

#[derive(Debug)]
pub struct MdbxStateManager {
    options: MdbxOptions,
    server_config: ServerConfig,
}

impl MdbxStateManager {
    pub fn into_raw(
        self,
        logger_ptr: *mut bop_raft_logger_ptr,
        log_store_ptr: *mut bop_raft_log_store_ptr,
    ) -> RaftResult<RawStateManager> {
        let MdbxStateManager {
            options,
            server_config,
        } = self;
        let dir = options.data_dir();
        fs::create_dir_all(dir).map_err(|err| {
            RaftError::ConfigError(format!(
                "failed to create MDBX directory {}: {err}",
                dir.display()
            ))
        })?;
        let dir_c = path_to_cstring(dir)?;
        let bytes = dir_c.as_bytes();
        let geom = options.geometry();
        let ptr = unsafe {
            bop_raft_mdbx_state_mgr_open(
                server_config.as_handle_ptr(),
                dir_c.as_ptr(),
                bytes.len(),
                logger_ptr,
                geom.size_lower,
                geom.size_now,
                geom.size_upper,
                geom.growth_step,
                geom.shrink_threshold,
                geom.page_size,
                options.flags,
                options.mode,
                log_store_ptr,
            )
        };
        if ptr.is_null() {
            return Err(RaftError::ConfigError(
                "Failed to open MDBX state manager; ensure the database is accessible".into(),
            ));
        }
        unsafe { RawStateManager::from_raw(ptr, StorageBackendKind::Mdbx) }
    }
}

fn path_to_cstring(path: &Path) -> RaftResult<CString> {
    let s = path
        .to_str()
        .ok_or_else(|| RaftError::ConfigError(format!("invalid UTF-8 path: {}", path.display())))?;
    CString::new(s).map_err(|_| {
        RaftError::ConfigError(format!(
            "path contains interior NUL bytes: {}",
            path.display()
        ))
    })
}
