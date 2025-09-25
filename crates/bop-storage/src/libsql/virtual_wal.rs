use std::os::raw::{c_char, c_int, c_longlong, c_uchar, c_uint, c_void};
use std::ptr::NonNull;
use std::slice;
use std::sync::{Arc, Mutex};

use libsql_ffi::{
    self, RefCountedWalManager, SQLITE_ERROR, SQLITE_MISUSE, SQLITE_NOTFOUND, SQLITE_OK,
    clone_wal_manager, destroy_wal_manager, libsql_pghdr, libsql_wal, libsql_wal_manager,
    libsql_wal_methods, make_ref_counted_wal_manager, sqlite3, sqlite3_file, sqlite3_vfs, wal_impl,
    wal_manager_impl,
};
use thiserror::Error;

use super::LibsqlVfs;

/// Configuration parameters for the libsql virtual WAL bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualWalConfig {
    /// Logical identifier for the WAL hook registration.
    pub name: String,
    /// Page size used when interacting with libsql frames.
    pub page_size: i32,
}

impl Default for VirtualWalConfig {
    fn default() -> Self {
        Self {
            name: "bop-virtual-wal".to_string(),
            page_size: 4096,
        }
    }
}

/// Callback interface used to surface WAL lifecycle events to higher layers.
pub trait LibsqlWalHook: Send + Sync + 'static {
    /// Invoked when the virtual WAL is attached to a libsql connection.
    fn on_open(&self) -> Result<(), LibsqlWalHookError> {
        Ok(())
    }

    /// Invoked for each frame written to the WAL.
    fn on_frame(&self, _frame: &[u8]) -> Result<(), LibsqlWalHookError> {
        Ok(())
    }

    /// Invoked when libsql requests a checkpoint.
    fn on_checkpoint(&self) -> Result<(), LibsqlWalHookError> {
        Ok(())
    }

    /// Invoked before detaching the virtual WAL.
    fn on_close(&self) -> Result<(), LibsqlWalHookError> {
        Ok(())
    }
}

/// Errors returned from [`LibsqlWalHook`] implementations.
#[derive(Debug, Error)]
pub enum LibsqlWalHookError {
    #[error("libsql virtual WAL hook `{operation}` has not been implemented yet")]
    Unimplemented { operation: &'static str },
    #[error("libsql returned error code {code}")]
    SqliteError { code: i32 },
}

/// Errors returned by the virtual WAL facade.
#[derive(Debug, Error)]
pub enum LibsqlVirtualWalError {
    #[error("virtual WAL is already attached")]
    AlreadyAttached,
    #[error("virtual WAL is not attached")]
    NotAttached,
    #[error(transparent)]
    Hook(#[from] LibsqlWalHookError),
    #[error("libsql returned error code {code}")]
    SqliteError { code: i32 },
}

#[derive(Default)]
struct VirtualWalState {
    manager: Option<WalManagerStorage>,
}

/// High-level wrapper managing libsql's virtual WAL API surface.
pub struct LibsqlVirtualWal {
    config: VirtualWalConfig,
    vfs: Arc<LibsqlVfs>,
    hook: Arc<dyn LibsqlWalHook>,
    state: Mutex<VirtualWalState>,
}

impl LibsqlVirtualWal {
    /// Construct a new virtual WAL instance using the supplied pieces.
    pub fn new(
        vfs: Arc<LibsqlVfs>,
        hook: Arc<dyn LibsqlWalHook>,
        config: VirtualWalConfig,
    ) -> Self {
        Self {
            config,
            vfs,
            hook,
            state: Mutex::new(VirtualWalState::default()),
        }
    }

    /// Create a new virtual WAL with default configuration values.
    pub fn with_defaults(vfs: Arc<LibsqlVfs>, hook: Arc<dyn LibsqlWalHook>) -> Self {
        Self::new(vfs, hook, VirtualWalConfig::default())
    }

    /// Access the configuration backing this virtual WAL instance.
    pub fn config(&self) -> &VirtualWalConfig {
        &self.config
    }

    /// Retrieve the associated VFS handle.
    pub fn vfs(&self) -> &LibsqlVfs {
        &self.vfs
    }

    /// Return the hook responsible for reacting to WAL events.
    pub fn hook(&self) -> Arc<dyn LibsqlWalHook> {
        self.hook.clone()
    }

    /// Determine whether the virtual WAL is currently attached to libsql.
    pub fn is_attached(&self) -> bool {
        self.state
            .lock()
            .expect("libsql virtual WAL state poisoned")
            .manager
            .is_some()
    }

