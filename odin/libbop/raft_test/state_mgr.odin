package raft_test

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:thread"

import bop "../"

state_mgr_load_config :: proc "c" (user_data: rawptr) -> ^bop.Raft_Cluster_Config {
	return nil
}

state_mgr_save_config :: proc "c" (user_data: rawptr, config: ^bop.Raft_Cluster_Config) {

}

state_mgr_read_state :: proc "c" (user_data: rawptr) -> ^bop.Raft_Srv_State {
	return nil
}

state_mgr_save_state :: proc "c" (user_data: rawptr, state: ^bop.Raft_Srv_State) {

}

state_mgr_load_log_store :: proc "c" (user_data: rawptr) -> ^bop.Raft_Log_Store_Ptr {
	return nil
}

state_mgr_server_id :: proc "c" (user_data: rawptr) -> i32 {
	return 0
}

state_mgr_system_exit :: proc "c" (user_data: rawptr, exit_code: i32) {

}

wire_state_mgr :: proc() {
	state_mgr := bop.raft_state_mgr_make(
		nil,
		state_mgr_load_config,
		state_mgr_save_config,
		state_mgr_read_state,
		state_mgr_save_state,
		state_mgr_load_log_store,
		state_mgr_server_id,
		state_mgr_system_exit,
	)
	ensure(state_mgr != nil, "state_mgr is nil")
	bop.raft_state_mgr_delete(state_mgr)
}

