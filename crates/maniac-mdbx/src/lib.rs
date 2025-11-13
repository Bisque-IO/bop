//! Ergonomic Rust wrapper around libmdbx (`mdbx_*` bindings via `maniac-sys`).
//!
//! This provides safe(ish) RAII types for environment, transactions, DBI handles,
//! basic CRUD and cursor iteration, with Result-based error handling.

use maniac_sys as sys;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::Path;
// HSR intentionally excluded: no global registry or callbacks are kept here.

// ----- Error handling -----

#[derive(Debug, Clone)]
pub struct Error {
    code: i32,
    message: String,
}

impl Error {
    pub fn code(&self) -> i32 {
        self.code
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MDBX error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

fn mk_error(code: i32) -> Error {
    unsafe {
        let msg_ptr = sys::mdbx_strerror(code as c_int);
        let msg = if msg_ptr.is_null() {
            format!("code {}", code)
        } else {
            CStr::from_ptr(msg_ptr).to_string_lossy().into_owned()
        };
        Error { code, message: msg }
    }
}

fn check(code: i32) -> Result<()> {
    if code == 0 {
        Ok(())
    } else {
        Err(mk_error(code))
    }
}

// ----- Flags & Options -----

bitflags::bitflags! {
    pub struct EnvFlags: i32 {
        const DEFAULTS      = sys::MDBX_env_flags_MDBX_ENV_DEFAULTS as i32;
        const NOSUBDIR      = sys::MDBX_env_flags_MDBX_NOSUBDIR as i32;
        const RDONLY        = sys::MDBX_env_flags_MDBX_RDONLY as i32;
        const EXCLUSIVE     = sys::MDBX_env_flags_MDBX_EXCLUSIVE as i32;
        const WRITEMAP      = sys::MDBX_env_flags_MDBX_WRITEMAP as i32;
        const NOSTICKY      = sys::MDBX_env_flags_MDBX_NOSTICKYTHREADS as i32;
        const NORDAHEAD     = sys::MDBX_env_flags_MDBX_NORDAHEAD as i32;
        const NOMEMINIT     = sys::MDBX_env_flags_MDBX_NOMEMINIT as i32;
        const COALESCE      = sys::MDBX_env_flags_MDBX_COALESCE as i32;
        const LIFORECLAIM   = sys::MDBX_env_flags_MDBX_LIFORECLAIM as i32;
        const PAGEPERTURB   = sys::MDBX_env_flags_MDBX_PAGEPERTURB as i32;
        const SYNC_DURABLE  = sys::MDBX_env_flags_MDBX_SYNC_DURABLE as i32;
        const NOMETASYNC    = sys::MDBX_env_flags_MDBX_NOMETASYNC as i32;
        const SAFE_NOSYNC   = sys::MDBX_env_flags_MDBX_SAFE_NOSYNC as i32;
        const UTTERLY_NOSYNC= sys::MDBX_env_flags_MDBX_UTTERLY_NOSYNC as i32;
        const ACCEDE        = sys::MDBX_env_flags_MDBX_ACCEDE as i32;
    }
}

impl EnvFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_env_flags_t {
        self.bits() as sys::MDBX_env_flags_t
    }
}

bitflags::bitflags! {
    pub struct WarmupFlags: i32 {
        const DEFAULT     = sys::MDBX_warmup_flags_MDBX_warmup_default as i32;
        const FORCE       = sys::MDBX_warmup_flags_MDBX_warmup_force as i32;
        const OOMSAFE     = sys::MDBX_warmup_flags_MDBX_warmup_oomsafe as i32;
        const LOCK        = sys::MDBX_warmup_flags_MDBX_warmup_lock as i32;
        const TOUCHLIMIT  = sys::MDBX_warmup_flags_MDBX_warmup_touchlimit as i32;
        const RELEASE     = sys::MDBX_warmup_flags_MDBX_warmup_release as i32;
    }
}

impl WarmupFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_warmup_flags_t {
        self.bits() as sys::MDBX_warmup_flags_t
    }
}

bitflags::bitflags! {
    pub struct DbFlags: i32 {
        const DEFAULTS   = sys::MDBX_db_flags_MDBX_DB_DEFAULTS as i32;
        const REVERSEKEY = sys::MDBX_db_flags_MDBX_REVERSEKEY as i32;
        const DUPSORT    = sys::MDBX_db_flags_MDBX_DUPSORT as i32;
        const INTEGERKEY = sys::MDBX_db_flags_MDBX_INTEGERKEY as i32;
        const DUPFIXED   = sys::MDBX_db_flags_MDBX_DUPFIXED as i32;
        const INTEGERDUP = sys::MDBX_db_flags_MDBX_INTEGERDUP as i32;
        const REVERSEDUP = sys::MDBX_db_flags_MDBX_REVERSEDUP as i32;
        const CREATE     = sys::MDBX_db_flags_MDBX_CREATE as i32;
        const DB_ACCEDE  = sys::MDBX_db_flags_MDBX_DB_ACCEDE as i32;
    }
}

impl DbFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_db_flags_t {
        self.bits() as sys::MDBX_db_flags_t
    }
}

bitflags::bitflags! {
    pub struct PutFlags: i32 {
        const UPSERT      = sys::MDBX_put_flags_MDBX_UPSERT as i32;
        const NOOVERWRITE = sys::MDBX_put_flags_MDBX_NOOVERWRITE as i32;
        const NODUPDATA   = sys::MDBX_put_flags_MDBX_NODUPDATA as i32;
        const CURRENT     = sys::MDBX_put_flags_MDBX_CURRENT as i32;
        const ALLDUPS     = sys::MDBX_put_flags_MDBX_ALLDUPS as i32;
        const RESERVE     = sys::MDBX_put_flags_MDBX_RESERVE as i32;
        const APPEND      = sys::MDBX_put_flags_MDBX_APPEND as i32;
        const APPENDDUP   = sys::MDBX_put_flags_MDBX_APPENDDUP as i32;
        const MULTIPLE    = sys::MDBX_put_flags_MDBX_MULTIPLE as i32;
    }
}

impl PutFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_put_flags_t {
        self.bits() as sys::MDBX_put_flags_t
    }
}

/// Cursor operations for MDBX database navigation.
///
/// This enum provides a Rust-friendly wrapper around the MDBX_cursor_op constants
/// used for cursor positioning and data retrieval operations.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum CursorOp {
    /// Position at first key/data item
    First = sys::MDBX_cursor_op_MDBX_FIRST as i32,
    /// MDBX_DUPSORT-only: Position at first data item of current key
    FirstDup = sys::MDBX_cursor_op_MDBX_FIRST_DUP as i32,
    /// MDBX_DUPSORT-only: Position at key/data pair
    GetBoth = sys::MDBX_cursor_op_MDBX_GET_BOTH as i32,
    /// MDBX_DUPSORT-only: Position at given key and at first data greater than or equal to specified data
    GetBothRange = sys::MDBX_cursor_op_MDBX_GET_BOTH_RANGE as i32,
    /// Return key/data at current cursor position
    GetCurrent = sys::MDBX_cursor_op_MDBX_GET_CURRENT as i32,
    /// MDBX_DUPFIXED-only: Return up to a page of duplicate data items from current cursor position
    GetMultiple = sys::MDBX_cursor_op_MDBX_GET_MULTIPLE as i32,
    /// Position at last key/data item
    Last = sys::MDBX_cursor_op_MDBX_LAST as i32,
    /// MDBX_DUPSORT-only: Position at last data item of current key
    LastDup = sys::MDBX_cursor_op_MDBX_LAST_DUP as i32,
    /// Position at next data item
    Next = sys::MDBX_cursor_op_MDBX_NEXT as i32,
    /// MDBX_DUPSORT-only: Position at next data item of current key
    NextDup = sys::MDBX_cursor_op_MDBX_NEXT_DUP as i32,
    /// MDBX_DUPFIXED-only: Return up to a page of duplicate data items from next cursor position
    NextMultiple = sys::MDBX_cursor_op_MDBX_NEXT_MULTIPLE as i32,
    /// Position at first data item of next key
    NextNoDup = sys::MDBX_cursor_op_MDBX_NEXT_NODUP as i32,
    /// Position at previous data item
    Prev = sys::MDBX_cursor_op_MDBX_PREV as i32,
    /// MDBX_DUPSORT-only: Position at previous data item of current key
    PrevDup = sys::MDBX_cursor_op_MDBX_PREV_DUP as i32,
    /// Position at last data item of previous key
    PrevNoDup = sys::MDBX_cursor_op_MDBX_PREV_NODUP as i32,
    /// Position at specified key
    Set = sys::MDBX_cursor_op_MDBX_SET as i32,
    /// Position at specified key, return both key and data
    SetKey = sys::MDBX_cursor_op_MDBX_SET_KEY as i32,
    /// Position at first key greater than or equal to specified key
    SetRange = sys::MDBX_cursor_op_MDBX_SET_RANGE as i32,
    /// MDBX_DUPFIXED-only: Position at previous page and return up to a page of duplicate data items
    PrevMultiple = sys::MDBX_cursor_op_MDBX_PREV_MULTIPLE as i32,
    /// Positions cursor at first key-value pair greater than or equal to specified
    SetLowerBound = sys::MDBX_cursor_op_MDBX_SET_LOWERBOUND as i32,
    /// Positions cursor at first key-value pair greater than specified
    SetUpperBound = sys::MDBX_cursor_op_MDBX_SET_UPPERBOUND as i32,
    /// Doubtless cursor positioning at a specified key (lesser than)
    ToKeyLesserThan = sys::MDBX_cursor_op_MDBX_TO_KEY_LESSER_THAN as i32,
    /// Doubtless cursor positioning at a specified key (lesser or equal)
    ToKeyLesserOrEqual = sys::MDBX_cursor_op_MDBX_TO_KEY_LESSER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key (equal)
    ToKeyEqual = sys::MDBX_cursor_op_MDBX_TO_KEY_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key (greater or equal)
    ToKeyGreaterOrEqual = sys::MDBX_cursor_op_MDBX_TO_KEY_GREATER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key (greater than)
    ToKeyGreaterThan = sys::MDBX_cursor_op_MDBX_TO_KEY_GREATER_THAN as i32,
    /// Doubtless cursor positioning at a specified key-value pair (lesser than)
    ToExactKeyValueLesserThan = sys::MDBX_cursor_op_MDBX_TO_EXACT_KEY_VALUE_LESSER_THAN as i32,
    /// Doubtless cursor positioning at a specified key-value pair (lesser or equal)
    ToExactKeyValueLesserOrEqual =
        sys::MDBX_cursor_op_MDBX_TO_EXACT_KEY_VALUE_LESSER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (equal)
    ToExactKeyValueEqual = sys::MDBX_cursor_op_MDBX_TO_EXACT_KEY_VALUE_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (greater or equal)
    ToExactKeyValueGreaterOrEqual =
        sys::MDBX_cursor_op_MDBX_TO_EXACT_KEY_VALUE_GREATER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (greater than)
    ToExactKeyValueGreaterThan = sys::MDBX_cursor_op_MDBX_TO_EXACT_KEY_VALUE_GREATER_THAN as i32,
    /// Doubtless cursor positioning at a specified key-value pair (lesser than)
    ToPairLesserThan = sys::MDBX_cursor_op_MDBX_TO_PAIR_LESSER_THAN as i32,
    /// Doubtless cursor positioning at a specified key-value pair (lesser or equal)
    ToPairLesserOrEqual = sys::MDBX_cursor_op_MDBX_TO_PAIR_LESSER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (equal)
    ToPairEqual = sys::MDBX_cursor_op_MDBX_TO_PAIR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (greater or equal)
    ToPairGreaterOrEqual = sys::MDBX_cursor_op_MDBX_TO_PAIR_GREATER_OR_EQUAL as i32,
    /// Doubtless cursor positioning at a specified key-value pair (greater than)
    ToPairGreaterThan = sys::MDBX_cursor_op_MDBX_TO_PAIR_GREATER_THAN as i32,
    /// MDBX_DUPFIXED-only: Seek to given key and return up to a page of duplicate data items
    SeekAndGetMultiple = sys::MDBX_cursor_op_MDBX_SEEK_AND_GET_MULTIPLE as i32,
}

