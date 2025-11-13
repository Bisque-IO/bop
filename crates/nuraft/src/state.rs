use std::marker::PhantomData;
use std::ptr::NonNull;

use maniac_sys::*;

use crate::buffer::Buffer;
use crate::error::{RaftError, RaftResult};
use crate::types::{LogIndex, ServerId, Term};

/// Wrapper for the Raft server state structure.
pub struct ServerState {
    ptr: NonNull<bop_raft_srv_state>,
}

impl ServerState {
    /// Construct from a raw pointer.
    #[allow(dead_code)]
    pub(crate) unsafe fn from_raw(ptr: *mut bop_raft_srv_state) -> RaftResult<Self> {
        NonNull::new(ptr)
            .map(|ptr| ServerState { ptr })
            .ok_or(RaftError::NullPointer)
    }

    /// Get current term.
    pub fn term(&self) -> Term {
        Term(unsafe { bop_raft_srv_state_term(self.ptr.as_ptr()) })
    }

    /// Get who this server voted for.
    pub fn voted_for(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_state_voted_for(self.ptr.as_ptr()) })
    }

    /// Check if election timer is allowed.
    pub fn is_election_timer_allowed(&self) -> bool {
        unsafe { bop_raft_srv_state_is_election_timer_allowed(self.ptr.as_ptr()) }
    }

    /// Check if server is catching up.
    pub fn is_catching_up(&self) -> bool {
        unsafe { bop_raft_srv_state_is_catching_up(self.ptr.as_ptr()) }
    }

    /// Check if server is receiving a snapshot.
    pub fn is_receiving_snapshot(&self) -> bool {
        unsafe { bop_raft_srv_state_is_receiving_snapshot(self.ptr.as_ptr()) }
    }

    /// Serialize the state into a buffer.
    pub fn serialize(&self) -> RaftResult<Buffer> {
        let ptr = unsafe { bop_raft_srv_state_serialize(self.ptr.as_ptr()) };
        unsafe { Buffer::from_raw(ptr) }
    }

    /// Transfer ownership of the underlying raw pointer to the caller.
    ///
    /// After calling this the [`ServerState`] instance must not be used.
    #[allow(dead_code)]
    pub(crate) fn into_raw(self) -> *mut bop_raft_srv_state {
        let ptr = self.ptr.as_ptr();
        std::mem::forget(self);
        ptr
    }
}

impl Drop for ServerState {
    fn drop(&mut self) {
        unsafe {
            bop_raft_srv_state_delete(self.ptr.as_ptr());
        }
    }
}

unsafe impl Send for ServerState {}
unsafe impl Sync for ServerState {}

/// Borrowed view into a server state pointer owned by the C++ layer.
#[derive(Clone, Copy)]
pub struct ServerStateView<'a> {
    ptr: *const bop_raft_srv_state,
    _marker: PhantomData<&'a bop_raft_srv_state>,
}

impl<'a> ServerStateView<'a> {
    /// Construct a view from a raw pointer.
    #[allow(dead_code)]
    pub(crate) unsafe fn new(ptr: *const bop_raft_srv_state) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self {
                ptr,
                _marker: PhantomData,
            })
        }
    }

    /// Get current term.
    pub fn term(&self) -> Term {
        Term(unsafe { bop_raft_srv_state_term(self.ptr) })
    }

    /// Get who this server voted for.
    pub fn voted_for(&self) -> ServerId {
        ServerId(unsafe { bop_raft_srv_state_voted_for(self.ptr) })
    }

    /// Check if election timer is allowed.
    pub fn is_election_timer_allowed(&self) -> bool {
        unsafe { bop_raft_srv_state_is_election_timer_allowed(self.ptr) }
    }

    /// Check if server is catching up.
    pub fn is_catching_up(&self) -> bool {
        unsafe { bop_raft_srv_state_is_catching_up(self.ptr) }
    }

    /// Check if server is receiving a snapshot.
    pub fn is_receiving_snapshot(&self) -> bool {
        unsafe { bop_raft_srv_state_is_receiving_snapshot(self.ptr) }
    }

    /// Access the raw pointer for advanced interop.
    #[allow(dead_code)]
    pub(crate) fn as_ptr(&self) -> *const bop_raft_srv_state {
        self.ptr
    }
}

unsafe impl<'a> Send for ServerStateView<'a> {}
unsafe impl<'a> Sync for ServerStateView<'a> {}

/// Peer information for other servers in the cluster.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub server_id: ServerId,
    pub last_log_idx: LogIndex,
    pub last_succ_resp_us: u64,
}
