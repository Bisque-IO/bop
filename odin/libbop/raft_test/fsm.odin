package raft_test

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:thread"

import bop "../"

@thread_local
local_context: runtime.Context

load_context :: #force_inline proc "contextless" () -> runtime.Context {
	c := local_context
	if c.allocator.procedure == nil {
		c = runtime.default_context()
		context = c
		context.logger = log.create_console_logger(.Debug)
		c = context
		log.info("created thread_local context")
		local_context = c
	}
    return c
}

fsm_commit :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
	result: ^^bop.Raft_Buffer,
) {
	context = load_context()
}

fsm_commit_config :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	new_conf: ^bop.Raft_Cluster_Config
) {
	context = load_context()
	runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()
}

fsm_pre_commit :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr,
	result: ^^bop.Raft_Buffer,
) {
	context = load_context()
}

fsm_rollback :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	data: [^]byte,
	size: uintptr
) {

}

fsm_rollback_config :: proc "c" (
	user_data: rawptr,
	log_idx: u64,
	new_conf: ^bop.Raft_Cluster_Config
) {

}

fsm_get_next_batch_size_hint_in_bytes :: proc "c" (user_data: rawptr) -> i64 {
	context = load_context()
	runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()
	log.info("fsm_get_next_batch_size_hint_in_bytes")
	return i64(1024 * 64)
}

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

fsm_read_snapshot :: proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr,
	obj_id: u64,
	data_out: ^^bop.Raft_Buffer,
	is_last_obj: ^bool,
) -> i32 {
	return 0
}

fsm_free_user_snapshot_ctx :: proc "c" (
	user_data: rawptr,
	user_snapshot_ctx: ^rawptr
) {

}

fsm_last_snapshot :: proc "c" (user_data: rawptr) -> ^bop.Raft_Snapshot {
	return nil
}

fsm_last_commit_index :: proc "c" (user_data: rawptr) -> u64 {
	return 0
}

fsm_create_snapshot :: proc "c" (
	user_data: rawptr,
	snapshot: ^bop.Raft_Snapshot,
	snapshot_data: ^bop.Raft_Buffer,
	snp_data: [^]byte,
	snp_data_size: uintptr,
) {

}

fsm_chk_create_snapshot :: proc "c" (user_data: rawptr) -> bool {
	return true
}

fsm_allow_leadership_transfer :: proc "c" (user_data: rawptr) -> bool {
	return true
}

fsm_adjust_commit_index :: proc "c" (
	user_data: rawptr,
	current_commit_index: u64,
	expected_commit_index: u64,
	params: ^bop.Raft_FSM_Adjust_Commit_Index_Params,
) {

}

import "core:testing"

@test
test_fsm :: proc(t: ^testing.T) {
	user_data : rawptr = nil
	current_conf : ^bop.Raft_Cluster_Config = nil
	rollback_conf : ^bop.Raft_Cluster_Config = nil
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
		adjust_commit_index = fsm_adjust_commit_index
	)

	fsm_get_next_batch_size_hint_in_bytes(nil)

	ensure(fsm != nil, "raft_fsm_make returned nil")
	bop.raft_fsm_delete(fsm)

	log.info("passed!")
	fmt.println("passed!")
}