impl CursorOp {
    /// Convert the enum variant to the raw MDBX_cursor_op value
    pub fn to_raw(self) -> sys::MDBX_cursor_op {
        self as i32 as sys::MDBX_cursor_op
    }
}

/// Error codes for MDBX database operations.
///
/// This enum provides a Rust-friendly wrapper around the MDBX_error constants
/// used for error handling in MDBX operations.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    /// Successful result
    Success = sys::MDBX_error_MDBX_SUCCESS as i32,
    /// Alias for MDBX_SUCCESS
    // ResultFalse = sys::MDBX_error_MDBX_RESULT_FALSE as i32,
    /// Successful result with special meaning or a flag
    ResultTrue = sys::MDBX_error_MDBX_RESULT_TRUE as i32,
    /// key/data pair already exists
    KeyExist = sys::MDBX_error_MDBX_KEYEXIST as i32,
    /// key/data pair not found (EOF)
    NotFound = sys::MDBX_error_MDBX_NOTFOUND as i32,
    /// Requested page not found - this usually indicates corruption
    PageNotFound = sys::MDBX_error_MDBX_PAGE_NOTFOUND as i32,
    /// Database is corrupted (page was wrong type and so on)
    Corrupted = sys::MDBX_error_MDBX_CORRUPTED as i32,
    /// Environment had fatal error, i.e. update of meta page failed and so on
    Panic = sys::MDBX_error_MDBX_PANIC as i32,
    /// DB file version mismatch with libmdbx
    VersionMismatch = sys::MDBX_error_MDBX_VERSION_MISMATCH as i32,
    /// File is not a valid MDBX file
    Invalid = sys::MDBX_error_MDBX_INVALID as i32,
    /// Environment mapsize reached
    MapFull = sys::MDBX_error_MDBX_MAP_FULL as i32,
    /// Environment maxdbs reached
    DbsFull = sys::MDBX_error_MDBX_DBS_FULL as i32,
    /// Environment maxreaders reached
    ReadersFull = sys::MDBX_error_MDBX_READERS_FULL as i32,
    /// Transaction has too many dirty pages, i.e transaction too big
    TxnFull = sys::MDBX_error_MDBX_TXN_FULL as i32,
    /// Cursor stack too deep - this usually indicates corruption, i.e branch-pages loop
    CursorFull = sys::MDBX_error_MDBX_CURSOR_FULL as i32,
    /// Page has not enough space - internal error
    PageFull = sys::MDBX_error_MDBX_PAGE_FULL as i32,
    /// Database engine was unable to extend mapping
    UnableExtendMapsize = sys::MDBX_error_MDBX_UNABLE_EXTEND_MAPSIZE as i32,
    /// Environment or table is not compatible with the requested operation or the specified flags
    Incompatible = sys::MDBX_error_MDBX_INCOMPATIBLE as i32,
    /// Invalid reuse of reader locktable slot, e.g. read-transaction already run for current thread
    BadRslot = sys::MDBX_error_MDBX_BAD_RSLOT as i32,
    /// Transaction is not valid for requested operation
    BadTxn = sys::MDBX_error_MDBX_BAD_TXN as i32,
    /// Invalid size or alignment of key or data for target table, either invalid table name
    BadValsize = sys::MDBX_error_MDBX_BAD_VALSIZE as i32,
    /// The specified DBI-handle is invalid or changed by another thread/transaction
    BadDbi = sys::MDBX_error_MDBX_BAD_DBI as i32,
    /// Unexpected internal error, transaction should be aborted
    Problem = sys::MDBX_error_MDBX_PROBLEM as i32,
    /// Another write transaction is running or environment is already used while opening with MDBX_EXCLUSIVE flag
    Busy = sys::MDBX_error_MDBX_BUSY as i32,
    /// The specified key has more than one associated value
    Emultival = sys::MDBX_error_MDBX_EMULTIVAL as i32,
    /// Bad signature of a runtime object(s)
    Ebadsign = sys::MDBX_error_MDBX_EBADSIGN as i32,
    /// Database should be recovered, but this could NOT be done for now since it opened in read-only mode
    WannaRecovery = sys::MDBX_error_MDBX_WANNA_RECOVERY as i32,
    /// The given key value is mismatched to the current cursor position
    Ekeymismatch = sys::MDBX_error_MDBX_EKEYMISMATCH as i32,
    /// Database is too large for current system, e.g. could NOT be mapped into RAM
    TooLarge = sys::MDBX_error_MDBX_TOO_LARGE as i32,
    /// A thread has attempted to use a not owned object, e.g. a transaction that started by another thread
    ThreadMismatch = sys::MDBX_error_MDBX_THREAD_MISMATCH as i32,
    /// Overlapping read and write transactions for the current thread
    TxnOverlapping = sys::MDBX_error_MDBX_TXN_OVERLAPPING as i32,
    /// Internal error for debugging - backlog depleted
    BacklogDepleted = sys::MDBX_error_MDBX_BACKLOG_DEPLETED as i32,
    /// Alternative/Duplicate LCK-file exists and should be removed manually
    DuplicatedClk = sys::MDBX_error_MDBX_DUPLICATED_CLK as i32,
    /// Some cursors and/or other resources should be closed before table or corresponding DBI-handle could be (re)used and/or closed
    DanglingDbi = sys::MDBX_error_MDBX_DANGLING_DBI as i32,
    /// The parked read transaction was outed for the sake of recycling old MVCC snapshots
    Ousted = sys::MDBX_error_MDBX_OUSTED as i32,
    /// MVCC snapshot used by parked transaction was bygone
    MvccRetarded = sys::MDBX_error_MDBX_MVCC_RETARDED as i32,
    /// No data available
    Enodata = sys::MDBX_error_MDBX_ENODATA as i32,
    /// Invalid argument
    Einval = sys::MDBX_error_MDBX_EINVAL as i32,
    /// Permission denied
    Eaccess = sys::MDBX_error_MDBX_EACCESS as i32,
    /// Out of memory
    Enomem = sys::MDBX_error_MDBX_ENOMEM as i32,
    /// Read-only file system
    Erofs = sys::MDBX_error_MDBX_EROFS as i32,
    /// Function not implemented
    Enosys = sys::MDBX_error_MDBX_ENOSYS as i32,
    /// I/O error
    Eio = sys::MDBX_error_MDBX_EIO as i32,
    /// Operation not permitted
    Eperm = sys::MDBX_error_MDBX_EPERM as i32,
    /// Interrupted system call
    Eintr = sys::MDBX_error_MDBX_EINTR as i32,
    /// No such file or directory
    EnoFile = sys::MDBX_error_MDBX_ENOFILE as i32,
    /// Object is remote
    Eremote = sys::MDBX_error_MDBX_EREMOTE as i32,
    /// Resource deadlock avoided
    Edeadlk = sys::MDBX_error_MDBX_EDEADLK as i32,
}

impl ErrorCode {
    /// Convert the enum variant to the raw MDBX_error value
    pub fn to_raw(self) -> sys::MDBX_error {
        self as i32 as sys::MDBX_error
    }