    /// Attach the virtual WAL to libsql by creating a reference-counted WAL manager.
    pub fn attach(&self) -> Result<(), LibsqlVirtualWalError> {
        let mut state = self
            .state
            .lock()
            .expect("libsql virtual WAL state poisoned");
        if state.manager.is_some() {
            return Err(LibsqlVirtualWalError::AlreadyAttached);
        }

        let manager_impl = Box::new(WalManagerImpl::new(self.hook.clone(), self.config.clone()));
        let storage = unsafe { WalManagerStorage::create(manager_impl)? };
        state.manager = Some(storage);
        Ok(())
    }

    /// Detach the virtual WAL manager and release its reference in libsql.
    pub fn detach(&self) -> Result<(), LibsqlVirtualWalError> {
        let mut state = self
            .state
            .lock()
            .expect("libsql virtual WAL state poisoned");
        let mut storage = state
            .manager
            .take()
            .ok_or(LibsqlVirtualWalError::NotAttached)?;
        unsafe { storage.release() };
        Ok(())
    }

    /// Perform a checkpoint request initiated by libsql.
    pub fn checkpoint(&self) -> Result<(), LibsqlVirtualWalError> {
        self.hook.on_checkpoint().map_err(Into::into)
    }

    /// Obtain the underlying `RefCountedWalManager` handle, if attached.
    pub fn manager_handle(&self) -> Option<NonNull<RefCountedWalManager>> {
        self.state
            .lock()
            .expect("libsql virtual WAL state poisoned")
            .manager
            .as_ref()
            .map(|storage| storage.handle())
    }

    /// Clone the reference-counted WAL manager, incrementing its ref count.
    pub fn clone_manager_handle(&self) -> Option<NonNull<RefCountedWalManager>> {
        let state = self
            .state
            .lock()
            .expect("libsql virtual WAL state poisoned");
        let storage = state.manager.as_ref()?;
        let cloned = unsafe { clone_wal_manager(storage.handle().as_ptr()) };
        NonNull::new(cloned)
    }
}

struct WalManagerStorage {
    _manager_impl: Box<WalManagerImpl>,
    handle: NonNull<RefCountedWalManager>,
    released: bool,
}

impl WalManagerStorage {
    unsafe fn create(manager_impl: Box<WalManagerImpl>) -> Result<Self, LibsqlVirtualWalError> {
        let mut manager_impl = manager_impl;
        let manager = manager_impl.as_wal_manager();
        let mut handle_ptr: *mut RefCountedWalManager = std::ptr::null_mut();
        let rc = unsafe { make_ref_counted_wal_manager(manager, &mut handle_ptr) };
        if rc != SQLITE_OK {
            return Err(LibsqlVirtualWalError::SqliteError { code: rc });
        }
        let handle = NonNull::new(handle_ptr).ok_or(LibsqlVirtualWalError::SqliteError {
            code: SQLITE_MISUSE,
        })?;

        Ok(Self {
            _manager_impl: manager_impl,
            handle,
            released: false,
        })
    }

    fn handle(&self) -> NonNull<RefCountedWalManager> {
        self.handle
    }

    unsafe fn release(&mut self) {
        if !self.released {
            unsafe { destroy_wal_manager(self.handle.as_ptr()) };
            self.released = true;
        }
    }
}

impl Drop for WalManagerStorage {
    fn drop(&mut self) {
        unsafe { self.release() };
    }
}

#[repr(C)]
struct WalManagerImpl {
    hook: Arc<dyn LibsqlWalHook>,
    config: VirtualWalConfig,
}

impl WalManagerImpl {
    fn new(hook: Arc<dyn LibsqlWalHook>, config: VirtualWalConfig) -> Self {
        Self { hook, config }
    }

    fn as_wal_manager(&mut self) -> libsql_wal_manager {
        libsql_wal_manager {
            bUsesShm: 0,
            xOpen: Some(wal_manager_x_open),
            xClose: Some(wal_manager_x_close),
            xLogDestroy: Some(wal_manager_x_log_destroy),
            xLogExists: Some(wal_manager_x_log_exists),
            xDestroy: Some(wal_manager_x_destroy),
            pData: self as *mut WalManagerImpl as *mut wal_manager_impl,
        }
    }

    fn hook(&self) -> &dyn LibsqlWalHook {
        self.hook.as_ref()
    }

    fn clone_hook(&self) -> Arc<dyn LibsqlWalHook> {
        self.hook.clone()
    }

    fn page_size(&self) -> i32 {
        self.config.page_size
    }

    unsafe fn from_raw<'a>(ptr: *mut wal_manager_impl) -> &'a mut WalManagerImpl {
        unsafe { &mut *(ptr as *mut WalManagerImpl) }
    }
}

#[repr(C)]
struct WalImpl {
    hook: Arc<dyn LibsqlWalHook>,
    page_size: i32,
}

impl WalImpl {
    fn new(hook: Arc<dyn LibsqlWalHook>, page_size: i32) -> Self {
        Self { hook, page_size }
    }

