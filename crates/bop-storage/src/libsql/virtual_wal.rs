use std::collections::VecDeque;
use std::os::raw::{c_char, c_int, c_longlong, c_uchar, c_uint, c_void};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::page_cache::{
    PageCache, PageCacheKey, PageCacheMetricsSnapshot, allocate_cache_object_id,
};
use dashmap::DashMap;
use libsql_ffi::{
    self, PageHdrIterMut, RefCountedWalManager, SQLITE_ERROR, SQLITE_MISUSE, SQLITE_NOTFOUND,
    SQLITE_OK, clone_wal_manager, destroy_wal_manager, libsql_wal, libsql_wal_manager,
    libsql_wal_methods, make_ref_counted_wal_manager, sqlite3, sqlite3_file, sqlite3_vfs, wal_impl,
    wal_manager_impl,
};
use thiserror::Error;

use super::LibsqlVfs;

/// Configuration parameters for the libsql virtual WAL bridge.
#[derive(Debug, Clone)]
pub struct VirtualWalConfig {
    /// Logical identifier for the WAL hook registration.
    pub name: String,
    /// Page size used when interacting with libsql frames.
    pub page_size: i32,
    /// Optional ceiling for the in-memory frame cache in bytes.
    pub frame_cache_bytes_limit: Option<usize>,
    /// Optional shared page cache used to expose frame payloads to other components.
    pub page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    /// Unique cache object identifier associated with this WAL for global caching.
    pub page_cache_object_id: Option<u64>,
}