    /// Convert a raw MDBX_error value to an ErrorCode enum variant
    ///
    /// # Safety
    /// This function uses transmute and assumes the input value corresponds to a valid ErrorCode variant.
    /// Invalid values may result in undefined behavior.
    pub unsafe fn from_raw(code: i32) -> Self {
        unsafe { std::mem::transmute(code) }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum OptionKey {
    MaxDbs,
    MaxReaders,
    SyncBytes,
    SyncPeriod,
}

impl OptionKey {
    fn to_raw(self) -> sys::MDBX_option_t {
        match self {
            OptionKey::MaxDbs => sys::MDBX_option_MDBX_opt_max_db,
            OptionKey::MaxReaders => sys::MDBX_option_MDBX_opt_max_readers,
            OptionKey::SyncBytes => sys::MDBX_option_MDBX_opt_sync_bytes,
            OptionKey::SyncPeriod => sys::MDBX_option_MDBX_opt_sync_period,
        }
    }
}

// ----- Value helpers -----

#[inline]
fn to_val(bytes: &[u8]) -> sys::MDBX_val {
    sys::MDBX_val {
        iov_base: bytes.as_ptr() as *mut c_void,
        iov_len: bytes.len(),
    }
}

#[inline]
#[allow(dead_code)]
unsafe fn from_val(val: &sys::MDBX_val) -> &[u8] {
    unsafe { std::slice::from_raw_parts(val.iov_base as *const u8, val.iov_len) }
}

// ----- Environment -----

pub struct Env {
    ptr: *mut sys::MDBX_env,
}

unsafe impl Send for Env {}
unsafe impl Sync for Env {}

impl Env {
    pub fn new() -> Result<Self> {
        let mut env_ptr: *mut sys::MDBX_env = std::ptr::null_mut();
        let rc = unsafe { sys::mdbx_env_create(&mut env_ptr as *mut _) } as i32;
        if rc != 0 || env_ptr.is_null() {
            return Err(mk_error(rc));
        }
        Ok(Self { ptr: env_ptr })
    }

    pub fn set_option(&self, key: OptionKey, value: u64) -> Result<()> {
        let rc = unsafe { sys::mdbx_env_set_option(self.ptr, key.to_raw(), value) } as i32;
        check(rc)
    }

    pub fn get_option(&self, key: OptionKey) -> Result<u64> {
        let mut out: u64 = 0;
        let rc =
            unsafe { sys::mdbx_env_get_option(self.ptr, key.to_raw(), &mut out as *mut _) } as i32;
        check(rc).map(|_| out)
    }

    pub fn set_geometry(
        &self,
        size_lower: isize,
        size_now: isize,
        size_upper: isize,
        growth_step: isize,
        shrink_threshold: isize,
        pagesize: isize,
    ) -> Result<()> {
        let rc = unsafe {
            sys::mdbx_env_set_geometry(
                self.ptr,
                size_lower,
                size_now,
                size_upper,
                growth_step,
                shrink_threshold,
                pagesize,
            )
        } as i32;
        check(rc)
    }

    pub fn open<P: AsRef<Path>>(&self, path: P, flags: EnvFlags, mode: u16) -> Result<()> {
        let cpath = CString::new(path.as_ref().as_os_str().to_string_lossy().into_owned())
            .map_err(|_| mk_error(sys::MDBX_error_MDBX_EINVAL))?;
        let rc = unsafe {
            sys::mdbx_env_open(
                self.ptr,
                cpath.as_ptr() as *const c_char,
                flags.to_raw(),
                mode as sys::mdbx_mode_t,
            )
        } as i32;
        check(rc)
    }

    pub fn close(self) -> Result<()> {
        // consume self, close with dont_sync=false
        let ptr = self.ptr;
        std::mem::forget(self);
        let rc = unsafe { sys::mdbx_env_close_ex(ptr, false) } as i32;
        check(rc)
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::MDBX_env {
        self.ptr
    }

    /// Flushes environment buffers. Returns true if no data pending for flush.
    pub fn sync(&self, force: bool, nonblock: bool) -> Result<bool> {
        let rc = unsafe { sys::mdbx_env_sync_ex(self.ptr, force, nonblock) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Returns environment statistics (snapshot or txn-relative if provided).
    pub fn stat(&self, txn: Option<&Txn<'_>>) -> Result<sys::MDBX_stat> {
        let mut st: sys::MDBX_stat = Default::default();
        let rc = unsafe {
            sys::mdbx_env_stat_ex(
                self.ptr as *const _,
                txn.map(|t| t.ptr as *const _).unwrap_or(std::ptr::null()),
                &mut st as *mut _,
                std::mem::size_of::<sys::MDBX_stat>(),
            )
        } as i32;
        check(rc).map(|_| st)
    }

    /// Returns environment info (snapshot or txn-relative if provided).
    pub fn info(&self, txn: Option<&Txn<'_>>) -> Result<sys::MDBX_envinfo> {
        let mut info: sys::MDBX_envinfo = Default::default();
        let rc = unsafe {
            sys::mdbx_env_info_ex(
                self.ptr as *const _,
                txn.map(|t| t.ptr as *const _).unwrap_or(std::ptr::null()),
                &mut info as *mut _,
                std::mem::size_of::<sys::MDBX_envinfo>(),
            )
        } as i32;
        check(rc).map(|_| info)
    }

    /// Get current environment flags.
    pub fn get_flags(&self) -> Result<EnvFlags> {
        let mut out: c_uint = 0;
        let rc =
            unsafe { sys::mdbx_env_get_flags(self.ptr as *const _, &mut out as *mut _) } as i32;
        check(rc).map(|_| EnvFlags::from_bits_truncate(out as i32))
    }

    /// Set or clear environment flags at runtime.
    pub fn set_flags(&self, flags: EnvFlags, on: bool) -> Result<()> {
        let rc = unsafe { sys::mdbx_env_set_flags(self.ptr, flags.to_raw(), on) } as i32;
        check(rc)
    }

    /// Returns the path string used to open this environment.
    pub fn get_path(&self) -> Result<String> {
        let mut ptr: *const c_char = std::ptr::null();
        let rc = unsafe { sys::mdbx_env_get_path(self.ptr as *const _, &mut ptr as *mut _) } as i32;
        if rc != 0 {
            return Err(mk_error(rc));
        }
        if ptr.is_null() {
            return Ok(String::new());
        }
        let s = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        Ok(s)
    }

    /// Returns various max sizes for keys/values given flags.
    pub fn max_key_size(&self, flags: DbFlags) -> i32 {
        unsafe { sys::mdbx_env_get_maxkeysize_ex(self.ptr as *const _, flags.to_raw()) as i32 }
    }

    pub fn max_val_size(&self, flags: DbFlags) -> i32 {
        unsafe { sys::mdbx_env_get_maxvalsize_ex(self.ptr as *const _, flags.to_raw()) as i32 }
    }

    /// Warm up database pages according to flags; returns true if timeout reached.
    pub fn warmup(
        &self,
        txn: Option<&Txn<'_>>,
        flags: WarmupFlags,
        timeout_16dot16: u32,
    ) -> Result<bool> {
        let rc = unsafe {
            sys::mdbx_env_warmup(
                self.ptr as *const _,
                txn.map(|t| t.ptr as *const _).unwrap_or(std::ptr::null()),
                flags.to_raw(),
                timeout_16dot16 as c_uint,
            )
        } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Get OS file handle of the environment.
    pub fn get_fd(&self) -> Result<sys::mdbx_filehandle_t> {
        let mut fd: sys::mdbx_filehandle_t = 0 as _;
        let rc = unsafe { sys::mdbx_env_get_fd(self.ptr as *const _, &mut fd as *mut _) } as i32;
        check(rc).map(|_| fd)
    }

    /// Returns system default page size from MDBX.
    pub fn default_pagesize() -> usize {
        unsafe { sys::mdbx_default_pagesize() }
    }

    /// Limits helpers forwarded to MDBX.
    pub fn limits_dbsize_max(pagesize: isize) -> isize {
        unsafe { sys::mdbx_limits_dbsize_max(pagesize) }
    }
    pub fn limits_keysize_max(pagesize: isize, flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_keysize_max(pagesize, flags.to_raw()) }
    }
    pub fn limits_keysize_min(flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_keysize_min(flags.to_raw()) }
    }
    pub fn limits_valsize_max(pagesize: isize, flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_valsize_max(pagesize, flags.to_raw()) }
    }
    pub fn limits_valsize_min(flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_valsize_min(flags.to_raw()) }
    }
    pub fn limits_pairsize4page_max(pagesize: isize, flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_pairsize4page_max(pagesize, flags.to_raw()) }
    }
    pub fn limits_valsize4page_max(pagesize: isize, flags: DbFlags) -> isize {
        unsafe { sys::mdbx_limits_valsize4page_max(pagesize, flags.to_raw()) }
    }

    /// Set/Get a raw user context pointer for this environment (advanced).
    pub fn set_userctx(&self, ctx: *mut c_void) -> Result<()> {
        let rc = unsafe { sys::mdbx_env_set_userctx(self.ptr, ctx) } as i32;
        check(rc)
    }
    pub fn get_userctx(&self) -> *mut c_void {
        unsafe { sys::mdbx_env_get_userctx(self.ptr as *const _) }
    }

    /// Enumerate reader table; returns Ok(true) if table empty (MDBX_RESULT_TRUE), Ok(false) if entries visited.
    pub fn reader_list<F>(&self, mut f: F) -> Result<bool>
    where
        F: FnMut(ReaderEntry) -> i32,
    {
        unsafe extern "C" fn tramp(
            ctx: *mut c_void,
            num: c_int,
            slot: c_int,
            pid: sys::mdbx_pid_t,
            thread: sys::mdbx_tid_t,
            txnid: u64,
            lag: u64,
            bytes_used: usize,
            bytes_retained: usize,
        ) -> c_int {
            let cb = unsafe { &mut *(ctx as *mut &mut dyn FnMut(ReaderEntry) -> i32) };
            let entry = ReaderEntry {
                num: num as i32,
                slot: slot as i32,
                txnid,
                lag,
                pid,
                tid: thread,
                bytes_used,
                bytes_retained,
            };
            cb(entry) as c_int
        }
        let mut cb: &mut dyn FnMut(ReaderEntry) -> i32 = &mut f;
        let rc = unsafe {
            sys::mdbx_reader_list(
                self.ptr as *const _,
                Some(tramp),
                (&mut cb as *mut _) as *mut c_void,
            )
        } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Check and cleanup stale reader table entries. Returns (had_issue, dead_count).
    pub fn reader_check(&self) -> Result<(bool, i32)> {
        let mut dead: c_int = 0;
        let rc = unsafe { sys::mdbx_reader_check(self.ptr, &mut dead as *mut _) } as i32;
        if rc == 0 {
            Ok((false, dead))
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok((true, dead))
        } else {
            Err(mk_error(rc))
        }
    }

    /// Pre-register/unregister current thread as reader.
    pub fn thread_register(&self) -> Result<bool> {
        let rc = unsafe { sys::mdbx_thread_register(self.ptr as *const _) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }
    pub fn thread_unregister(&self) -> Result<bool> {
        let rc = unsafe { sys::mdbx_thread_unregister(self.ptr as *const _) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    // /// Set or clear the Handle-Slow-Readers callback. Returns Ok when installed.
    // pub fn set_hsr<F>(&self, cb: Option<F>) -> Result<()>
    // where
    //     F: FnMut(HsrArgs) -> i32 + Send + 'static,
    // {
    //     let reg = HSR_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    //     let mut map = reg.lock().expect("hsr registry lock");
    //     match cb {
    //         Some(mut f) => {
    //             map.insert(self.ptr as *const _, Box::new(f));
    //             let rc = unsafe { sys::mdbx_env_set_hsr(self.ptr, Some(hsr_trampoline)) } as i32;
    //             check(rc)
    //         }
    //         None => {
    //             map.remove(&(self.ptr as *const _));
    //             let rc = unsafe { sys::mdbx_env_set_hsr(self.ptr, None) } as i32;
    //             check(rc)
    //         }
    //     }
    // }
}

#[derive(Debug, Clone)]
pub struct ReaderEntry {
    pub num: i32,
    pub slot: i32,
    pub txnid: u64,
    pub lag: u64,
    pub pid: sys::mdbx_pid_t,
    pub tid: sys::mdbx_tid_t,
    pub bytes_used: usize,
    pub bytes_retained: usize,
}

// HSR support intentionally omitted per request.

// ----- Extra Env Ops: copy/backup and delete -----

impl Env {
    /// Copy the environment to a new file path.
    pub fn copy_to_path<P: AsRef<Path>>(&self, dest: P, flags: CopyFlags) -> Result<()> {
        let cpath = CString::new(dest.as_ref().as_os_str().to_string_lossy().into_owned())
            .map_err(|_| mk_error(sys::MDBX_error_MDBX_EINVAL))?;
        let rc = unsafe { sys::mdbx_env_copy(self.ptr, cpath.as_ptr(), flags.to_raw()) } as i32;
        check(rc)
    }

    /// Copy the environment to a file descriptor.
    pub fn copy_to_fd(&self, fd: sys::mdbx_filehandle_t, flags: CopyFlags) -> Result<()> {
        let rc = unsafe { sys::mdbx_env_copy2fd(self.ptr, fd, flags.to_raw()) } as i32;
        check(rc)
    }

    /// Delete the environment files at a given path with a mode.
    pub fn delete_path<P: AsRef<Path>>(path: P, mode: DeleteMode) -> Result<()> {
        let cpath = CString::new(path.as_ref().as_os_str().to_string_lossy().into_owned())
            .map_err(|_| mk_error(sys::MDBX_error_MDBX_EINVAL))?;
        let rc = unsafe { sys::mdbx_env_delete(cpath.as_ptr(), mode.to_raw()) } as i32;
        // mdbx_env_delete returns 0 or MDBX_RESULT_TRUE if nothing to delete; treat both as Ok.
        if rc == 0 || rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(())
        } else {
            Err(mk_error(rc))
        }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::mdbx_env_close_ex(self.ptr, false);
        }
    }
}

// ----- Transactions -----

bitflags::bitflags! {
    pub struct TxnFlags: i32 {
        const READWRITE   = sys::MDBX_txn_flags_MDBX_TXN_READWRITE as i32; // 0
        const RDONLY      = sys::MDBX_txn_flags_MDBX_TXN_RDONLY as i32;
        const TRY         = sys::MDBX_txn_flags_MDBX_TXN_TRY as i32;
        const NOMETASYNC  = sys::MDBX_txn_flags_MDBX_TXN_NOMETASYNC as i32;
        const NOSYNC      = sys::MDBX_txn_flags_MDBX_TXN_NOSYNC as i32;
    }
}

impl TxnFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_txn_flags_t {
        self.bits() as sys::MDBX_txn_flags_t
    }
}

pub struct Txn<'e> {
    ptr: *mut sys::MDBX_txn,
    _marker: PhantomData<&'e mut Env>,
}

impl<'e> Txn<'e> {
    pub fn begin(
        env: &'e Env,
        parent: Option<*mut sys::MDBX_txn>,
        flags: TxnFlags,
    ) -> Result<Self> {
        let mut txn_ptr: *mut sys::MDBX_txn = std::ptr::null_mut();
        let rc = unsafe {
            sys::mdbx_txn_begin_ex(
                env.ptr,
                parent.unwrap_or(std::ptr::null_mut()),
                flags.to_raw(),
                &mut txn_ptr as *mut _,
                std::ptr::null_mut(),
            )
        } as i32;
        if rc != 0 || txn_ptr.is_null() {
            return Err(mk_error(rc));
        }
        Ok(Self {
            ptr: txn_ptr,
            _marker: PhantomData,
        })
    }

