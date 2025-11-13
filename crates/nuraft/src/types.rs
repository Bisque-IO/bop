use std::ffi::c_void;
use std::marker::PhantomData;

use maniac_sys::bop_raft_cb_param;

/// Strong type for server IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServerId(pub i32);

/// Strong type for data center IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataCenterId(pub i32);

/// Strong type for Raft terms
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Term(pub u64);

/// Strong type for log indices
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogIndex(pub u64);

/// Strong type for log entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogEntryType(pub i32);

/// Logging verbosity levels exposed by NuRaft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown(i32),
}

impl LogLevel {
    pub fn from_raw(level: i32) -> Self {
        match level {
            0 => LogLevel::Trace,
            1 => LogLevel::Debug,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            4 => LogLevel::Error,
            5 => LogLevel::Fatal,
            other => LogLevel::Unknown(other),
        }
    }
}

/// Callback types for Raft events originating from the underlying NuRaft engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackType {
    ProcessRequest,
    GotAppendEntryRespFromPeer,
    AppendLogs,
    HeartBeat,
    JoinedCluster,
    BecomeLeader,
    RequestAppendEntries,
    SaveSnapshot,
    NewConfig,
    RemovedFromCluster,
    BecomeFollower,
    BecomeFresh,
    BecomeStale,
    GotAppendEntryReqFromLeader,
    OutOfLogRangeWarning,
    ConnectionOpened,
    ConnectionClosed,
    NewSessionFromLeader,
    StateMachineExecution,
    SentAppendEntriesReq,
    ReceivedAppendEntriesReq,
    SentAppendEntriesResp,
    ReceivedAppendEntriesResp,
    AutoAdjustQuorum,
    ServerJoinFailed,
    SnapshotCreationBegin,
    ResignationFromLeader,
    FollowerLost,
    ReceivedMisbehavingMessage,
    Unknown(i32),
}

impl CallbackType {
    pub fn from_raw(raw: i32) -> Self {
        match raw {
            1 => CallbackType::ProcessRequest,
            2 => CallbackType::GotAppendEntryRespFromPeer,
            3 => CallbackType::AppendLogs,
            4 => CallbackType::HeartBeat,
            5 => CallbackType::JoinedCluster,
            6 => CallbackType::BecomeLeader,
            7 => CallbackType::RequestAppendEntries,
            8 => CallbackType::SaveSnapshot,
            9 => CallbackType::NewConfig,
            10 => CallbackType::RemovedFromCluster,
            11 => CallbackType::BecomeFollower,
            12 => CallbackType::BecomeFresh,
            13 => CallbackType::BecomeStale,
            14 => CallbackType::GotAppendEntryReqFromLeader,
            15 => CallbackType::OutOfLogRangeWarning,
            16 => CallbackType::ConnectionOpened,
            17 => CallbackType::ConnectionClosed,
            18 => CallbackType::NewSessionFromLeader,
            19 => CallbackType::StateMachineExecution,
            20 => CallbackType::SentAppendEntriesReq,
            21 => CallbackType::ReceivedAppendEntriesReq,
            22 => CallbackType::SentAppendEntriesResp,
            23 => CallbackType::ReceivedAppendEntriesResp,
            24 => CallbackType::AutoAdjustQuorum,
            25 => CallbackType::ServerJoinFailed,
            26 => CallbackType::SnapshotCreationBegin,
            27 => CallbackType::ResignationFromLeader,
            28 => CallbackType::FollowerLost,
            29 => CallbackType::ReceivedMisbehavingMessage,
            other => CallbackType::Unknown(other),
        }
    }