    fn hook(&self) -> &dyn LibsqlWalHook {
        self.hook.as_ref()
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }

    unsafe fn from_raw<'a>(ptr: *mut wal_impl) -> &'a mut WalImpl {
        unsafe { &mut *(ptr as *mut WalImpl) }
    }
}

fn hook_error_to_sqlite(err: LibsqlWalHookError) -> c_int {
    match err {
        LibsqlWalHookError::Unimplemented { .. } => SQLITE_ERROR,
        LibsqlWalHookError::SqliteError { code } => code,
    }
}

fn wal_methods() -> libsql_wal_methods {
    libsql_wal_methods {
        iVersion: 1,
        xLimit: Some(wal_x_limit),
        xBeginReadTransaction: Some(wal_x_begin_read_transaction),
        xEndReadTransaction: Some(wal_x_end_read_transaction),
        xFindFrame: Some(wal_x_find_frame),
        xReadFrame: Some(wal_x_read_frame),
        xReadFrameRaw: Some(wal_x_read_frame_raw),
        xDbsize: Some(wal_x_dbsize),
        xBeginWriteTransaction: Some(wal_x_begin_write_transaction),
        xEndWriteTransaction: Some(wal_x_end_write_transaction),
        xUndo: Some(wal_x_undo),
        xSavepoint: Some(wal_x_savepoint),
        xSavepointUndo: Some(wal_x_savepoint_undo),
        xFrameCount: Some(wal_x_frame_count),
        xFrames: Some(wal_x_frames),
        xCheckpoint: Some(wal_x_checkpoint),
        xCallback: Some(wal_x_callback),
        xExclusiveMode: Some(wal_x_exclusive_mode),
        xHeapMemory: Some(wal_x_heap_memory),
        xSnapshotGet: None,
        xSnapshotOpen: None,
        xSnapshotRecover: None,
        xSnapshotCheck: None,
        xSnapshotUnlock: Some(wal_x_snapshot_unlock),
        xFramesize: Some(wal_x_framesize),
        xFile: None,
        xWriteLock: Some(wal_x_write_lock),
        xDb: Some(wal_x_db),
    }
}

unsafe extern "C" fn wal_manager_x_open(
    manager_ptr: *mut wal_manager_impl,
    _vfs: *mut sqlite3_vfs,
    _db_fd: *mut sqlite3_file,
    _no_shm_mode: c_int,
    _max_size: c_longlong,
    _name: *const c_char,
    out_wal: *mut libsql_wal,
) -> c_int {
    if manager_ptr.is_null() || out_wal.is_null() {
        return SQLITE_MISUSE;
    }

    let manager = unsafe { WalManagerImpl::from_raw(manager_ptr) };
    if let Err(err) = manager.hook().on_open() {
        return hook_error_to_sqlite(err);
    }

    let wal = Box::new(WalImpl::new(manager.clone_hook(), manager.page_size()));
    unsafe {
        (*out_wal).methods = wal_methods();
        (*out_wal).pData = Box::into_raw(wal) as *mut wal_impl;
    }
    SQLITE_OK
}

unsafe extern "C" fn wal_manager_x_close(
    manager_ptr: *mut wal_manager_impl,
    wal_ptr: *mut wal_impl,
    _db: *mut sqlite3,
    _sync_flags: c_int,
    _n_buf: c_int,
    _z_buf: *mut c_uchar,
) -> c_int {
    if wal_ptr.is_null() || manager_ptr.is_null() {
        return SQLITE_MISUSE;
    }

    let _manager = unsafe { WalManagerImpl::from_raw(manager_ptr) };
    let wal = unsafe { Box::from_raw(wal_ptr as *mut WalImpl) };
    let result = match wal.hook().on_close() {
        Ok(()) => SQLITE_OK,
        Err(err) => hook_error_to_sqlite(err),
    };
    drop(wal);
    result
}

unsafe extern "C" fn wal_manager_x_log_destroy(
    _manager_ptr: *mut wal_manager_impl,
    _vfs: *mut sqlite3_vfs,
    _name: *const c_char,
) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_manager_x_log_exists(
    _manager_ptr: *mut wal_manager_impl,
    _vfs: *mut sqlite3_vfs,
    _name: *const c_char,
    exists: *mut c_int,
) -> c_int {
    if !exists.is_null() {
        unsafe { *exists = 0 };
    }
    SQLITE_OK
}

unsafe extern "C" fn wal_manager_x_destroy(_manager_ptr: *mut wal_manager_impl) {}

unsafe extern "C" fn wal_x_limit(_wal: *mut wal_impl, _limit: c_longlong) {}

unsafe extern "C" fn wal_x_begin_read_transaction(
    _wal: *mut wal_impl,
    result: *mut c_int,
) -> c_int {
    if !result.is_null() {
        unsafe { *result = 0 };
    }
    SQLITE_OK
}

unsafe extern "C" fn wal_x_end_read_transaction(_wal: *mut wal_impl) {}

unsafe extern "C" fn wal_x_find_frame(
    _wal: *mut wal_impl,
    _pgno: c_uint,
    _frame: *mut c_uint,
) -> c_int {
    SQLITE_NOTFOUND
}

unsafe extern "C" fn wal_x_read_frame(
    _wal: *mut wal_impl,
    _frame: c_uint,
    _amount: c_int,
    _out: *mut c_uchar,
) -> c_int {
    SQLITE_NOTFOUND
}

unsafe extern "C" fn wal_x_read_frame_raw(
    _wal: *mut wal_impl,
    _frame: c_uint,
    _amount: c_int,
    _out: *mut c_uchar,
) -> c_int {
    SQLITE_NOTFOUND
}

unsafe extern "C" fn wal_x_dbsize(_wal: *mut wal_impl) -> c_uint {
    0
}

unsafe extern "C" fn wal_x_begin_write_transaction(_wal: *mut wal_impl) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_end_write_transaction(_wal: *mut wal_impl) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_undo(
    _wal: *mut wal_impl,
    _undo: Option<unsafe extern "C" fn(*mut c_void, c_uint) -> c_int>,
    _ctx: *mut c_void,
) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_savepoint(_wal: *mut wal_impl, data: *mut c_uint) {
    if !data.is_null() {
        for idx in 0..4 {
            unsafe { *data.add(idx) = 0 };
        }
    }
}

unsafe extern "C" fn wal_x_savepoint_undo(_wal: *mut wal_impl, _data: *mut c_uint) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_frame_count(
    _wal: *mut wal_impl,
    _include_uncommitted: c_int,
    count: *mut c_uint,
) -> c_int {
    if !count.is_null() {
        unsafe { *count = 0 };
    }
    SQLITE_OK
}

unsafe extern "C" fn wal_x_frames(
    wal_ptr: *mut wal_impl,
    _commit: c_int,
    mut list: *mut libsql_pghdr,
    _max_frame: c_uint,
    _is_ckpt: c_int,
    _sync_flags: c_int,
    _written: *mut c_int,
) -> c_int {
    if wal_ptr.is_null() {
        return SQLITE_MISUSE;
    }

    let wal = unsafe { WalImpl::from_raw(wal_ptr) };
    let page_size = wal.page_size() as usize;

    while !list.is_null() {
        let (frame, next) = unsafe {
            let header = &*list;
            let frame = slice::from_raw_parts(header.pData as *const u8, page_size);
            let next = header.pDirty as *mut libsql_pghdr;
            (frame, next)
        };
        if let Err(err) = wal.hook().on_frame(frame) {
            return hook_error_to_sqlite(err);
        }
        list = next;
    }

    SQLITE_OK
}

unsafe extern "C" fn wal_x_checkpoint(
    wal_ptr: *mut wal_impl,
    _db: *mut sqlite3,
    _mode: c_int,
    _busy: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
    _busy_ctx: *mut c_void,
    _sync_flags: c_int,
    _n_buf: c_int,
    _buf: *mut c_uchar,
    _pn_log: *mut c_int,
    _pn_ckpt: *mut c_int,
    _cb: Option<
        unsafe extern "C" fn(*mut c_void, c_int, *const c_uchar, c_int, c_int, c_int) -> c_int,
    >,
    _cb_ctx: *mut c_void,
) -> c_int {
    if wal_ptr.is_null() {
        return SQLITE_MISUSE;
    }

    let wal = unsafe { WalImpl::from_raw(wal_ptr) };
    match wal.hook().on_checkpoint() {
        Ok(()) => SQLITE_OK,
        Err(err) => hook_error_to_sqlite(err),
    }
}

unsafe extern "C" fn wal_x_callback(_wal: *mut wal_impl) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_exclusive_mode(_wal: *mut wal_impl, _op: c_int) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_heap_memory(_wal: *mut wal_impl) -> c_int {
    0
}

unsafe extern "C" fn wal_x_snapshot_unlock(_wal: *mut wal_impl) {}

unsafe extern "C" fn wal_x_framesize(wal_ptr: *mut wal_impl) -> c_int {
    if wal_ptr.is_null() {
        return SQLITE_MISUSE;
    }
    unsafe { WalImpl::from_raw(wal_ptr) }.page_size()
}

unsafe extern "C" fn wal_x_write_lock(_wal: *mut wal_impl, _lock: c_int) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn wal_x_db(_wal: *mut wal_impl, _db: *mut sqlite3) {}