    pub fn commit(self) -> Result<()> {
        let ptr = self.ptr;
        std::mem::forget(self);
        let rc = unsafe { sys::mdbx_txn_commit_ex(ptr, std::ptr::null_mut()) } as i32;
        check(rc)
    }

    pub fn abort(self) -> Result<()> {
        let ptr = self.ptr;
        std::mem::forget(self);
        let rc = unsafe { sys::mdbx_txn_abort(ptr) } as i32;
        check(rc)
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut sys::MDBX_txn {
        self.ptr
    }

    /// Transaction flags including internal; map to TxnFlags where possible.
    pub fn flags(&self) -> TxnFlags {
        let raw = unsafe { sys::mdbx_txn_flags(self.ptr as *const _) } as i32;
        TxnFlags::from_bits_truncate(raw)
    }

    #[inline]
    pub fn id(&self) -> u64 {
        unsafe { sys::mdbx_txn_id(self.ptr as *const _) }
    }

    pub fn info(&self, scan_rlt: bool) -> Result<sys::MDBX_txn_info> {
        let mut info: sys::MDBX_txn_info = Default::default();
        let rc = unsafe { sys::mdbx_txn_info(self.ptr as *const _, &mut info as *mut _, scan_rlt) }
            as i32;
        check(rc).map(|_| info)
    }

    /// Reset a read-only transaction for reuse (see MDBX docs).
    pub fn reset(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_txn_reset(self.ptr) } as i32;
        check(rc)
    }

    /// Renew a reset read-only transaction for use again.
    pub fn renew(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_txn_renew(self.ptr) } as i32;
        check(rc)
    }

    /// Unbind or close all cursors of this txn and its parents.
    pub fn release_all_cursors(&self, unbind: bool) -> Result<usize> {
        let mut count: usize = 0;
        let rc = unsafe {
            sys::mdbx_txn_release_all_cursors_ex(self.ptr as *const _, unbind, &mut count as *mut _)
        } as i32;
        check(rc).map(|_| count)
    }

