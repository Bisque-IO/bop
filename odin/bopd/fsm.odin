package bopd

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:thread"

import bop "../odin/libbop"

FSM :: struct {
	allocator: runtime.Allocator,
}

/*
Commit the given Raft log.

NOTE:
  Given memory buffer is owned by caller, so that
  commit implementation should clone it if user wants to
  use the memory even after the commit call returns.

  Here provide a default implementation for facilitating the
  situation when application does not care its implementation.

@param log_idx Raft log number to commit.
@param data Payload of the Raft log.

@return Result value of state machine.
*/
fsm_commit :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
	result: ^^bop.Raft_Buffer,
) {
	context = tls_context()
}

/*
(Optional)
Handler on the commit of a configuration change.

@param log_idx Raft log number of the configuration change.
@param new_conf New cluster configuration.
*/
fsm_commit_config :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	new_conf: ^bop.Raft_Cluster_Config,
) {
	context = tls_context()
	runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()
}

/*
Pre-commit the given Raft log.

Pre-commit is called after appending Raft log,
before getting acks from quorum nodes.
Users can ignore this function if not needed.

Same as `commit()`, memory buffer is owned by caller.

@param log_idx Raft log number to commit.
@param data Payload of the Raft log.

@return Result value of state machine.
*/
fsm_pre_commit :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
	result: ^^bop.Raft_Buffer,
) {
	context = tls_context()
}

/*
Rollback the state machine to given Raft log number.

It will be called for uncommitted Raft logs only,
so that users can ignore this function if they don't
do anything on pre-commit.

Same as `commit()`, memory buffer is owned by caller.

@param log_idx Raft log number to commit.
@param data Payload of the Raft log.
*/
fsm_rollback :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
) {

}

/*
(Optional)
Handler on the rollback of a configuration change.
The configuration can be either committed or uncommitted one,
and that can be checked by the given `log_idx`, comparing it with
the current `cluster_config`'s log index.

@param log_idx Raft log number of the configuration change.
@param conf The cluster configuration to be rolled back.
*/
fsm_rollback_config :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	new_conf: ^bop.Raft_Cluster_Config,
) {

}

/*
(Optional)
Return a hint about the preferred size (in number of bytes)
of the next batch of logs to be sent from the leader.

Only applicable on followers.

@return The preferred size of the next log batch.
        `0` indicates no preferred size (any size is good).
        `positive value` indicates at least one log can be sent,
        (the size of that log may be bigger than this hint size).
        `negative value` indicates no log should be sent since this
        follower is busy handling pending logs.
*/
fsm_get_next_batch_size_hint_in_bytes :: proc "c" (user_data: rawptr) -> i64 {
	context = tls_context()
	runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()
	log.info("fsm_get_next_batch_size_hint_in_bytes")
	return i64(102464)
}

/*
Save the given snapshot object to local snapshot.
This API is for snapshot receiver (i.e., follower).

This is an optional API for users who want to use logical
snapshot. Instead of splitting a snapshot into multiple
physical chunks, this API uses logical objects corresponding
to a unique object ID. Users are responsible for defining
what object is: it can be a key-value pair, a set of
key-value pairs, or whatever.

Same as `commit()`, memory buffer is owned by caller.

@param s Snapshot instance to save.
@param obj_id[in,out]
    Object ID.
    As a result of this API call, the next object ID
    that reciever wants to get should be set to
    this parameter.
@param data Payload of given object.
@param is_first_obj `true` if this is the first object.
@param is_last_obj `true` if this is the last object.
*/
fsm_save_snapshot :: proc "c" (
	user_data: rawptr,
	last_log_idx: u64,
	last_log_term: u64,
	last_config: ^bop.Raft_Cluster_Config,
	size: uintptr,
	type: u8,
	is_first_obj: bool,
	is_last_obj: bool,
	data: [^]byte,
	data_size: uintptr,
) {

}

/*
Apply received snapshot to state machine.

@param s Snapshot instance to apply.

@returm `true` on success.
*/
fsm_apply_snapshot :: proc "c" (
	user_data: rawptr,
	last_log_idx: u64,
	last_log_term: u64,
	last_config: ^bop.Raft_Cluster_Config,
	size: uintptr,
	type: u8,
) -> bool {
	return true
}

