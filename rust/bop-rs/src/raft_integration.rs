//! Raft integration with openraft - placeholder module
//! 
//! This module provides integration between BOP's custom Raft wrapper
//! and the standard openraft crate when needed.

// Note: This is a placeholder to resolve module import errors.
// The actual openraft integration is commented out in lib.rs because
// the openraft dependency is not currently enabled.

pub type BopNodeId = u32;
pub type BopNode = String;
pub type BopRequest = String;
pub type BopResponse = String;
pub type BopRaftLogStorage = ();
pub type BopRaftStateMachine = ();
pub type BopRaftNetwork = ();
pub type BopTypeConfig = ();