    /// Mark transaction as broken to prevent further operations (must abort later).
    pub fn break_txn(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_txn_break(self.ptr) } as i32;
        check(rc)
    }

    /// Park a read-only transaction; autounpark allows implicit unpark by read ops.
    #[allow(dead_code)]
    pub fn park(&mut self, autounpark: bool) -> Result<()> {
        #[allow(non_snake_case)]
        unsafe extern "C" {
            fn mdbx_txn_park(txn: *mut sys::MDBX_txn, autounpark: bool) -> c_int;
        }
        let rc = unsafe { sys::mdbx_txn_park(self.ptr, autounpark) } as i32;
        check(rc)
    }

    /// Unpark a parked read-only transaction; returns true if ousted/restarted.
    #[allow(dead_code)]
    pub fn unpark(&mut self, restart_if_ousted: bool) -> Result<bool> {
        #[allow(non_snake_case)]
        unsafe extern "C" {
            fn mdbx_txn_unpark(txn: *mut sys::MDBX_txn, restart_if_ousted: bool) -> c_int;
        }
        let rc = unsafe { sys::mdbx_txn_unpark(self.ptr, restart_if_ousted) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Copy a consistent snapshot of the environment to a path via this read txn.
    pub fn copy_to_path<P: AsRef<Path>>(&self, dest: P, flags: CopyFlags) -> Result<()> {
        let cpath = CString::new(dest.as_ref().as_os_str().to_string_lossy().into_owned())
            .map_err(|_| mk_error(sys::MDBX_error_MDBX_EINVAL))?;
        let rc =
            unsafe { sys::mdbx_txn_copy2pathname(self.ptr, cpath.as_ptr(), flags.to_raw()) } as i32;
        check(rc)
    }

    /// Copy a consistent snapshot of the environment to a file descriptor via this read txn.
    pub fn copy_to_fd(&self, fd: sys::mdbx_filehandle_t, flags: CopyFlags) -> Result<()> {
        let rc = unsafe { sys::mdbx_txn_copy2fd(self.ptr, fd, flags.to_raw()) } as i32;
        check(rc)
    }

    /// Set/Get a raw user context pointer for this transaction (advanced).
    pub fn set_userctx(&mut self, ctx: *mut c_void) -> Result<()> {
        let rc = unsafe { sys::mdbx_txn_set_userctx(self.ptr, ctx) } as i32;
        check(rc)
    }
    pub fn get_userctx(&self) -> *mut c_void {
        unsafe { sys::mdbx_txn_get_userctx(self.ptr as *const _) }
    }

    /// Set the environment canary (x,y,z from canary; v is set to txn id). If canary is None, only v is updated.
    pub fn set_canary(&self, canary: Option<&sys::MDBX_canary>) -> Result<()> {
        let rc = unsafe {
            sys::mdbx_canary_put(self.ptr, canary.map_or(std::ptr::null(), |c| c as *const _))
        } as i32;
        check(rc)
    }

    /// Get the current environment canary values.
    pub fn get_canary(&self) -> Result<sys::MDBX_canary> {
        let mut out: sys::MDBX_canary = Default::default();
        let rc = unsafe { sys::mdbx_canary_get(self.ptr as *const _, &mut out as *mut _) } as i32;
        check(rc).map(|_| out)
    }

    /// Compare two keys for this table using MDBX's ordering.
    pub fn cmp_keys(&self, dbi: Dbi, a: &[u8], b: &[u8]) -> std::cmp::Ordering {
        let av = to_val(a);
        let bv = to_val(b);
        let r = unsafe {
            sys::mdbx_cmp(
                self.ptr as *const _,
                dbi.0,
                &av as *const _,
                &bv as *const _,
            )
        } as i32;
        r.cmp(&0)
    }

    /// Compare two data items for this table (DUPSORT) using MDBX's ordering.
    pub fn cmp_data(&self, dbi: Dbi, a: &[u8], b: &[u8]) -> std::cmp::Ordering {
        let av = to_val(a);
        let bv = to_val(b);
        let r = unsafe {
            sys::mdbx_dcmp(
                self.ptr as *const _,
                dbi.0,
                &av as *const _,
                &bv as *const _,
            )
        } as i32;
        r.cmp(&0)
    }

    /// Return lag and percentage of page allocation of this read txn relative to head.
    pub fn straggler(&self) -> Result<(i32, i32)> {
        let mut percent: c_int = 0;
        let lag =
            unsafe { sys::mdbx_txn_straggler(self.ptr as *const _, &mut percent as *mut _) } as i32;
        if lag < 0 {
            Err(mk_error(lag))
        } else {
            Ok((lag, percent))
        }
    }
}

impl<'e> Drop for Txn<'e> {
    fn drop(&mut self) {
        unsafe {
            let _ = sys::mdbx_txn_abort(self.ptr);
        }
    }
}

// ----- DBI handle -----

#[derive(Copy, Clone)]
pub struct Dbi(sys::MDBX_dbi);

impl Dbi {
    pub fn open(txn: &Txn<'_>, name: Option<&str>, flags: DbFlags) -> Result<Dbi> {
        let cname = match name {
            Some(s) => Some(CString::new(s).map_err(|_| mk_error(sys::MDBX_error_MDBX_EINVAL))?),
            None => None,
        };
        let mut dbi: sys::MDBX_dbi = 0;
        let rc = unsafe {
            sys::mdbx_dbi_open(
                txn.ptr,
                cname.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
                flags.to_raw(),
                &mut dbi as *mut _,
            )
        } as i32;
        check(rc).map(|_| Dbi(dbi))
    }

    pub fn close(&self, env: &Env) -> Result<()> {
        let rc = unsafe { sys::mdbx_dbi_close(env.ptr, self.0) } as i32;
        check(rc)
    }

    #[inline]
    pub fn raw(&self) -> sys::MDBX_dbi {
        self.0
    }

    /// Retrieve DB flags/state for this handle in the given transaction.
    pub fn flags_ex(&self, txn: &Txn<'_>) -> Result<(DbFlags, u32)> {
        let mut flags: c_uint = 0;
        let mut state: c_uint = 0;
        let rc = unsafe {
            sys::mdbx_dbi_flags_ex(
                txn.ptr as *const _,
                self.0,
                &mut flags as *mut _,
                &mut state as *mut _,
            )
        } as i32;
        check(rc).map(|_| (DbFlags::from_bits_truncate(flags as i32), state as u32))
    }

    /// Retrieve statistics for this table in the given transaction.
    pub fn stat(&self, txn: &Txn<'_>) -> Result<sys::MDBX_stat> {
        let mut st: sys::MDBX_stat = Default::default();
        let rc = unsafe {
            sys::mdbx_dbi_stat(
                txn.ptr as *const _,
                self.0,
                &mut st as *mut _,
                std::mem::size_of::<sys::MDBX_stat>(),
            )
        } as i32;
        check(rc).map(|_| st)
    }

    /// Retrieve dupsort nested btree depthmask. Returns None if not dupsort.
    pub fn dupsort_depthmask(&self, txn: &Txn<'_>) -> Result<Option<u32>> {
        let mut mask: u32 = 0;
        let rc = unsafe {
            sys::mdbx_dbi_dupsort_depthmask(txn.ptr as *const _, self.0, &mut mask as *mut _)
        } as i32;
        if rc == 0 {
            Ok(Some(mask))
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Empty or delete and close a table.
    pub fn drop(&self, txn: &Txn<'_>, delete: bool) -> Result<()> {
        let rc = unsafe { sys::mdbx_drop(txn.ptr, self.0, delete) } as i32;
        check(rc)
    }
}

// ----- CRUD -----

pub fn get<'txn>(txn: &'txn Txn<'_>, dbi: Dbi, key: &[u8]) -> Result<Option<&'txn [u8]>> {
    let key_val = to_val(key);
    let mut data = sys::MDBX_val {
        iov_base: std::ptr::null_mut(),
        iov_len: 0,
    };
    let rc = unsafe {
        sys::mdbx_get(
            txn.ptr as *const _,
            dbi.0,
            &key_val as *const _,
            &mut data as *mut _,
        )
    } as i32;
    if rc == 0 {
        // Safety: MDBX returns a pointer to memory owned by the DB, valid for the
        // lifetime of the read transaction and until the next write affecting it.
        let slice: &'txn [u8] =
            unsafe { std::slice::from_raw_parts(data.iov_base as *const u8, data.iov_len) };
        Ok(Some(slice))
    } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
        Ok(None)
    } else {
        Err(mk_error(rc))
    }
}

pub fn put(txn: &Txn<'_>, dbi: Dbi, key: &[u8], value: &[u8], flags: PutFlags) -> Result<()> {
    let mut key_val = to_val(key);
    let mut data_val = to_val(value);
    let rc = unsafe {
        sys::mdbx_put(
            txn.ptr,
            dbi.0,
            &mut key_val as *mut _,
            &mut data_val as *mut _,
            flags.to_raw(),
        )
    } as i32;
    check(rc)
}

/// Put multiple contiguous values for DUPFIXED tables in a single call.
/// - `elem_size` is the size of each element in bytes.
/// - `payload` is a contiguous region containing `count = payload.len() / elem_size` elements.
/// Returns the number of elements actually written.
pub fn put_multiple(
    txn: &Txn<'_>,
    dbi: Dbi,
    key: &[u8],
    elem_size: usize,
    payload: &[u8],
    flags: PutFlags,
) -> Result<usize> {
    assert!(elem_size > 0, "elem_size must be > 0");
    if payload.len() % elem_size != 0 {
        return Err(mk_error(sys::MDBX_error_MDBX_EINVAL));
    }
    let mut key_val = to_val(key);
    // Prepare the 2-element array per MDBX MULTIPLE semantics
    let mut vals = [
        sys::MDBX_val {
            iov_base: payload.as_ptr() as *mut c_void,
            iov_len: elem_size,
        },
        sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: payload.len() / elem_size,
        },
    ];
    let rc = unsafe {
        sys::mdbx_put(
            txn.ptr,
            dbi.0,
            &mut key_val as *mut _,
            vals.as_mut_ptr(),
            (flags | PutFlags::MULTIPLE).to_raw(),
        )
    } as i32;
    if rc == 0 {
        Ok(vals[1].iov_len)
    } else {
        Err(mk_error(rc))
    }
}

pub fn del(txn: &Txn<'_>, dbi: Dbi, key: &[u8], value: Option<&[u8]>) -> Result<()> {
    let key_val = to_val(key);
    let data_ptr = if let Some(v) = value {
        &to_val(v) as *const _
    } else {
        std::ptr::null()
    };
    let rc = unsafe { sys::mdbx_del(txn.ptr, dbi.0, &key_val as *const _, data_ptr) } as i32;
    check(rc)
}

/// Retrieve equal or next greater key/value pair. Returns whether it was an exact match.
pub enum Match {
    Exact,
    Greater,
}

pub fn get_equal_or_great<'txn>(
    txn: &'txn Txn<'_>,
    dbi: Dbi,
    key_in_out: &mut &'txn [u8],
    data_in_out: &mut &'txn [u8],
) -> Result<Match> {
    let mut k = to_val(*key_in_out);
    let mut d = to_val(*data_in_out);
    let rc = unsafe {
        sys::mdbx_get_equal_or_great(
            txn.ptr as *const _,
            dbi.0,
            &mut k as *mut _,
            &mut d as *mut _,
        )
    } as i32;
    if rc == 0 || rc == sys::MDBX_error_MDBX_RESULT_TRUE {
        // Update to DB-backed slices
        *key_in_out = unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
        *data_in_out = unsafe { std::slice::from_raw_parts(d.iov_base as *const u8, d.iov_len) };
        Ok(if rc == 0 {
            Match::Exact
        } else {
            Match::Greater
        })
    } else {
        Err(mk_error(rc))
    }
}

/// Replace item at key with new value (or delete if new=None). Optionally retrieve previous value into provided buffer.
/// If the buffer is too small, returns NeedOldSize(required_len) without changing data.
pub enum ReplaceOutcome {
    Replaced,
    Deleted,
    NeedOldSize(usize),
}

pub fn replace(
    txn: &Txn<'_>,
    dbi: Dbi,
    key: &[u8],
    new: Option<&[u8]>,
    mut old_out: Option<&mut [u8]>,
    flags: PutFlags,
) -> Result<ReplaceOutcome> {
    let k = to_val(key);
    let mut new_val = new.map(|n| to_val(n));
    let mut old_val = if let Some(buf) = old_out.as_deref_mut() {
        sys::MDBX_val {
            iov_base: buf.as_mut_ptr() as *mut c_void,
            iov_len: buf.len(),
        }
    } else {
        sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: 0,
        }
    };
    let rc = unsafe {
        sys::mdbx_replace_ex(
            txn.ptr,
            dbi.0,
            &k as *const _,
            new_val
                .as_mut()
                .map_or(std::ptr::null_mut(), |v| v as *mut _),
            &mut old_val as *mut _,
            flags.to_raw(),
            None,
            std::ptr::null_mut(),
        )
    } as i32;
    if rc == 0 {
        Ok(if new.is_some() {
            ReplaceOutcome::Replaced
        } else {
            ReplaceOutcome::Deleted
        })
    } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
        // old_out too small; report required length via iov_len
        Ok(ReplaceOutcome::NeedOldSize(old_val.iov_len))
    } else {
        Err(mk_error(rc))
    }
}

// ----- Cursor -----

pub struct Cursor<'t> {
    ptr: *mut sys::MDBX_cursor,
    _marker: PhantomData<&'t Txn<'t>>,
}

impl<'t> Cursor<'t> {
    pub fn open(txn: &'t Txn<'t>, dbi: Dbi) -> Result<Self> {
        let mut cur: *mut sys::MDBX_cursor = std::ptr::null_mut();
        let rc = unsafe { sys::mdbx_cursor_open(txn.ptr, dbi.0, &mut cur as *mut _) } as i32;
        if rc != 0 || cur.is_null() {
            return Err(mk_error(rc));
        }
        Ok(Self {
            ptr: cur,
            _marker: PhantomData,
        })
    }