    pub fn as_raw(self) -> i32 {
        match self {
            CallbackType::ProcessRequest => 1,
            CallbackType::GotAppendEntryRespFromPeer => 2,
            CallbackType::AppendLogs => 3,
            CallbackType::HeartBeat => 4,
            CallbackType::JoinedCluster => 5,
            CallbackType::BecomeLeader => 6,
            CallbackType::RequestAppendEntries => 7,
            CallbackType::SaveSnapshot => 8,
            CallbackType::NewConfig => 9,
            CallbackType::RemovedFromCluster => 10,
            CallbackType::BecomeFollower => 11,
            CallbackType::BecomeFresh => 12,
            CallbackType::BecomeStale => 13,
            CallbackType::GotAppendEntryReqFromLeader => 14,
            CallbackType::OutOfLogRangeWarning => 15,
            CallbackType::ConnectionOpened => 16,
            CallbackType::ConnectionClosed => 17,
            CallbackType::NewSessionFromLeader => 18,
            CallbackType::StateMachineExecution => 19,
            CallbackType::SentAppendEntriesReq => 20,
            CallbackType::ReceivedAppendEntriesReq => 21,
            CallbackType::SentAppendEntriesResp => 22,
            CallbackType::ReceivedAppendEntriesResp => 23,
            CallbackType::AutoAdjustQuorum => 24,
            CallbackType::ServerJoinFailed => 25,
            CallbackType::SnapshotCreationBegin => 26,
            CallbackType::ResignationFromLeader => 27,
            CallbackType::FollowerLost => 28,
            CallbackType::ReceivedMisbehavingMessage => 29,
            CallbackType::Unknown(other) => other,
        }
    }
}

/// Result classification for priority updates when interacting with a Raft server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PrioritySetResult {
    Set = 1,
    Broadcast = 2,
    Ignored = 3,
}

/// Action returned to the NuRaft callback dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackAction {
    Continue,
    ReturnNull,
}

/// Lightweight view over the parameters supplied by NuRaft callbacks.
pub struct CallbackParam<'a> {
    ptr: *mut bop_raft_cb_param,
    _marker: PhantomData<&'a mut bop_raft_cb_param>,
}

impl<'a> CallbackParam<'a> {
    pub fn my_id(&self) -> ServerId {
        if self.ptr.is_null() {
            ServerId::new(0)
        } else {
            ServerId::new(unsafe { (*self.ptr).my_id })
        }
    }

    pub fn leader_id(&self) -> ServerId {
        if self.ptr.is_null() {
            ServerId::new(0)
        } else {
            ServerId::new(unsafe { (*self.ptr).leader_id })
        }
    }

    pub fn peer_id(&self) -> ServerId {
        if self.ptr.is_null() {
            ServerId::new(0)
        } else {
            ServerId::new(unsafe { (*self.ptr).peer_id })
        }
    }

    pub fn user_ctx(&self) -> *mut c_void {
        if self.ptr.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { (*self.ptr).ctx }
        }
    }

    pub fn as_raw(&self) -> *mut bop_raft_cb_param {
        self.ptr
    }
}

/// Fully-typed callback context delivered to [`ServerCallbacks`].
pub struct CallbackContext<'a> {
    ty: CallbackType,
    param: CallbackParam<'a>,
}

impl<'a> CallbackContext<'a> {
    pub fn callback_type(&self) -> CallbackType {
        self.ty
    }

    pub fn param(&self) -> &CallbackParam<'a> {
        &self.param
    }

    pub(crate) unsafe fn from_raw(ty: CallbackType, param: *mut bop_raft_cb_param) -> Self {
        Self {
            ty,
            param: CallbackParam {
                ptr: param,
                _marker: PhantomData,
            },
        }
    }
}

impl ServerId {
    /// Create a new server ID
    pub fn new(id: i32) -> Self {
        ServerId(id)
    }

    /// Get the inner value
    pub fn inner(self) -> i32 {
        self.0
    }
}

impl DataCenterId {
    /// Create a new data center ID
    pub fn new(id: i32) -> Self {
        DataCenterId(id)
    }

    /// Get the inner value
    pub fn inner(self) -> i32 {
        self.0
    }
}

impl Term {
    /// Create a new term
    pub fn new(term: u64) -> Self {
        Term(term)
    }

    /// Get the inner value
    pub fn inner(self) -> u64 {
        self.0
    }

    /// Get the next term value
    pub fn next(self) -> Self {
        Term(self.0 + 1)
    }
}

impl LogIndex {
    /// Create a new log index
    pub fn new(index: u64) -> Self {
        LogIndex(index)
    }

    /// Get the inner value
    pub fn inner(self) -> u64 {
        self.0
    }

    /// Get the next index
    pub fn next(self) -> Self {
        LogIndex(self.0 + 1)
    }

    /// Get the previous index, returns `None` if already at zero
    pub fn prev(self) -> Option<Self> {
        if self.0 > 0 {
            Some(LogIndex(self.0 - 1))
        } else {
            None
        }
    }
}