impl Default for VirtualWalConfig {
    fn default() -> Self {
        Self {
            name: "bop-virtual-wal".to_string(),
            page_size: 4096,
            frame_cache_bytes_limit: None,
            page_cache: None,
            page_cache_object_id: None,
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
        mut config: VirtualWalConfig,
    ) -> Self {
        if config.page_cache.is_some() && config.page_cache_object_id.is_none() {
            config.page_cache_object_id = Some(allocate_cache_object_id());
        }
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

    /// Return metrics for the shared page cache, when configured.
    pub fn page_cache_metrics(&self) -> Option<PageCacheMetricsSnapshot> {
        self.config.page_cache.as_ref().map(|cache| cache.metrics())
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

    fn cache_limit_bytes(&self) -> Option<usize> {
        self.config.frame_cache_bytes_limit
    }

    fn page_cache(&self) -> Option<Arc<PageCache<PageCacheKey>>> {
        self.config.page_cache.clone()
    }

    fn cache_object_id(&self) -> Option<u64> {
        self.config.page_cache_object_id
    }

    unsafe fn from_raw<'a>(ptr: *mut wal_manager_impl) -> &'a mut WalManagerImpl {
        unsafe { &mut *(ptr as *mut WalManagerImpl) }
    }
}

struct FrameEntry {
    page_no: u32,
    bytes: Arc<[u8]>,
}

impl FrameEntry {
    fn new(page_no: u32, bytes: Arc<[u8]>) -> Self {
        Self { page_no, bytes }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Default)]
struct FrameCacheState {
    order: VecDeque<u32>,
    total_bytes: usize,
}

/// Concrete libsql WAL handle. Writer callbacks are serialized by SQLite,
/// but the internal frame cache relies on lock-free data structures so that
/// read-side lookups can run concurrently with frame ingestion. The
/// `frame_state` mutex only protects eviction bookkeeping.
struct WalImpl {
    hook: Arc<dyn LibsqlWalHook>,
    page_size: i32,
    cache_limit_bytes: Option<usize>,
    page_cache: Option<Arc<PageCache<PageCacheKey>>>,
    page_cache_object_id: Option<u64>,
    frames: DashMap<u32, FrameEntry>,
    page_to_frame: DashMap<u32, u32>,
    frame_state: Mutex<FrameCacheState>,
    next_frame: AtomicU32,
    max_page: AtomicU32,
}

impl WalImpl {
    fn new(
        hook: Arc<dyn LibsqlWalHook>,
        page_size: i32,
        cache_limit_bytes: Option<usize>,
        page_cache: Option<Arc<PageCache<PageCacheKey>>>,
        page_cache_object_id: Option<u64>,
    ) -> Self {
        Self {
            hook,
            page_size,
            cache_limit_bytes,
            page_cache,
            page_cache_object_id,
            frames: DashMap::new(),
            page_to_frame: DashMap::new(),
            frame_state: Mutex::new(FrameCacheState::default()),
            next_frame: AtomicU32::new(0),
            max_page: AtomicU32::new(0),
        }
    }

    fn hook(&self) -> &dyn LibsqlWalHook {
        self.hook.as_ref()
    }

    fn page_size(&self) -> i32 {
        self.page_size
    }

    unsafe fn from_raw<'a>(ptr: *mut wal_impl) -> &'a WalImpl {
        unsafe { &*(ptr as *mut WalImpl) }
    }

    fn page_cache_key(&self, pgno: u32) -> Option<PageCacheKey> {
        self.page_cache_object_id
            .map(|id| PageCacheKey::libsql_wal(id, pgno as u64))
    }

    fn remove_page_cache_entry(&self, pgno: u32) {
        if let (Some(cache), Some(key)) = (self.page_cache.as_ref(), self.page_cache_key(pgno)) {
            cache.remove(&key);
        }
    }

    fn allocate_frame_id(&self) -> u32 {
        self.next_frame.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn store_frame(&self, frame_id: u32, pgno: u32, data: Vec<u8>) {
        let should_cache = match self.cache_limit_bytes {
            Some(limit) if limit == 0 || data.len() > limit => false, // Too large to retain.
            _ => true,
        };

        if should_cache {
            let bytes: Arc<[u8]> = Arc::from(data);
            if let (Some(cache), Some(key)) = (self.page_cache.as_ref(), self.page_cache_key(pgno))
            {
                cache.insert(key, bytes.clone());
            }

            let entry = FrameEntry::new(pgno, bytes);
            let entry_len = entry.len();
            let mut state = self
                .frame_state
                .lock()
                .expect("libsql WAL frame cache state poisoned");

            if let Some(replaced) = self.frames.insert(frame_id, entry) {
                if let Some(position) = state.order.iter().position(|id| *id == frame_id) {
                    state.order.remove(position);
                }
                state.total_bytes = state.total_bytes.saturating_sub(replaced.len());
                if self
                    .page_to_frame
                    .remove_if(&replaced.page_no, |_, frame| *frame == frame_id)
                    .is_some()
                {
                    self.remove_page_cache_entry(replaced.page_no);
                }
            }

            self.page_to_frame.insert(pgno, frame_id);
            state.order.push_back(frame_id);
            state.total_bytes += entry_len;
            self.enforce_limit_locked(&mut state);
        }

        self.bump_max_page(pgno);
    }

    fn bump_max_page(&self, pgno: u32) {
        let mut current = self.max_page.load(Ordering::SeqCst);
        while pgno > current {
            match self
                .max_page
                .compare_exchange(current, pgno, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }

    fn clear(&self) {
        if let (Some(cache), Some(object_id)) =
            (self.page_cache.as_ref(), self.page_cache_object_id)
        {
            let keys: Vec<u32> = self
                .page_to_frame
                .iter()
                .map(|entry| *entry.key())
                .collect();
            for pgno in keys {
                cache.remove(&PageCacheKey::libsql_wal(object_id, pgno as u64));
            }
        }
        self.frames.clear();
        self.page_to_frame.clear();
        if let Ok(mut state) = self.frame_state.lock() {
            state.order.clear();
            state.total_bytes = 0;
        }
        self.next_frame.store(0, Ordering::SeqCst);
        self.max_page.store(0, Ordering::SeqCst);
    }

    fn enforce_limit_locked(&self, state: &mut FrameCacheState) {
        let Some(limit) = self.cache_limit_bytes else {
            return;
        };

        while state.total_bytes > limit {
            let Some(oldest_id) = state.order.pop_front() else {
                break;
            };

            if let Some((_key, evicted)) = self.frames.remove(&oldest_id) {
                state.total_bytes = state.total_bytes.saturating_sub(evicted.len());
                if self
                    .page_to_frame
                    .remove_if(&evicted.page_no, |_, frame| *frame == oldest_id)
                    .is_some()
                {
                    self.remove_page_cache_entry(evicted.page_no);
                }
            } else {
                // Fall back to avoid spinning if the cache dropped this frame elsewhere.
                state.total_bytes = state.total_bytes.min(limit);
            }
        }
        if state.total_bytes > limit {
            state.total_bytes = limit;
        }
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

    let wal = Box::new(WalImpl::new(
        manager.clone_hook(),
        manager.page_size(),
        manager.cache_limit_bytes(),
        manager.page_cache(),
        manager.cache_object_id(),
    ));
    unsafe {
        (*out_wal).methods = wal_methods();
        (*out_wal).pData = Box::into_raw(wal) as *mut wal_impl;
    }
    SQLITE_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use crate::page_cache::{PageCache, PageCacheConfig, PageCacheKey, allocate_cache_object_id};

    #[derive(Default)]
    struct MockWalHook {
        frames: Mutex<Vec<Vec<u8>>>,
        checkpoints: AtomicUsize,
    }

    impl MockWalHook {
        fn frame_count(&self) -> usize {
            self.frames.lock().expect("frame log poisoned").len()
        }

        fn checkpoints(&self) -> usize {
            self.checkpoints.load(Ordering::SeqCst)
        }
    }

    impl LibsqlWalHook for MockWalHook {
        fn on_frame(&self, frame: &[u8]) -> Result<(), LibsqlWalHookError> {
            self.frames
                .lock()
                .expect("frame log poisoned")
                .push(frame.to_vec());
            Ok(())
        }

        fn on_checkpoint(&self) -> Result<(), LibsqlWalHookError> {
            self.checkpoints.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct TestFrame {
        header: Box<libsql_ffi::PgHdr>,
        _data: Vec<u8>,
    }

    struct TestFrameList {
        frames: Vec<TestFrame>,
    }

    #[derive(Clone, Copy)]
    struct WalPtr(*mut libsql_ffi::wal_impl);

    unsafe impl Send for WalPtr {}
    unsafe impl Sync for WalPtr {}

    impl TestFrameList {
        fn new(pages: Vec<(u32, Vec<u8>)>, page_size: usize) -> Self {
            let mut frames: Vec<TestFrame> = pages
                .into_iter()
                .map(|(pgno, payload)| TestFrame::new(pgno, payload, page_size))
                .collect();

            let mut ptrs: Vec<*mut libsql_ffi::PgHdr> = frames
                .iter_mut()
                .map(|frame| frame.header.as_mut() as *mut libsql_ffi::PgHdr)
                .collect();

            for idx in 0..ptrs.len() {
                let next_ptr = ptrs.get(idx + 1).copied().unwrap_or(std::ptr::null_mut());
                unsafe {
                    (*ptrs[idx]).pDirty = next_ptr;
                }
            }

            Self { frames }
        }

        fn head_ptr(&mut self) -> *mut libsql_ffi::libsql_pghdr {
            self.frames
                .first_mut()
                .map(|frame| {
                    frame.header.as_mut() as *mut libsql_ffi::PgHdr as *mut libsql_ffi::libsql_pghdr
                })
                .unwrap_or(std::ptr::null_mut())
        }
    }

    impl TestFrame {
        fn new(pgno: u32, payload: Vec<u8>, page_size: usize) -> Self {
            assert!(payload.len() <= page_size, "payload exceeds page size");
            let mut data = vec![0u8; page_size];
            data[..payload.len()].copy_from_slice(&payload);
            let mut header = Box::new(unsafe { std::mem::zeroed::<libsql_ffi::PgHdr>() });
            header.pgno = pgno;
            header.pData = data.as_mut_ptr() as *mut c_void;
            header.pDirty = std::ptr::null_mut();
            Self {
                header,
                _data: data,
            }
        }
    }

    fn make_wal(hook: Arc<MockWalHook>, page_size: usize, limit: Option<usize>) -> WalPtr {
        make_wal_with_cache(hook, page_size, limit, None, None)
    }

    fn make_wal_with_cache(
        hook: Arc<MockWalHook>,
        page_size: usize,
        limit: Option<usize>,
        cache: Option<Arc<PageCache<PageCacheKey>>>,
        cache_object_id: Option<u64>,
    ) -> WalPtr {
        let hook_trait: Arc<dyn LibsqlWalHook> = hook;
        WalPtr(Box::into_raw(Box::new(WalImpl::new(
            hook_trait,
            page_size as i32,
            limit,
            cache,
            cache_object_id,
        ))) as *mut libsql_ffi::wal_impl)
    }

    unsafe fn drop_wal(ptr: WalPtr) {
        unsafe {
            drop(Box::from_raw(ptr.0 as *mut WalImpl));
        }
    }

    #[test]
    fn frame_cache_roundtrip() {
        const PAGE_SIZE: usize = 64;
        let hook = Arc::new(MockWalHook::default());
        let wal_ptr = make_wal(hook.clone(), PAGE_SIZE, None);

        let mut frames = TestFrameList::new(
            vec![
                (1, vec![0xAA; PAGE_SIZE]),
                (2, vec![0xBB; PAGE_SIZE]),
                (1, vec![0xCC; PAGE_SIZE]),
            ],
            PAGE_SIZE,
        );

        let rc = unsafe {
            wal_x_frames(
                wal_ptr.0,
                0,
                frames.head_ptr(),
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(rc, SQLITE_OK);

        let recorded = hook.frames.lock().expect("frame log poisoned");
        assert_eq!(recorded.len(), 3);
        assert_eq!(recorded[0][0], 0xAA);
        assert_eq!(recorded[1][0], 0xBB);
        assert_eq!(recorded[2][0], 0xCC);
        drop(recorded);

        let mut frame_id: c_uint = 0;
        let rc = unsafe { wal_x_find_frame(wal_ptr.0, 1, &mut frame_id) };
        assert_eq!(rc, SQLITE_OK);
        let mut buf = vec![0u8; PAGE_SIZE];
        let rc =
            unsafe { wal_x_read_frame(wal_ptr.0, frame_id, PAGE_SIZE as c_int, buf.as_mut_ptr()) };
        assert_eq!(rc, SQLITE_OK);
        assert!(buf.iter().all(|&b| b == 0xCC));

        buf.iter_mut().for_each(|b| *b = 0);
        let rc = unsafe { wal_x_read_frame(wal_ptr.0, 1, PAGE_SIZE as c_int, buf.as_mut_ptr()) };
        assert_eq!(rc, SQLITE_OK);
        assert!(buf.iter().all(|&b| b == 0xAA));

        let rc = unsafe { wal_x_find_frame(wal_ptr.0, 2, &mut frame_id) };
        assert_eq!(rc, SQLITE_OK);
        buf.iter_mut().for_each(|b| *b = 0);
        let rc =
            unsafe { wal_x_read_frame(wal_ptr.0, frame_id, PAGE_SIZE as c_int, buf.as_mut_ptr()) };
        assert_eq!(rc, SQLITE_OK);
        assert!(buf.iter().all(|&b| b == 0xBB));

        unsafe { drop_wal(wal_ptr) };
    }

    #[test]
    fn checkpoint_clears_cached_frames() {
        const PAGE_SIZE: usize = 64;
        let hook = Arc::new(MockWalHook::default());
        let wal_ptr = make_wal(hook.clone(), PAGE_SIZE, None);

        for page in 1..=2 {
            let payload = vec![page as u8; PAGE_SIZE];
            let mut frames = TestFrameList::new(vec![(page, payload)], PAGE_SIZE);
            let rc = unsafe {
                wal_x_frames(
                    wal_ptr.0,
                    0,
                    frames.head_ptr(),
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            assert_eq!(rc, SQLITE_OK);
        }

        let rc = unsafe {
            wal_x_checkpoint(
                wal_ptr.0,
                std::ptr::null_mut(),
                0,
                None,
                std::ptr::null_mut(),
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(rc, SQLITE_OK);
        assert_eq!(hook.checkpoints(), 1);

        let mut frame_id: c_uint = 0;
        let rc = unsafe { wal_x_find_frame(wal_ptr.0, 1, &mut frame_id) };
        assert_eq!(rc, SQLITE_NOTFOUND);

        let wal_ref = unsafe { &*(wal_ptr.0 as *mut WalImpl) };
        assert!(wal_ref.frames.is_empty());

        unsafe { drop_wal(wal_ptr) };
    }

    #[test]
    fn parallel_x_frames_calls_do_not_race() {
        const PAGE_SIZE: usize = 32;
        const THREADS: usize = 4;
        const FRAMES_PER_THREAD: usize = 6;

        let hook = Arc::new(MockWalHook::default());
        let wal_ptr = make_wal(hook.clone(), PAGE_SIZE, None);

        let mut handles = Vec::new();
        for t in 0..THREADS {
            let wal_ptr = wal_ptr;
            let raw_ptr = wal_ptr.0 as usize;
            handles.push(thread::spawn(move || {
                let wal_raw = raw_ptr as *mut libsql_ffi::wal_impl;
                for i in 0..FRAMES_PER_THREAD {
                    let page = (t * FRAMES_PER_THREAD + i + 1) as u32;
                    let payload = vec![(page % 255) as u8; PAGE_SIZE];
                    let mut frames = TestFrameList::new(vec![(page, payload)], PAGE_SIZE);
                    let rc = unsafe {
                        wal_x_frames(wal_raw, 0, frames.head_ptr(), 0, 0, 0, std::ptr::null_mut())
                    };
                    assert_eq!(rc, SQLITE_OK);
                }
            }));
        }

        for handle in handles {
            handle.join().expect("parallel writer panicked");
        }

        let expected = (THREADS * FRAMES_PER_THREAD) as u32;
        let wal_ref = unsafe { &*(wal_ptr.0 as *mut WalImpl) };
        assert_eq!(wal_ref.next_frame.load(Ordering::SeqCst), expected);
        assert!(hook.frame_count() >= expected as usize);

        unsafe { drop_wal(wal_ptr) };
    }

    #[test]
    fn cache_eviction_respects_byte_limit() {
        const PAGE_SIZE: usize = 64;
        let hook = Arc::new(MockWalHook::default());
        let limit_bytes = PAGE_SIZE * 2;
        let wal_ptr = make_wal(hook, PAGE_SIZE, Some(limit_bytes));

        for page in 1..=3 {
            let payload = vec![(page % 255) as u8; PAGE_SIZE];
            let mut frames = TestFrameList::new(vec![(page, payload)], PAGE_SIZE);
            let rc = unsafe {
                wal_x_frames(
                    wal_ptr.0,
                    0,
                    frames.head_ptr(),
                    0,
                    0,
                    0,
                    std::ptr::null_mut(),
                )
            };
            assert_eq!(rc, SQLITE_OK);
        }

        let mut frame_id: c_uint = 0;
        let rc = unsafe { wal_x_find_frame(wal_ptr.0, 1, &mut frame_id) };
        assert_eq!(rc, SQLITE_NOTFOUND);

        for page in 2..=3 {
            let rc = unsafe { wal_x_find_frame(wal_ptr.0, page, &mut frame_id) };
            assert_eq!(rc, SQLITE_OK);
        }

        let wal_ref = unsafe { &*(wal_ptr.0 as *mut WalImpl) };
        assert!(!wal_ref.frames.contains_key(&1));
        assert!(wal_ref.frames.contains_key(&2));
        assert!(wal_ref.frames.contains_key(&3));

        let state = wal_ref.frame_state.lock().expect("frame state poisoned");
        assert!(state.total_bytes <= limit_bytes);

        unsafe { drop_wal(wal_ptr) };
    }

    #[test]
    fn frames_populate_global_page_cache() {
        const PAGE_SIZE: usize = 32;
        let hook = Arc::new(MockWalHook::default());
        let cache = Arc::new(PageCache::new(PageCacheConfig {
            capacity_bytes: Some(4096),
        }));
        let cache_id = allocate_cache_object_id();
        let wal_ptr =
            make_wal_with_cache(hook, PAGE_SIZE, None, Some(cache.clone()), Some(cache_id));

        let mut frames = TestFrameList::new(vec![(1, vec![0xAB; PAGE_SIZE])], PAGE_SIZE);
        let rc = unsafe {
            wal_x_frames(
                wal_ptr.0,
                0,
                frames.head_ptr(),
                0,
                0,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(rc, SQLITE_OK);

        let key = PageCacheKey::libsql_wal(cache_id, 1);
        assert!(cache.get(&key).is_some());

        unsafe { drop_wal(wal_ptr) };
    }
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
    wal_ptr: *mut wal_impl,
    pgno: c_uint,
    frame_out: *mut c_uint,
) -> c_int {
    if wal_ptr.is_null() || frame_out.is_null() {
        return SQLITE_MISUSE;
    }

    let wal = unsafe { WalImpl::from_raw(wal_ptr) };
    if let Some(frame) = wal.page_to_frame.get(&(pgno as u32)) {
        unsafe { *frame_out = *frame }
        SQLITE_OK
    } else {
        unsafe { *frame_out = 0 };
        SQLITE_NOTFOUND
    }
}

unsafe extern "C" fn wal_x_read_frame(
    wal_ptr: *mut wal_impl,
    frame_id: c_uint,
    amount: c_int,
    out: *mut c_uchar,
) -> c_int {
    unsafe { wal_read_frame_common(wal_ptr, frame_id, amount, out) }
}

unsafe extern "C" fn wal_x_read_frame_raw(
    wal_ptr: *mut wal_impl,
    frame_id: c_uint,
    amount: c_int,
    out: *mut c_uchar,
) -> c_int {
    unsafe { wal_read_frame_common(wal_ptr, frame_id, amount, out) }
}

unsafe extern "C" fn wal_x_dbsize(_wal: *mut wal_impl) -> c_uint {
    if _wal.is_null() {
        return 0;
    }
    let wal = unsafe { WalImpl::from_raw(_wal) };
    wal.max_page.load(Ordering::SeqCst)
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
    wal_ptr: *mut wal_impl,
    _include_uncommitted: c_int,
    count: *mut c_uint,
) -> c_int {
    if wal_ptr.is_null() {
        return SQLITE_MISUSE;
    }
    let wal = unsafe { WalImpl::from_raw(wal_ptr) };
    if !count.is_null() {
        unsafe { *count = wal.frames.len() as c_uint };
    }
    SQLITE_OK
}

unsafe extern "C" fn wal_x_frames(
    wal_ptr: *mut wal_impl,
    _commit: c_int,
    list: *mut libsql_ffi::libsql_pghdr,
    _max_frame: c_uint,
    _is_ckpt: c_int,
    _sync_flags: c_int,
    _written: *mut c_int,
) -> c_int {
    if wal_ptr.is_null() {
        return SQLITE_MISUSE;
    }

    let wal = unsafe { WalImpl::from_raw(wal_ptr) };

    let mut frames_written: c_int = 0;
    let mut iter = PageHdrIterMut::new(list as *mut libsql_ffi::PgHdr, wal.page_size() as usize);
    while let Some((pgno, frame)) = iter.next() {
        let frame_vec = frame.to_vec();
        if let Err(err) = wal.hook().on_frame(&frame_vec) {
            return hook_error_to_sqlite(err);
        }
        let frame_id = wal.allocate_frame_id();
        wal.store_frame(frame_id, pgno, frame_vec);
        frames_written += 1;
    }

    if !_written.is_null() {
        unsafe { *_written = frames_written };
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
        Ok(()) => {
            wal.clear();
            SQLITE_OK
        }
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

unsafe fn wal_read_frame_common(
    wal_ptr: *mut wal_impl,
    frame_id: c_uint,
    amount: c_int,
    out: *mut c_uchar,
) -> c_int {
    if wal_ptr.is_null() || out.is_null() {
        return SQLITE_MISUSE;
    }

    let wal = unsafe { WalImpl::from_raw(wal_ptr) };
    let Some(frame) = wal.frames.get(&(frame_id as u32)) else {
        return SQLITE_NOTFOUND;
    };

    if amount <= 0 {
        return SQLITE_OK;
    }
    let amount = amount as usize;
    let entry = frame.value();
    let bytes = entry.bytes();
    let copy_len = bytes.len().min(amount);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out, copy_len);
    }
    if copy_len < amount {
        unsafe {
            std::ptr::write_bytes(out.add(copy_len), 0, amount - copy_len);
        }
    }
    SQLITE_OK
}