    pub fn first<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_FIRST)
    }

    pub fn next<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_NEXT)
    }

    pub fn last<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_LAST)
    }

    pub fn prev<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_PREV)
    }

    pub fn set_range<'c>(&'c mut self, key: &[u8]) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = to_val(key);
        let mut v = sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: 0,
        };
        let rc = unsafe {
            sys::mdbx_cursor_get(
                self.ptr,
                &mut k as *mut _,
                &mut v as *mut _,
                sys::MDBX_cursor_op_MDBX_SET_RANGE,
            )
        } as i32;
        if rc == 0 {
            // Safety: as above; returned pointers are owned by DB and valid while txn lives.
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    fn get_op<'c>(&'c mut self, op: sys::MDBX_cursor_op) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: 0,
        };
        let mut v = sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: 0,
        };
        let rc = unsafe { sys::mdbx_cursor_get(self.ptr, &mut k as *mut _, &mut v as *mut _, op) }
            as i32;
        if rc == 0 {
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Position cursor at exact key if present and return current pair.
    pub fn seek_key<'c>(&'c mut self, key: &[u8]) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = to_val(key);
        let mut v = sys::MDBX_val {
            iov_base: std::ptr::null_mut(),
            iov_len: 0,
        };
        let rc = unsafe {
            sys::mdbx_cursor_get(
                self.ptr,
                &mut k as *mut _,
                &mut v as *mut _,
                sys::MDBX_cursor_op_MDBX_SET,
            )
        } as i32;
        if rc == 0 {
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    /// For DUPSORT tables: move to first duplicate at current key.
    pub fn first_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_FIRST_DUP)
    }

    /// For DUPSORT tables: move to last duplicate at current key.
    pub fn last_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_LAST_DUP)
    }

    /// For DUPSORT tables: move to next duplicate (stay on same key).
    pub fn next_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_NEXT_DUP)
    }

    /// For DUPSORT tables: move to previous duplicate (stay on same key).
    pub fn prev_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_PREV_DUP)
    }

    /// Move to next non-duplicate key.
    pub fn next_no_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_NEXT_NODUP)
    }

    /// Move to previous non-duplicate key.
    pub fn prev_no_dup<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_PREV_NODUP)
    }

    /// Seek to exact (key,data) pair in DUPSORT tables.
    pub fn get_both<'c>(
        &'c mut self,
        key: &[u8],
        data: &[u8],
    ) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = to_val(key);
        let mut v = to_val(data);
        let rc = unsafe {
            sys::mdbx_cursor_get(
                self.ptr,
                &mut k as *mut _,
                &mut v as *mut _,
                sys::MDBX_cursor_op_MDBX_GET_BOTH,
            )
        } as i32;
        if rc == 0 {
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Seek to key and the smallest data >= given data (DUPSORT tables).
    pub fn get_both_range<'c>(
        &'c mut self,
        key: &[u8],
        data: &[u8],
    ) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = to_val(key);
        let mut v = to_val(data);
        let rc = unsafe {
            sys::mdbx_cursor_get(
                self.ptr,
                &mut k as *mut _,
                &mut v as *mut _,
                sys::MDBX_cursor_op_MDBX_GET_BOTH_RANGE,
            )
        } as i32;
        if rc == 0 {
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Return the current key/value without moving the cursor.
    pub fn current<'c>(&'c mut self) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(sys::MDBX_cursor_op_MDBX_GET_CURRENT)
    }

    /// Reset/unset the cursor position.
    pub fn reset(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_cursor_reset(self.ptr) } as i32;
        check(rc)
    }

    /// Unbind cursor from its current transaction keeping DBI for reuse.
    pub fn unbind(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_cursor_unbind(self.ptr) } as i32;
        check(rc)
    }

    /// Bind cursor to a transaction and dbi handle.
    pub fn bind(&mut self, txn: &mut Txn<'t>, dbi: Dbi) -> Result<()> {
        let rc = unsafe { sys::mdbx_cursor_bind(txn.ptr, self.ptr, dbi.0) } as i32;
        check(rc)
    }

    /// Renew cursor for a transaction using its previous DBI.
    pub fn renew(&mut self, txn: &mut Txn<'t>) -> Result<()> {
        let rc = unsafe { sys::mdbx_cursor_renew(txn.ptr, self.ptr) } as i32;
        check(rc)
    }

    /// Check whether cursor is on last key/value.
    pub fn on_last(&self) -> Result<bool> {
        let rc = unsafe { sys::mdbx_cursor_on_last(self.ptr as *const _) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Check whether cursor is on last duplicate for current key.
    pub fn on_last_dup(&self) -> Result<bool> {
        let rc = unsafe { sys::mdbx_cursor_on_last_dup(self.ptr as *const _) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Estimate distance between this cursor and another cursor (same dbi/txn).
    pub fn estimate_distance(&self, other: &Cursor<'t>) -> Result<isize> {
        let mut out: isize = 0;
        let rc = unsafe {
            sys::mdbx_estimate_distance(
                self.ptr as *const _,
                other.ptr as *const _,
                &mut out as *mut _,
            )
        } as i32;
        check(rc).map(|_| out)
    }

    /// Estimate distance for a move op relative to current position (does not move the cursor).
    pub fn estimate_move(
        &self,
        op: sys::MDBX_cursor_op,
        key_out: &mut &[u8],
        val_out: &mut &[u8],
    ) -> Result<isize> {
        let mut k = to_val(*key_out);
        let mut v = to_val(*val_out);
        let mut out: isize = 0;
        let rc = unsafe {
            sys::mdbx_estimate_move(
                self.ptr as *const _,
                &mut k as *mut _,
                &mut v as *mut _,
                op,
                &mut out as *mut _,
            )
        } as i32;
        if rc != 0 {
            return Err(mk_error(rc));
        }
        *key_out = unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
        *val_out = unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
        Ok(out)
    }

    /// Estimate distance for a move op relative to current position using CursorOp enum (does not move the cursor).
    pub fn estimate_move_with_op(
        &self,
        op: CursorOp,
        key_out: &mut &[u8],
        val_out: &mut &[u8],
    ) -> Result<isize> {
        self.estimate_move(op.to_raw(), key_out, val_out)
    }

    /// Get the owning transaction pointer (unsafe low-level).
    pub fn txn_ptr(&self) -> *mut sys::MDBX_txn {
        unsafe { sys::mdbx_cursor_txn(self.ptr as *const _) }
    }

    /// Get the DBI handle for this cursor.
    pub fn dbi(&self) -> Dbi {
        Dbi(unsafe { sys::mdbx_cursor_dbi(self.ptr as *const _) })
    }

    /// For DUPFIXED tables: GET_MULTIPLE. Returns the contiguous payload slice and element count (computed from elem_size).
    pub fn get_multiple<'c>(&'c mut self, elem_size: usize) -> Result<Option<(&'c [u8], usize)>> {
        match self.get_op(sys::MDBX_cursor_op_MDBX_GET_MULTIPLE)? {
            Some((_k, v)) => Ok(Some((
                v,
                if elem_size > 0 {
                    v.len() / elem_size
                } else {
                    0
                },
            ))),
            None => Ok(None),
        }
    }

    /// NEXT_MULTIPLE variant for DUPFIXED tables.
    pub fn next_multiple<'c>(&'c mut self, elem_size: usize) -> Result<Option<(&'c [u8], usize)>> {
        match self.get_op(sys::MDBX_cursor_op_MDBX_NEXT_MULTIPLE)? {
            Some((_k, v)) => Ok(Some((
                v,
                if elem_size > 0 {
                    v.len() / elem_size
                } else {
                    0
                },
            ))),
            None => Ok(None),
        }
    }

    /// PREV_MULTIPLE variant for DUPFIXED tables.
    pub fn prev_multiple<'c>(&'c mut self, elem_size: usize) -> Result<Option<(&'c [u8], usize)>> {
        match self.get_op(sys::MDBX_cursor_op_MDBX_PREV_MULTIPLE)? {
            Some((_k, v)) => Ok(Some((
                v,
                if elem_size > 0 {
                    v.len() / elem_size
                } else {
                    0
                },
            ))),
            None => Ok(None),
        }
    }

    /// Insert/replace using this cursor. For MULTIPLE semantics, prefer put_multiple.
    pub fn put(&mut self, key: &[u8], data: &mut [u8], flags: PutFlags) -> Result<()> {
        let kv = to_val(key);
        let mut dv = sys::MDBX_val {
            iov_base: data.as_mut_ptr() as *mut c_void,
            iov_len: data.len(),
        };
        let rc = unsafe {
            sys::mdbx_cursor_put(self.ptr, &kv as *const _, &mut dv as *mut _, flags.to_raw())
        } as i32;
        check(rc)
    }

    /// Delete at current position; if all_dups then delete all duplicates for this key.
    pub fn del_current(&mut self, all_dups: bool) -> Result<()> {
        let flag = if all_dups {
            sys::MDBX_put_flags_MDBX_ALLDUPS
        } else {
            sys::MDBX_put_flags_MDBX_CURRENT
        };
        let rc = unsafe { sys::mdbx_cursor_del(self.ptr, flag) } as i32;
        check(rc)
    }

    /// Return number of duplicates for current key.
    pub fn dup_count(&self) -> Result<usize> {
        let mut c: usize = 0;
        let rc = unsafe { sys::mdbx_cursor_count(self.ptr as *const _, &mut c as *mut _) } as i32;
        check(rc).map(|_| c)
    }

    /// Return EOF state of cursor.
    pub fn eof(&self) -> Result<bool> {
        let rc = unsafe { sys::mdbx_cursor_eof(self.ptr as *const _) } as i32;
        if rc == 0 {
            Ok(false)
        } else if rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(true)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Disable order checks for this cursor (for custom comparators scenarios).
    pub fn ignore_order(&mut self) -> Result<()> {
        let rc = unsafe { sys::mdbx_cursor_ignord(self.ptr) } as i32;
        check(rc)
    }

    /// Scan using a predicate function starting from start_op then turning with turn_op until predicate returns true or end.
    pub fn scan<F>(
        &mut self,
        start_op: sys::MDBX_cursor_op,
        turn_op: sys::MDBX_cursor_op,
        mut pred: F,
    ) -> Result<i32>
    where
        F: FnMut(&[u8], &mut [u8]) -> Result<bool>,
    {
        unsafe extern "C" fn tramp(
            ctx: *mut c_void,
            key: *mut sys::MDBX_val,
            value: *mut sys::MDBX_val,
            _arg: *mut c_void,
        ) -> c_int {
            let cb =
                unsafe { &mut *(ctx as *mut &mut dyn FnMut(&[u8], &mut [u8]) -> Result<bool>) };
            let ks =
                unsafe { std::slice::from_raw_parts((*key).iov_base as *const u8, (*key).iov_len) };
            let vs = unsafe {
                std::slice::from_raw_parts_mut((*value).iov_base as *mut u8, (*value).iov_len)
            };
            match cb(ks, vs) {
                Ok(true) => sys::MDBX_error_MDBX_RESULT_TRUE,
                Ok(false) => 0,
                Err(_) => 1,
            }
        }

        let mut cb: &mut dyn FnMut(&[u8], &mut [u8]) -> Result<bool> = &mut pred;
        let rc = unsafe {
            sys::mdbx_cursor_scan(
                self.ptr,
                Some(tramp),
                (&mut cb as *mut _) as *mut c_void,
                start_op,
                turn_op,
                std::ptr::null_mut(),
            )
        } as i32;
        if rc >= 0 || rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(rc)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Scan using a predicate function with CursorOp enum.
    pub fn scan_with_ops<F>(
        &mut self,
        start_op: CursorOp,
        turn_op: CursorOp,
        pred: F,
    ) -> Result<i32>
    where
        F: FnMut(&[u8], &mut [u8]) -> Result<bool>,
    {
        self.scan(start_op.to_raw(), turn_op.to_raw(), pred)
    }

    /// Scan from a given (key,value) position using predicate.
    pub fn scan_from<F>(
        &mut self,
        from_op: sys::MDBX_cursor_op,
        from_key: &mut [u8],
        from_value: &mut [u8],
        turn_op: sys::MDBX_cursor_op,
        mut pred: F,
    ) -> Result<i32>
    where
        F: FnMut(&[u8], &mut [u8]) -> Result<bool>,
    {
        unsafe extern "C" fn tramp(
            ctx: *mut c_void,
            key: *mut sys::MDBX_val,
            value: *mut sys::MDBX_val,
            _arg: *mut c_void,
        ) -> c_int {
            let cb =
                unsafe { &mut *(ctx as *mut &mut dyn FnMut(&[u8], &mut [u8]) -> Result<bool>) };
            let ks =
                unsafe { std::slice::from_raw_parts((*key).iov_base as *const u8, (*key).iov_len) };
            let vs = unsafe {
                std::slice::from_raw_parts_mut((*value).iov_base as *mut u8, (*value).iov_len)
            };
            match cb(ks, vs) {
                Ok(true) => sys::MDBX_error_MDBX_RESULT_TRUE,
                Ok(false) => 0,
                Err(_) => 1,
            }
        }

        let mut k = sys::MDBX_val {
            iov_base: from_key.as_mut_ptr() as *mut c_void,
            iov_len: from_key.len(),
        };
        let mut v = sys::MDBX_val {
            iov_base: from_value.as_mut_ptr() as *mut c_void,
            iov_len: from_value.len(),
        };
        let mut cb: &mut dyn FnMut(&[u8], &mut [u8]) -> Result<bool> = &mut pred;
        let rc = unsafe {
            sys::mdbx_cursor_scan_from(
                self.ptr,
                Some(tramp),
                (&mut cb as *mut _) as *mut c_void,
                from_op,
                &mut k as *mut _,
                &mut v as *mut _,
                turn_op,
                std::ptr::null_mut(),
            )
        } as i32;
        if rc >= 0 || rc == sys::MDBX_error_MDBX_RESULT_TRUE {
            Ok(rc)
        } else {
            Err(mk_error(rc))
        }
    }

    /// Scan from a given (key,value) position using predicate with CursorOp enum.
    pub fn scan_from_with_ops<F>(
        &mut self,
        from_op: CursorOp,
        from_key: &mut [u8],
        from_value: &mut [u8],
        turn_op: CursorOp,
        pred: F,
    ) -> Result<i32>
    where
        F: FnMut(&[u8], &mut [u8]) -> Result<bool>,
    {
        self.scan_from(
            from_op.to_raw(),
            from_key,
            from_value,
            turn_op.to_raw(),
            pred,
        )
    }

    /// Generic cursor operation using the CursorOp enum.
    /// This provides a unified interface for all cursor operations.
    pub fn get_with_op<'c>(&'c mut self, op: CursorOp) -> Result<Option<(&'c [u8], &'c [u8])>> {
        self.get_op(op.to_raw())
    }

    /// Generic cursor operation with key/value parameters using the CursorOp enum.
    /// This is useful for operations that require key/value input like Set, GetBoth, etc.
    pub fn get_with_op_and_data<'c>(
        &'c mut self,
        op: CursorOp,
        key: &[u8],
        data: &[u8],
    ) -> Result<Option<(&'c [u8], &'c [u8])>> {
        let mut k = to_val(key);
        let mut v = to_val(data);
        let rc = unsafe {
            sys::mdbx_cursor_get(self.ptr, &mut k as *mut _, &mut v as *mut _, op.to_raw())
        } as i32;
        if rc == 0 {
            let ks: &'c [u8] =
                unsafe { std::slice::from_raw_parts(k.iov_base as *const u8, k.iov_len) };
            let vs: &'c [u8] =
                unsafe { std::slice::from_raw_parts(v.iov_base as *const u8, v.iov_len) };
            Ok(Some((ks, vs)))
        } else if rc == sys::MDBX_error_MDBX_NOTFOUND {
            Ok(None)
        } else {
            Err(mk_error(rc))
        }
    }
}