/*
Read the given snapshot object.
This API is for snapshot sender (i.e., leader).

Same as above, this is an optional API for users who want to
use logical snapshot.

@param s Snapshot instance to read.
@param[in,out] user_snp_ctx
    User-defined instance that needs to be passed through
    the entire snapshot read. It can be a pointer to
    state machine specific iterators, or whatever.
    On the first `read_logical_snp_obj` call, it will be
    set to `null`, and this API may return a new pointer if necessary.
    Returned pointer will be passed to next `read_logical_snp_obj`
    call.
@param obj_id Object ID to read.
@param[out] data Buffer where the read object will be stored.
@param[out] is_last_obj Set `true` if this is the last object.

@return Negative number if failed.
*/
fsm_read_snapshot :: proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr,
	obj_id: u64,
	data_out: ^^bop.Raft_Buffer,
	is_last_obj: ^bool,
) -> i32 {
	return 0
}

/*
Free user-defined instance that is allocated by
`read_logical_snp_obj`.
This is an optional API for users who want to use logical snapshot.

@param user_snp_ctx User-defined instance to free.
*/
fsm_free_user_snapshot_ctx :: proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr,
) {

}

/*
Get the latest snapshot instance.

This API will be invoked at the initialization of Raft server,
so that the last last snapshot should be durable for server restart,
if you want to avoid unnecessary catch-up.

@return Pointer to the latest snapshot.
*/
fsm_last_snapshot :: proc "c" (user_data: rawptr) -> ^bop.Raft_Snapshot {
	return nil
}

/*
Get the last committed Raft log number.

This API will be invoked at the initialization of Raft server
to identify what the last committed point is, so that the last
committed index number should be durable for server restart,
if you want to avoid unnecessary catch-up.

@return Last committed Raft log number.
*/
fsm_last_commit_index :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

/*
Create a snapshot corresponding to the given info.

@param s Snapshot info to create.
@param when_done Callback function that will be called after
                 snapshot creation is done.
*/
fsm_create_snapshot :: proc "c" (
	user_data: rawptr,
	snapshot: ^bop.Raft_Snapshot,
	snapshot_data: ^bop.Raft_Buffer,
	snp_data: [^]byte,
	snp_data_size: uintptr,
) {

}

/*
Decide to create snapshot or not.
Once the pre-defined condition is satisfied, Raft core will invoke
this function to ask if it needs to create a new snapshot.
If user-defined state machine does not want to create snapshot
at this time, this function will return `false`.

@return `true` if wants to create snapshot.
        `false` if does not want to create snapshot.
*/
fsm_chk_create_snapshot :: proc "c" (user_data: rawptr) -> bool {
	return true
}

/*
Decide to transfer leadership.
Once the other conditions are met, Raft core will invoke
this function to ask if it is allowed to transfer the
leadership to other member.

@return `true` if wants to transfer leadership.
        `false` if not.
*/
fsm_allow_leadership_transfer :: proc "c" (user_data: rawptr) -> bool {
	return true
}

/*
This function will be called when Raft succeeds in replicating logs
to an arbitrary follower and attempts to commit logs. Users can manually
adjust the commit index. The adjusted commit index should be equal to
or greater than the given `current_commit_index`. Otherwise, no log
will be committed.

@param params Parameters.
@return Adjusted commit index.
*/
fsm_adjust_commit_index :: proc "c" (
	user_data: rawptr,
	current_commit_index: u64,
	expected_commit_index: u64,
	params: ^bop.Raft_FSM_Adjust_Commit_Index_Params,
) {

}

import "core:testing"

@(test)
test_fsm_make_delete :: proc(t: ^testing.T) {
	user_data: rawptr = nil
	current_conf: ^bop.Raft_Cluster_Config = nil
	rollback_conf: ^bop.Raft_Cluster_Config = nil
	fsm := bop.raft_fsm_make(
		user_data = user_data,
		current_conf = current_conf,
		rollback_conf = rollback_conf,
		commit = fsm_commit,
		commit_config = fsm_commit_config,
		pre_commit = fsm_pre_commit,
		rollback = fsm_rollback,
		rollback_config = fsm_rollback_config,
		get_next_batch_size_hint_in_bytes = fsm_get_next_batch_size_hint_in_bytes,
		save_snapshot = fsm_save_snapshot,
		apply_snapshot = fsm_apply_snapshot,
		read_snapshot = fsm_read_snapshot,
		free_snapshot_user_ctx = fsm_free_user_snapshot_ctx,
		last_snapshot = fsm_last_snapshot,
		last_commit_index = fsm_last_commit_index,
		create_snapshot = fsm_create_snapshot,
		chk_create_snapshot = fsm_chk_create_snapshot,
		allow_leadership_transfer = fsm_allow_leadership_transfer,
		adjust_commit_index = fsm_adjust_commit_index,
	)

	fsm_get_next_batch_size_hint_in_bytes(nil)

	ensure(fsm != nil, "raft_fsm_make returned nil")
	bop.raft_fsm_delete(fsm)
}
