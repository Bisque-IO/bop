package raft_test

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:thread"

import bop "../"

State_Mgr :: struct {}

/*
Load the last saved cluster config.
This function will be invoked on initialization of
Raft server.

Even at the very first initialization, it should
return proper initial cluster config, not `nullptr`.
The initial cluster config must include the server itself.

@return Cluster config.
*/
state_mgr_load_config :: proc "c" (user_data: rawptr) -> ^bop.Raft_Cluster_Config {
	return nil
}

/*
Save given cluster config.

@param config Cluster config to save.
*/
state_mgr_save_config :: proc "c" (user_data: rawptr, config: ^bop.Raft_Cluster_Config) {

}

/*
Load the last saved server state.
This function will be invoked on initialization of
Raft server

At the very first initialization, it should return
`nullptr`.

@param Server state.
*/
state_mgr_read_state :: proc "c" (user_data: rawptr) -> ^bop.Raft_Srv_State {
	return nil
}

/*
Save given server state.

@param state Server state to save.
*/
state_mgr_save_state :: proc "c" (user_data: rawptr, state: ^bop.Raft_Srv_State) {

}

/*
Get instance of user-defined Raft log store.

@param Raft log store instance.
*/
state_mgr_load_log_store :: proc "c" (user_data: rawptr) -> ^bop.Raft_Log_Store_Ptr {
	return nil
}

/*
Get ID of this Raft server.

@return Server ID.
*/
state_mgr_server_id :: proc "c" (user_data: rawptr) -> i32 {
	return 0
}

/*
System exit handler. This function will be invoked on
abnormal termination of Raft server.

@param exit_code Error code.
*/
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