bitflags::bitflags! {
    pub struct CopyFlags: i32 {
        const DEFAULTS          = sys::MDBX_copy_flags_MDBX_CP_DEFAULTS as i32;
        const COMPACT           = sys::MDBX_copy_flags_MDBX_CP_COMPACT as i32;
        const FORCE_DYNAMIC_SIZE= sys::MDBX_copy_flags_MDBX_CP_FORCE_DYNAMIC_SIZE as i32;
        const DONT_FLUSH        = sys::MDBX_copy_flags_MDBX_CP_DONT_FLUSH as i32;
        const THROTTLE_MVCC     = sys::MDBX_copy_flags_MDBX_CP_THROTTLE_MVCC as i32;
        // The following two flags refer to txn copy behavior in mdl version; exposed for completeness
        const DISPOSE_TXN       = sys::MDBX_copy_flags_MDBX_CP_DISPOSE_TXN as i32;
        const RENEW_TXN         = sys::MDBX_copy_flags_MDBX_CP_RENEW_TXN as i32;
    }
}

impl CopyFlags {
    #[inline]
    fn to_raw(self) -> sys::MDBX_copy_flags_t {
        self.bits() as sys::MDBX_copy_flags_t
    }
}

#[derive(Copy, Clone, Debug)]
pub enum DeleteMode {
    JustDelete,
    EnsureUnused,
    WaitForUnused,
}

impl DeleteMode {
    fn to_raw(self) -> sys::MDBX_env_delete_mode_t {
        match self {
            DeleteMode::JustDelete => sys::MDBX_env_delete_mode_MDBX_ENV_JUST_DELETE,
            DeleteMode::EnsureUnused => sys::MDBX_env_delete_mode_MDBX_ENV_ENSURE_UNUSED,
            DeleteMode::WaitForUnused => sys::MDBX_env_delete_mode_MDBX_ENV_WAIT_FOR_UNUSED,
        }
    }
}

/// Estimate number of elements between two bounds. begin/end data are only for DUPSORT.
pub fn estimate_range(
    txn: &Txn<'_>,
    dbi: Dbi,
    begin_key: Option<&[u8]>,
    begin_data: Option<&[u8]>,
    end_key: Option<&[u8]>,
    end_data: Option<&[u8]>,
) -> Result<isize> {
    let bk = begin_key.map(|s| to_val(s));
    let bd = begin_data.map(|s| to_val(s));
    let ek = end_key.map(|s| to_val(s));
    let ed = end_data.map(|s| to_val(s));
    let mut out: isize = 0;
    let rc = unsafe {
        sys::mdbx_estimate_range(
            txn.ptr as *const _,
            dbi.0,
            bk.as_ref().map_or(std::ptr::null(), |v| v as *const _),
            bd.as_ref().map_or(std::ptr::null(), |v| v as *const _),
            ek.as_ref().map_or(std::ptr::null(), |v| v as *const _),
            ed.as_ref().map_or(std::ptr::null(), |v| v as *const _),
            &mut out as *mut _,
        )
    } as i32;
    check(rc).map(|_| out)
}

impl<'t> Drop for Cursor<'t> {
    fn drop(&mut self) {
        unsafe { sys::mdbx_cursor_close(self.ptr) }
    }
}

// ----- Unbound cursor (create first, bind later) -----

pub struct UnboundCursor {
    ptr: *mut sys::MDBX_cursor,
}

impl UnboundCursor {
    pub fn create(context: Option<*mut c_void>) -> Option<Self> {
        let ptr = unsafe { sys::mdbx_cursor_create(context.unwrap_or(std::ptr::null_mut())) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    pub fn into_bound<'t>(mut self, txn: &'t mut Txn<'t>, dbi: Dbi) -> Result<Cursor<'t>> {
        let rc = unsafe { sys::mdbx_cursor_bind(txn.ptr, self.ptr, dbi.0) } as i32;
        if rc != 0 {
            return Err(mk_error(rc));
        }
        let ptr = std::mem::replace(&mut self.ptr, std::ptr::null_mut());
        std::mem::forget(self);
        Ok(Cursor {
            ptr,
            _marker: PhantomData,
        })
    }
}

impl Drop for UnboundCursor {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { sys::mdbx_cursor_close(self.ptr) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_op_to_raw() {
        assert_eq!(
            CursorOp::First.to_raw(),
            sys::MDBX_cursor_op_MDBX_FIRST as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::Last.to_raw(),
            sys::MDBX_cursor_op_MDBX_LAST as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::Next.to_raw(),
            sys::MDBX_cursor_op_MDBX_NEXT as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::Prev.to_raw(),
            sys::MDBX_cursor_op_MDBX_PREV as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::Set.to_raw(),
            sys::MDBX_cursor_op_MDBX_SET as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::SetKey.to_raw(),
            sys::MDBX_cursor_op_MDBX_SET_KEY as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::SetRange.to_raw(),
            sys::MDBX_cursor_op_MDBX_SET_RANGE as sys::MDBX_cursor_op
        );
        assert_eq!(
            CursorOp::GetCurrent.to_raw(),
            sys::MDBX_cursor_op_MDBX_GET_CURRENT as sys::MDBX_cursor_op
        );
    }

    #[test]
    fn test_cursor_op_debug() {
        let op = CursorOp::First;
        let debug_str = format!("{:?}", op);
        assert!(debug_str.contains("First"));
    }

    #[test]
    fn test_cursor_op_equality() {
        assert_eq!(CursorOp::First, CursorOp::First);
        assert_ne!(CursorOp::First, CursorOp::Last);
    }

    #[test]
    fn test_error_code_to_raw() {
        assert_eq!(
            ErrorCode::Success.to_raw(),
            sys::MDBX_error_MDBX_SUCCESS as sys::MDBX_error
        );
        assert_eq!(
            ErrorCode::NotFound.to_raw(),
            sys::MDBX_error_MDBX_NOTFOUND as sys::MDBX_error
        );
        assert_eq!(
            ErrorCode::KeyExist.to_raw(),
            sys::MDBX_error_MDBX_KEYEXIST as sys::MDBX_error
        );
        assert_eq!(
            ErrorCode::Corrupted.to_raw(),
            sys::MDBX_error_MDBX_CORRUPTED as sys::MDBX_error
        );
        assert_eq!(
            ErrorCode::MapFull.to_raw(),
            sys::MDBX_error_MDBX_MAP_FULL as sys::MDBX_error
        );
        assert_eq!(
            ErrorCode::Busy.to_raw(),
            sys::MDBX_error_MDBX_BUSY as sys::MDBX_error
        );
    }

    #[test]
    fn test_error_code_from_raw() {
        unsafe {
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_SUCCESS as i32),
                ErrorCode::Success
            );
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_NOTFOUND as i32),
                ErrorCode::NotFound
            );
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_KEYEXIST as i32),
                ErrorCode::KeyExist
            );
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_CORRUPTED as i32),
                ErrorCode::Corrupted
            );
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_MAP_FULL as i32),
                ErrorCode::MapFull
            );
            assert_eq!(
                ErrorCode::from_raw(sys::MDBX_error_MDBX_BUSY as i32),
                ErrorCode::Busy
            );
        }
    }

    #[test]
    fn test_error_code_roundtrip() {
        unsafe {
            let codes = vec![
                ErrorCode::Success,
                ErrorCode::NotFound,
                ErrorCode::KeyExist,
                ErrorCode::Corrupted,
                ErrorCode::MapFull,
                ErrorCode::Busy,
                ErrorCode::Problem,
                ErrorCode::Invalid,
                ErrorCode::Panic,
                ErrorCode::VersionMismatch,
            ];

            for code in codes {
                let raw = code.to_raw() as i32;
                let converted_back = ErrorCode::from_raw(raw);
                assert_eq!(code, converted_back);
            }
        }
    }

    #[test]
    fn test_error_code_debug() {
        let code = ErrorCode::Success;
        let debug_str = format!("{:?}", code);
        assert!(debug_str.contains("Success"));
    }

    #[test]
    fn test_error_code_equality() {
        assert_eq!(ErrorCode::Success, ErrorCode::Success);
        assert_ne!(ErrorCode::Success, ErrorCode::NotFound);
    }

    #[test]
    fn test_option_key_to_raw() {
        assert_eq!(OptionKey::MaxDbs.to_raw(), sys::MDBX_option_MDBX_opt_max_db);
        assert_eq!(
            OptionKey::MaxReaders.to_raw(),
            sys::MDBX_option_MDBX_opt_max_readers
        );
        assert_eq!(
            OptionKey::SyncBytes.to_raw(),
            sys::MDBX_option_MDBX_opt_sync_bytes
        );
        assert_eq!(
            OptionKey::SyncPeriod.to_raw(),
            sys::MDBX_option_MDBX_opt_sync_period
        );
    }

    #[test]
    fn test_option_key_debug() {
        let key = OptionKey::MaxDbs;
        let debug_str = format!("{:?}", key);
        assert!(debug_str.contains("MaxDbs"));
    }

    #[test]
    fn test_mk_error() {
        let error = mk_error(0);
        assert_eq!(error.code(), 0);
        assert!(!error.message().is_empty());

        let error = mk_error(-30798); // MDBX_NOTFOUND
        assert_eq!(error.code(), -30798);
        assert!(!error.message().is_empty());
    }

    #[test]
    fn test_error_display() {
        let error = mk_error(-30798);
        let display_str = format!("{}", error);
        assert!(display_str.contains("MDBX error"));
        assert!(display_str.contains("-30798"));
    }

    #[test]
    fn test_error_debug() {
        let error = mk_error(0);
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Error"));
        assert!(debug_str.contains("code"));
        assert!(debug_str.contains("message"));
    }

    #[test]
    fn test_check_success() {
        let result = check(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_error() {
        let result = check(-30798); // MDBX_NOTFOUND
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.code(), -30798);
    }

    #[test]
    fn test_to_val() {
        let data = b"hello world";
        let val = to_val(data);
        assert_eq!(val.iov_len, data.len());
        assert!(!val.iov_base.is_null());
    }

    #[test]
    fn test_env_flags_bitflags() {
        let flags = EnvFlags::DEFAULTS | EnvFlags::RDONLY;
        assert!(flags.contains(EnvFlags::DEFAULTS));
        assert!(flags.contains(EnvFlags::RDONLY));
        assert!(!flags.contains(EnvFlags::EXCLUSIVE));
    }

    #[test]
    fn test_db_flags_bitflags() {
        let flags = DbFlags::DEFAULTS | DbFlags::DUPSORT;
        assert!(flags.contains(DbFlags::DEFAULTS));
        assert!(flags.contains(DbFlags::DUPSORT));
        assert!(!flags.contains(DbFlags::REVERSEKEY));
    }

    #[test]
    fn test_put_flags_bitflags() {
        let flags = PutFlags::UPSERT | PutFlags::NOOVERWRITE;
        assert!(flags.contains(PutFlags::UPSERT));
        assert!(flags.contains(PutFlags::NOOVERWRITE));
        assert!(!flags.contains(PutFlags::NODUPDATA));
    }

    #[test]
    fn test_warmup_flags_bitflags() {
        let flags = WarmupFlags::DEFAULT | WarmupFlags::FORCE;
        assert!(flags.contains(WarmupFlags::DEFAULT));
        assert!(flags.contains(WarmupFlags::FORCE));
        assert!(!flags.contains(WarmupFlags::LOCK));
    }

    #[test]
    fn test_error_code_all_variants() {
        // Test that all ErrorCode variants can be converted to raw and back
        unsafe {
            let variants = vec![
                ErrorCode::Success,
                ErrorCode::ResultTrue,
                ErrorCode::KeyExist,
                ErrorCode::NotFound,
                ErrorCode::PageNotFound,
                ErrorCode::Corrupted,
                ErrorCode::Panic,
                ErrorCode::VersionMismatch,
                ErrorCode::Invalid,
                ErrorCode::MapFull,
                ErrorCode::DbsFull,
                ErrorCode::ReadersFull,
                ErrorCode::TxnFull,
                ErrorCode::CursorFull,
                ErrorCode::PageFull,
                ErrorCode::UnableExtendMapsize,
                ErrorCode::Incompatible,
                ErrorCode::BadRslot,
                ErrorCode::BadTxn,
                ErrorCode::BadValsize,
                ErrorCode::BadDbi,
                ErrorCode::Problem,
                ErrorCode::Busy,
                ErrorCode::Emultival,
                ErrorCode::Ebadsign,
                ErrorCode::WannaRecovery,
                ErrorCode::Ekeymismatch,
                ErrorCode::TooLarge,
                ErrorCode::ThreadMismatch,
                ErrorCode::TxnOverlapping,
                ErrorCode::BacklogDepleted,
                ErrorCode::DuplicatedClk,
                ErrorCode::DanglingDbi,
                ErrorCode::Ousted,
                ErrorCode::MvccRetarded,
                ErrorCode::Enodata,
                ErrorCode::Einval,
                ErrorCode::Eaccess,
                ErrorCode::Enomem,
                ErrorCode::Erofs,
                ErrorCode::Enosys,
                ErrorCode::Eio,
                ErrorCode::Eperm,
                ErrorCode::Eintr,
                ErrorCode::EnoFile,
                ErrorCode::Eremote,
                ErrorCode::Edeadlk,
            ];

            for variant in variants {
                let raw = variant.to_raw() as i32;
                let converted_back = ErrorCode::from_raw(raw);
                assert_eq!(
                    variant, converted_back,
                    "Failed roundtrip for {:?}",
                    variant
                );
            }
        }
    }

    #[test]
    fn test_cursor_op_all_variants() {
        // Test that all CursorOp variants can be converted to raw
        let variants = vec![
            CursorOp::First,
            CursorOp::FirstDup,
            CursorOp::GetBoth,
            CursorOp::GetBothRange,
            CursorOp::GetCurrent,
            CursorOp::GetMultiple,
            CursorOp::Last,
            CursorOp::LastDup,
            CursorOp::Next,
            CursorOp::NextDup,
            CursorOp::NextMultiple,
            CursorOp::NextNoDup,
            CursorOp::Prev,
            CursorOp::PrevDup,
            CursorOp::PrevNoDup,
            CursorOp::Set,
            CursorOp::SetKey,
            CursorOp::SetRange,
            CursorOp::PrevMultiple,
            CursorOp::SetLowerBound,
            CursorOp::SetUpperBound,
            CursorOp::ToKeyLesserThan,
            CursorOp::ToKeyLesserOrEqual,
            CursorOp::ToKeyEqual,
            CursorOp::ToKeyGreaterOrEqual,
            CursorOp::ToKeyGreaterThan,
            CursorOp::ToExactKeyValueLesserThan,
            CursorOp::ToExactKeyValueLesserOrEqual,
            CursorOp::ToExactKeyValueEqual,
            CursorOp::ToExactKeyValueGreaterOrEqual,
            CursorOp::ToExactKeyValueGreaterThan,
            CursorOp::ToPairLesserThan,
            CursorOp::ToPairLesserOrEqual,
            CursorOp::ToPairEqual,
            CursorOp::ToPairGreaterOrEqual,
            CursorOp::ToPairGreaterThan,
            CursorOp::SeekAndGetMultiple,
        ];

        for variant in variants {
            let raw = variant.to_raw();
            // Just ensure it doesn't panic and returns a valid value
            assert!(raw >= 0);
        }
    }

    #[test]
    fn test_error_code_clone() {
        let code = ErrorCode::Success;
        let cloned = code.clone();
        assert_eq!(code, cloned);
    }

    #[test]
    fn test_cursor_op_clone() {
        let op = CursorOp::First;
        let cloned = op.clone();
        assert_eq!(op, cloned);
    }

    #[test]
    fn test_option_key_clone() {
        let key = OptionKey::MaxDbs;
        let cloned = key.clone();
        // Note: OptionKey doesn't implement PartialEq, so we can't compare directly
        // but we can verify the clone doesn't panic
        let _cloned = cloned;
    }
}
