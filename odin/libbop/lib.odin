package libbop

import "core:fmt"
import "core:testing"

import "core:c"

#assert(size_of(c.int) == size_of(i32))

when ODIN_OS == .Windows && ODIN_ARCH == .amd64 {
	when #config(BOP_SHARED, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/windows/x64/release/bop.lib"
	} else {
		@(private)
		LIB_PATH :: "../../build/windows/x64/debug/bop-static.lib"
	}

	when #config(BOP_DEBUG, 1) == 1 {
		@(private)
		MSVCRT_NAME :: "system:msvcrtd.lib"
	} else {
		@(private)
//		MSVCRT_NAME :: "system:crt.lib"
		MSVCRT_NAME :: "system:msvcrt.lib"
	}
	
	//odinfmt:disable
	foreign import lib {
//	    "windows/amd64/libcrypto_static.lib",
//	    "windows/amd64/libssl_static.lib",
		"windows/amd64/libcrypto.lib",
		"windows/amd64/libssl.lib",
		"system:Kernel32.lib",
		"system:User32.lib",
		"system:Advapi32.lib",
		"system:ntdll.lib",
		"system:onecore.lib",
		"system:Synchronization.lib",
		MSVCRT_NAME,
		LIB_PATH,
 	}
	//odinfmt:enable
} else when ODIN_OS == .Windows && ODIN_ARCH == .arm64 {
	#panic("libbop does not support Windows ARM64 yet")
} else when ODIN_OS == .Linux && ODIN_ARCH == .amd64 {
	when #config(BOP_DEBUG, 1) == 1 {
		@(private)
		LIB_PATH :: "../../build/linux/x86_64/release/libbop-static.a"
		//		LIB_PATH :: "../../build/linux/x86_64/release/libbop.so"
		//		LIB_PATH :: "libbop.so"
	} else {
		@(private)
		LIB_PATH :: "linux/amd64/libbop-static.a"
	}

	// odin build . -o:aggressive -define:BOP_DEBUG=1 -extra-linker-flags:"-Wl,-rpath,$ORIGIN/libbop.so" -linker:lld
	//odinfmt:disable
	foreign import lib {
//		"system:dl",
//		"system:pthread",
//		"linux/amd64/libcrypto.a",
//		"linux/amd64/libssl.a",
		"system:crypto",
		"system:ssl",
		"system:stdc++",
		LIB_PATH,
	}
	//odinfmt:enable
} else when ODIN_OS == .Linux && ODIN_ARCH == .arm64 {
	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/linux/aarch64/debug/libbop.a"
	} else {
		@(private)
		LIB_PATH :: "linux/arm64/libbop.a"
	}
	//odinfmt:disable
	foreign import lib {
		"system:libm",
		"system:libc",
		LIB_PATH,
	}
	//odinfmt:enable
} else when ODIN_OS == .Darwin && ODIN_ARCH == .amd64 {
	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/macosx/x86_64/debug/libbop.a"
	} else {
		@(private)
		LIB_PATH :: "macos/amd64/libbop.a"
	}
	//odinfmt:disable
	foreign import lib {
		LIB_PATH,
	}
	//odinfmt:enable
} else when ODIN_OS == .Darwin && ODIN_ARCH == .arm64 {
	when #config(BOP_DEBUG, 0) == 1 {
		@(private)
		LIB_PATH :: "../../build/macosx/aarch64/debug/libbop.a"
	} else {
		@(private)
		LIB_PATH :: "macos/arm64/libbop.a"
	}
	//odinfmt:disable
	foreign import lib {
		LIB_PATH,
	}
	//odinfmt:enable
} else {
	#panic("libbop does not support this platform yet")
}

when !#exists(LIB_PATH) {
	#panic(
		"Could not find the compiled libbop libraries at \"" +
		LIB_PATH +
		"\", they can be compiled by running `xmake b` on the bop repo root",
	)
}

/*
*/

/*
///////////////////////////////////////////////////////////////////////////////////
//
// snmalloc
//
//
///////////////////

https://github.com/microsoft/snmalloc

snmalloc is a high-performance allocator. snmalloc can be used directly in a project as a
header-only C++ library, it can be LD_PRELOADed on Elf platforms (e.g. Linux, BSD), and
there is a crate to use it from Rust.

Its key design features are:

 - Memory that is freed by the same thread that allocated it does not require any synchronising
   operations.
 - Freeing memory in a different thread to initially allocated it, does not take any locks and
   instead uses a novel message passing scheme to return the memory to the original allocator,
   where it is recycled. This enables 1000s of remote deallocations to be performed with only a
   single atomic operation enabling great scaling with core count.
 - The allocator uses large ranges of pages to reduce the amount of meta-data required.
 - The fast paths are highly optimised with just two branches on the fast path for malloc
   (On Linux compiled with Clang).
 - The platform dependencies are abstracted away to enable porting to other platforms. snmalloc's
   design is particular well suited to the following two difficult scenarios that can be problematic
   for other allocators:

 - Allocations on one thread are freed by a different thread
 - Deallocations occur in large batches
 - Both of these can cause massive reductions in performance of other allocators, but do not for snmalloc.

The implementation of snmalloc has evolved significantly since the initial paper. The mechanism
for returning memory to remote threads has remained, but most of the meta-data layout has changed.
We recommend you read docs/security to find out about the current design, and if you want to dive
into the code docs/AddressSpace.md provides a good overview of the allocation and deallocation paths.
*/
@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	alloc :: proc(size: uintptr) -> rawptr ---
	zalloc :: proc(size: uintptr) -> rawptr ---
	dealloc :: proc(p: rawptr) ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// llco
// low-level coroutines
//
///////////////////

https://github.com/tidwall/llco

*/

@(default_calling_convention = "c")
foreign lib {
	@(link_name = "llco_current")
	co_current :: proc() -> ^Co ---
	@(link_name = "llco_start")
	co_start :: proc(desc: ^Co_Descriptor, final: bool) ---
	@(link_name = "llco_switch")
	co_switch :: proc(co: ^Co, final: bool) ---
	@(link_name = "llco_method")
	co_method :: proc(co: ^Co) -> cstring ---
}

/*
///////////////////////////////////////////////////////////////////////////////////
//
// raft (wraps C++ NuRaft by eBay)
//
///////////////////

https://github.com/eBay/NuRaft

Features

 - Core Raft algorithm
 - Log replication & compaction
 - Leader election
 - Snapshot
 - Dynamic membership & configuration change
 - Group commit & pipelined write
 - User-defined log store & state machine support
 - New features added in this project
 - Pre-vote protocol
 - Leadership expiration
 - Priority-based semi-deterministic leader election
 - Read-only member (learner)
 - Object-based logical snapshot
 - Custom/separate quorum size for commit & leader election
 - Asynchronous replication
 - SSL/TLS support
 - Parallel Log Appending
 - Custom Commit Policy
 - Streaming Mode

NuRaft makes extensive use of C++ paradigms and APIs including std::shared_ptr

Hot paths avoid
*/


@(link_prefix = "bop_")
@(default_calling_convention = "c")
foreign lib {
	raft_buffer_new :: proc(size: uintptr) -> ^Raft_Buffer ---
	raft_buffer_free :: proc(buf: ^Raft_Buffer) ---
	raft_buffer_container_size :: proc(buf: ^Raft_Buffer) -> uintptr ---
	raft_buffer_size :: proc(buf: ^Raft_Buffer) -> uintptr ---
	raft_buffer_pos :: proc(buf: ^Raft_Buffer) -> uintptr ---
	raft_buffer_set_pos :: proc(buf: ^Raft_Buffer, pos: uintptr) -> uintptr ---

	raft_async_u64_make :: proc(user_data: rawptr, when_ready: Raft_Async_U64_Done) -> ^Raft_Async_U64_Ptr ---
	raft_async_u64_delete :: proc(ptr: ^Raft_Async_U64_Ptr) ---
	raft_async_u64_get_user_data :: proc(ptr: ^Raft_Async_U64_Ptr) -> rawptr ---
	raft_async_u64_set_user_data :: proc(ptr: ^Raft_Async_U64_Ptr, user_data: rawptr) ---
	raft_async_u64_get_when_ready :: proc(ptr: ^Raft_Async_U64_Ptr) -> Raft_Async_U64_Done ---
	raft_async_u64_set_when_ready :: proc(ptr: ^Raft_Async_U64_Ptr, when_ready: Raft_Async_U64_Done) ---

	raft_async_buffer_make :: proc(user_data: rawptr, when_ready: Raft_Async_Buffer_Done) -> ^Raft_Async_Buffer_Ptr ---
	raft_async_buffer_delete :: proc(ptr: ^Raft_Async_Buffer_Ptr) ---
	raft_async_buffer_get_user_data :: proc(ptr: ^Raft_Async_Buffer_Ptr) -> rawptr ---
	raft_async_buffer_set_user_data :: proc(ptr: ^Raft_Async_Buffer_Ptr, user_data: rawptr) ---
	raft_async_buffer_get_when_ready :: proc(ptr: ^Raft_Async_Buffer_Ptr) -> Raft_Async_Buffer_Done ---
	raft_async_buffer_set_when_ready :: proc(ptr: ^Raft_Async_Buffer_Ptr, when_ready: Raft_Async_Buffer_Done) ---

	raft_snapshot_serialize :: proc(ptr: ^Raft_Snapshot) -> ^Raft_Buffer ---
	raft_snapshot_deserialize :: proc(ptr: ^Raft_Buffer) -> ^Raft_Snapshot ---

	raft_cluster_config_new :: proc() -> ^Raft_Cluster_Config ---
	raft_cluster_config_free :: proc(config: ^Raft_Cluster_Config) ---
	raft_cluster_config_ptr_create :: proc(config: ^Raft_Cluster_Config) -> ^Raft_Cluster_Config_Ptr ---
	raft_cluster_config_ptr_delete :: proc(cfg_ptr: ^Raft_Cluster_Config_Ptr) ---
	raft_cluster_config_serialize :: proc(cfg: ^Raft_Cluster_Config) -> ^Raft_Buffer ---
	raft_cluster_config_deserialize :: proc(cfg: ^Raft_Buffer) -> ^Raft_Cluster_Config ---
	raft_cluster_config_log_idx :: proc(cfg: ^Raft_Cluster_Config) -> u64 ---
	raft_cluster_config_prev_log_idx :: proc(cfg: ^Raft_Cluster_Config) -> u64 ---
	raft_cluster_config_is_async_replication :: proc(cfg: ^Raft_Cluster_Config) -> bool ---
	raft_cluster_config_user_ctx :: proc(cfg: ^Raft_Cluster_Config, out_data: ^byte, out_data_size: uintptr) -> uintptr ---
	raft_cluster_config_user_ctx_size :: proc(cfg: ^Raft_Cluster_Config) -> uintptr ---
	raft_cluster_config_servers_size :: proc(cfg: ^Raft_Cluster_Config) -> uintptr ---
	raft_cluster_config_server :: proc(cfg: ^Raft_Cluster_Config, idx: i32) -> ^Raft_Srv_Config ---

	raft_srv_config_vec_create :: proc() -> ^Raft_Srv_Config_Vec ---
	raft_srv_config_vec_delete :: proc(vec: ^Raft_Srv_Config_Vec) ---
	raft_srv_config_vec_size :: proc(vec: ^Raft_Srv_Config_Vec) -> uintptr ---
	raft_srv_config_vec_get :: proc(vec: ^Raft_Srv_Config_Vec, idx: uintptr) -> ^Raft_Srv_Config ---

	raft_srv_config_ptr_make :: proc(config: ^Raft_Srv_Config) -> ^Raft_Srv_Config_Ptr ---
	raft_srv_config_ptr_delete :: proc(config_ptr: ^Raft_Srv_Config_Ptr) ---
	raft_srv_config_make :: proc(
		id: i32,
		dc_id: i32,
		endpoint: [^]byte,
		endpoint_size: uintptr,
		aux: [^]byte,
		aux_size: uintptr,
		learner: bool,
		priority: i32,
	) -> ^Raft_Srv_Config ---
	raft_srv_config_delete :: proc(config: ^Raft_Srv_Config) ---
	raft_srv_config_id :: proc(config: ^Raft_Srv_Config) -> i32 ---
	raft_srv_config_dc_id :: proc(config: ^Raft_Srv_Config) -> i32 ---
	raft_srv_config_endpoint :: proc(config: ^Raft_Srv_Config) -> cstring ---
	raft_srv_config_endpoint_size :: proc(config: ^Raft_Srv_Config) -> uintptr ---
	raft_srv_config_aux :: proc(config: ^Raft_Srv_Config) -> cstring ---
	raft_srv_config_aux_size :: proc(config: ^Raft_Srv_Config) -> uintptr ---

	// `true` if this node is learner.
	// Learner will not initiate or participate in leader election.
	raft_srv_config_is_learner :: proc(config: ^Raft_Srv_Config) -> bool ---

	// `true` if this node is a new joiner, but not yet fully synced.
	// New joiner will not initiate or participate in leader election.
	raft_srv_config_is_new_joiner :: proc(config: ^Raft_Srv_Config) -> bool ---

	// Priority of this node.
	// 0 will never be a leader.
	raft_srv_config_priority :: proc(config: ^Raft_Srv_Config) -> i32 ---

	raft_srv_state_serialize :: proc(buf: ^Raft_Srv_State) -> ^Raft_Buffer ---
	raft_srv_state_deserialize :: proc(buf: ^Raft_Buffer) -> ^Raft_Srv_State ---
	raft_srv_state_delete :: proc(buf: ^Raft_Srv_State) ---

	// Term
	raft_srv_state_term :: proc(buf: ^Raft_Srv_State) -> u64 ---

	// Server ID that this server voted for.
	// `-1` if not voted.
	raft_srv_state_voted_for :: proc(buf: ^Raft_Srv_State) -> i32 ---

	// `true` if election timer is allowed.
	raft_srv_state_is_election_timer_allowed :: proc(buf: ^Raft_Srv_State) -> bool ---

	// true if this server has joined the cluster but has not yet
	// fully caught up with the latest log. While in the catch-up status,
	// this server will not receive normal append_entries requests.
	raft_srv_state_is_catching_up :: proc(buf: ^Raft_Srv_State) -> bool ---

	// `true` if this server is receiving a snapshot.
	// Same as `catching_up_`, it must be a durable flag so as not to be
	// reset after restart. While this flag is set, this server will neither
	// receive normal append_entries requests nor initiate election.
	raft_srv_state_is_receiving_snapshot :: proc(buf: ^Raft_Srv_State) -> bool ---

	raft_logger_make :: proc(user_data: rawptr, write_func: Raft_Logger_Write_Func) -> ^Raft_Logger_Ptr ---
	raft_logger_delete :: proc(logger: ^Raft_Logger_Ptr) ---

	raft_delayed_task_make :: proc(user_data: rawptr, type: i32, callback: proc "c" (user_data: rawptr)) -> ^Raft_Delayed_Task ---
	raft_delayed_task_delete :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_cancel :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_reset :: proc(task: ^Raft_Delayed_Task) ---
	raft_delayed_task_type :: proc(task: ^Raft_Delayed_Task) -> i32 ---
	raft_delayed_task_user_data :: proc(task: ^Raft_Delayed_Task) -> rawptr ---
	raft_delayed_task_set :: proc(task: ^Raft_Delayed_Task, user_data: rawptr, callback: proc "c" (user_data: rawptr)) -> rawptr ---

	raft_asio_service_make :: proc(options: ^Raft_Asio_Options, logger: ^Raft_Logger_Ptr) -> ^Raft_Asio_Service_Ptr ---
	raft_asio_service_delete :: proc(asio_service: ^Raft_Asio_Service_Ptr) ---

	raft_asio_service_stop :: proc(asio_service: ^Raft_Asio_Service_Ptr) ---
	raft_asio_service_get_active_workers :: proc(asio_service: ^Raft_Asio_Service_Ptr) -> u32 ---

	raft_asio_service_schedule :: proc(asio_service: ^Raft_Asio_Service_Ptr, task: ^Raft_Delayed_Task, millis: i32) ---

	raft_asio_rpc_listener_make :: proc(asio_service: ^Raft_Asio_Service_Ptr, listening_port: u16, logger: ^Raft_Logger_Ptr) -> ^Raft_Asio_RPC_Listener_Ptr ---
	raft_asio_rpc_listener_delete :: proc(rpc_listener: ^Raft_Asio_RPC_Listener_Ptr) ---

	raft_asio_rpc_client_make :: proc(
		asio_service: ^Raft_Asio_Service_Ptr,
		endpoint: [^]byte,
		endpoint_size: uintptr
	) -> ^Raft_Asio_RPC_Client_Ptr ---

	raft_asio_rpc_client_delete :: proc(rpc_listener: ^Raft_Asio_RPC_Client_Ptr) ---

	raft_fsm_make :: proc(
		user_data: rawptr,
		current_conf: ^Raft_Cluster_Config,
		rollback_conf: ^Raft_Cluster_Config,
		commit: Raft_FSM_Commit_Func,
		commit_config: Raft_FSM_Cluster_Config_Func,
		pre_commit: Raft_FSM_Commit_Func,
		rollback: Raft_FSM_Rollback_Func,
		rollback_config: Raft_FSM_Cluster_Config_Func,
		get_next_batch_size_hint_in_bytes: Raft_FSM_Get_Next_Batch_Size_Hint_In_Bytes_Func,
		save_snapshot: Raft_FSM_Save_Snapshot_Func,
		apply_snapshot: Raft_FSM_Apply_Snapshot_Func,
		read_snapshot: Raft_FSM_Read_Snapshot_Func,
		free_snapshot_user_ctx: Raft_FSM_Free_User_Snapshot_Ctx_Func,
		last_snapshot: Raft_FSM_Last_Snapshot_Func,
		last_commit_index: Raft_FSM_Last_Commit_Index_Func,
		create_snapshot: Raft_FSM_Create_Snapshot_Func,
		chk_create_snapshot: Raft_FSM_Chk_Create_Snapshot_Func,
		allow_leadership_transfer: Raft_FSM_Allow_Leadership_Transfer_Func,
		adjust_commit_index: Raft_FSM_Adjust_Commit_Index_Func,
	) -> ^Raft_FSM_Ptr ---

	raft_fsm_delete :: proc(fsm: ^Raft_FSM_Ptr) ---

	raft_log_entry_make :: proc(
		term: u64,
		data: ^Raft_Buffer,
		timestamp: u64,
		has_crc32: bool,
		crc32: u32
	) -> ^Raft_Log_Entry ---

	raft_log_entry_delete :: proc(entry: ^Raft_Log_Entry) ---

	raft_log_entry_vec_push :: proc(
		vec: ^Raft_Log_Entry_Vec,
		term: u64,
		data: ^Raft_Buffer,
		timestamp: u64,
		has_crc32: bool,
		crc32: u32
	) ---

	raft_log_store_make :: proc(
		user_data: rawptr,
		next_slot: Raft_Log_Store_Next_Slot_Func,
		start_index: Raft_Log_Store_Start_Index_Func,
		last_entry: Raft_Log_Store_Last_Entry_Func,
		append: Raft_Log_Store_Append_Func,
		write_at: Raft_Log_Store_Write_At_Func,
		end_of_append_batch: Raft_Log_Store_End_Of_Append_Batch_Func,
		log_entries: Raft_Log_Store_Log_Entries_Func,
		entry_at: Raft_Log_Store_Entry_At_Func,
		term_at: Raft_Log_Store_Term_At_Func,
		pack: Raft_Log_Store_Pack_Func,
		apply_pack: Raft_Log_Store_Apply_Pack_Func,
		compact: Raft_Log_Store_Compact_Func,
		compact_async: Raft_Log_Store_Compact_Async_Func,
		flush: Raft_Log_Store_Flush_Func,
		last_durable_index: Raft_Log_Store_Last_Durable_Index_Func,
	) -> ^Raft_Log_Store_Ptr ---

	raft_log_store_delete :: proc(log_store: ^Raft_Log_Store_Ptr) ---

	raft_state_mgr_make :: proc(
		user_data: rawptr,
		load_config: Raft_State_Mgr_Load_Config_Func,
		save_config: Raft_State_Mgr_Save_Config_Func,
		read_state: Raft_State_Mgr_Read_State_Func,
		save_state: Raft_State_Mgr_Save_State_Func,
		load_log_store: Raft_State_Mgr_Load_Log_Store_Func,
		server_id: Raft_State_Mgr_Server_ID_Func,
		system_exit: Raft_State_Mgr_System_Exit_Func,
	) -> ^Raft_State_Mgr_Ptr ---

	raft_state_mgr_delete :: proc(state_mgr: ^Raft_State_Mgr_Ptr) ---

	raft_server_peer_info_vec_make :: proc() -> ^Raft_Server_Peer_Info_Vec ---

	raft_server_peer_info_vec_delete :: proc(vec: ^Raft_Server_Peer_Info_Vec) ---

	raft_server_peer_info_vec_size :: proc(vec: ^Raft_Server_Peer_Info_Vec) -> uintptr ---
	raft_server_peer_info_vec_get :: proc(vec: ^Raft_Server_Peer_Info_Vec, index: uintptr) -> ^Raft_Server_Peer_Info ---

	raft_params_make :: proc() -> ^Raft_Params ---
	raft_params_delete :: proc(params: ^Raft_Params) ---

	raft_server_launch :: proc(
		user_data: rawptr,
		fsm: ^Raft_FSM_Ptr,
		state_mgr: ^Raft_State_Mgr_Ptr,
		logger: ^Raft_Logger_Ptr,
		port_number: i32,
		asio_service: ^Raft_Asio_Service_Ptr,
		params_given: ^Raft_Params,
		skip_initial_election_timeout: bool,
		start_server_in_constructor: bool,
		test_mode_flag: bool,
		cb_func: rawptr
	) -> ^Raft_Server_Ptr ---

	raft_server_stop :: proc(
		rs_ptr: ^Raft_Server_Ptr,
		time_limit_sec: uintptr
	) -> bool ---

	raft_server_get :: proc(
		rs_ptr: ^Raft_Server_Ptr
	) -> ^Raft_Server ---

	/*
	Check if this server is ready to serve operation.

	@return `true` if it is ready.
	*/
	raft_server_is_initialized :: proc(
		rs: ^Raft_Server,
	) -> bool ---


	/*
	*/
	raft_server_is_catching_up :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	*/
	raft_server_is_receiving_snapshot :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Add a new server to the current cluster. Only leader will accept this operation.
	Note that this is an asynchronous task so that needs more network communications.
	Returning this function does not guarantee adding the server.

	@param srv Configuration of server to add.
	@return `get_accepted()` will be true on success.
	*/
	raft_server_add_srv :: proc(
		rs: ^Raft_Server,
		srv: ^Raft_Srv_Config_Ptr,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Remove a server from the current cluster. Only leader will accept this operation.
	The same as `add_srv`, this is also an asynchronous task.

	@param srv_id ID of server to remove.
	@return `get_accepted()` will be true on success.
	*/
	raft_server_remove_srv :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Flip learner flag of given server. Learner will be excluded from the quorum. Only
	leader will accept this operation. This is also an asynchronous task.

	@param srv_id ID of the server to set as a learner.
	@param to If `true`, set the server as a learner, otherwise, clear learner flag.
	@return `ret->get_result_code()` will be OK on success.
	*/
	raft_server_flip_learner_flag :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		to: bool,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Append and replicate the given logs. Only leader will accept this operation.

	@param entries Set of logs to replicate.
	@return
	    In blocking mode, it will be blocked during replication, and
		return `cmd_result` instance which contains the commit results from
		the state machine.

		In async mode, this function will return immediately, and the commit
		results will be set to returned `cmd_result` instance later.
	*/
	raft_server_append_entries :: proc(
		rs: ^Raft_Server,
		entries: ^Raft_Append_Entries_Ptr,
		handler: Raft_Async_Buffer_Ptr,
	) -> bool ---

	/*
	Update the priority of given server.

	@param rs local bop_raft_server instance
	@param srv_id ID of server to update priority.
	@param new_priority Priority value, greater than or equal to 0.
			If priority is set to 0, this server will never be a leader.
	@param broadcast_when_leader_exists If we're not a leader and a
			leader exists, broadcast priority change to other peers.
			If false, set_priority does nothing. Please note that
			setting this option to true may possibly cause cluster config
			to diverge.

	@return SET If we're a leader and we have committed priority change.

	@return BROADCAST If either there's no:
			- live leader now, or we're a leader, and we want to set our priority to 0
			- we're not a leader and broadcast_when_leader_exists = true.
			  We have sent messages to other peers about priority change but haven't
			  committed this change.
	@return IGNORED If we're not a leader and broadcast_when_leader_exists = false.
			We ignored the request.
	*/
	raft_server_set_priority :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		new_priority: i32,
		broadcast_when_leader_exists: bool,
	) -> Raft_Server_Priority_Set_Result ---

	/*
	Broadcast the priority change of given server to all peers. This function should be used
	only when there is no live leader and leader election is blocked by priorities of live
	followers. In that case, we are not able to change priority by using normal `set_priority`
	operation.

	@param rs local bop_raft_server instance
	@param srv_id ID of server to update priority
	@param new_priority New priority.
	*/
	raft_server_broadcast_priority_change :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		new_priority: i32,
	) ---

	/*
	Yield current leadership and becomes a follower. Only a leader will accept this operation.

	If given `immediate_yield` flag is `true`, it will become a follower immediately.
	The subsequent leader election will be totally random so that there is always a
	chance that this server becomes the next leader again.

	Otherwise, this server will pause write operations first, wait until the successor
	(except for this server) finishes the catch-up of the latest log, and then resign.
	In such a case, the next leader will be much more predictable.

	Users can designate the successor. If not given, this API will automatically choose
	the highest priority server as a successor.

	@param rs local bop_raft_server instance
	@param immediate_yield If `true`, yield immediately.
	@param successor_id The server ID of the successor.
						If `-1`, the successor will be chosen automatically.
	*/
	raft_server_yield_leadership :: proc(
		rs: ^Raft_Server,
		immediate_yield: bool,
		successor_id: i32,
	) ---

	/*
	Send a request to the current leader to yield its leadership, and become the next leader.

	@return `true` on success. But it does not guarantee to become
	        the next leader due to various failures.
	*/
	raft_server_request_leadership :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Start the election timer on this server, if this server is a follower. It will
	allow the election timer permanently, if it was disabled by state manager.
	*/
	raft_server_restart_election_timer :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Set custom context to Raft cluster config. It will create a new configuration log and
	replicate it.

	@param data custom context data
	@param size number of bytes in data
	*/
	raft_server_set_user_ctx :: proc(
		rs: ^Raft_Server,
		data: [^]byte,
		size: uintptr,
	) ---

	/*
	Get custom context from the current cluster config.

	@return custom context. The caller is transferred ownership of the Raft_Buffer.
			Any non-null value must be freed by calling `raft_buffer_delete` or
			transfer ownership.
	*/
	raft_server_get_user_ctx :: proc(
		rs: ^Raft_Server,
	) -> ^Raft_Buffer ---

	/*
	Get timeout for snapshot_sync_ctx

	@return snapshot_sync_ctx_timeout
	*/
	raft_server_get_snapshot_sync_ctx_timeout :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get ID of this server.

	@return Server ID
	*/
	raft_server_get_id :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get the current term of this server.

	@return Term
	*/
	raft_server_get_term :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the term of given log index number.

	@param log_idx Log index number
	@return Term of given log
	*/
	raft_server_get_log_term :: proc(
		rs: ^Raft_Server,
		log_idx: u64,
	) -> u64 ---

	/*
	Get the term of the last log.

	@return Term of the last log
	*/
	raft_server_get_last_log_term :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the last log index number.

	@return Last log index number.
	*/
	raft_server_get_last_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the last committed log index number of state machine.

	@return Last committed log index number of state machine.
	*/
	raft_server_get_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the target log index number we are required to commit.

	@return Target committed log index number.
	*/
	raft_server_get_target_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the leader's last committed log index number.

	@return The leader's last committed log index number.
	*/
	raft_server_get_leader_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the log index of the first config when this server became a leader.
	This API can be used for checking if the state machine is fully caught up
	with the latest log after a leader election, so that the new leader can
	guarantee strong consistency.

	It will return 0 if this server is not a leader.

	@return The log index of the first config when this server became a leader.
	*/
	raft_server_get_log_idx_at_becoming_leader :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Calculate the log index to be committed from current peers' matched indexes.

	@return Expected committed log index.
	*/
	raft_server_get_expected_committed_log_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	/*
	Get the current Raft cluster config.

	@param rs raft server instance
	@param cluster_config Wrapper for holding ^Raft_Cluster_Config_Ptr
	*/
	raft_server_get_config :: proc(
		rs: ^Raft_Server,
		cluster_config: ^Raft_Cluster_Config_Ptr,
	) ---

	/*
	Get data center ID of the given server.

	@param srv_id Server ID.
	@return -1 if given server ID does not exist.
	         0 if data center ID was not assigned.
	*/
	raft_server_get_dc_id :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Get auxiliary context stored in the server config.

	@param srv_id Server ID.
	@return auxiliary context. The caller is transferred ownership of the Raft_Buffer.
			Any non-null value must be freed by calling `raft_buffer_delete` or
			transfer ownership.
	*/
	raft_server_get_aux :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
	) -> ^Raft_Buffer ---

	/*
	Get the ID of current leader.

	@return Leader ID
	        -1 if there is no live leader.
	*/
	raft_server_get_leader :: proc(
		rs: ^Raft_Server,
	) -> i32 ---

	/*
	Check if this server is leader.

	@return `true` if it is leader.
	*/
	raft_server_is_leader :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Check if there is live leader in the current cluster.

	@return `true` if live leader exists.
	*/
	raft_server_is_leader_alive :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Get the configuration of given server.

	@param srv_id Server ID
	@return Server configuration
	*/
	raft_server_get_srv_config :: proc(
		rs: ^Raft_Server,
		svr_config: ^Raft_Srv_Config_Ptr,
		srv_id: i32,
	) ---

	/*
	Get the configuration of all servers.

	@param[out] configs_out Set of server configurations.
	*/
	raft_server_get_srv_config_all :: proc(
		rs: ^Raft_Server,
		configs_out: ^Raft_Srv_Config_Vec,
	) ---

	/*
	Update the server configuration, only leader will accept this operation.
	This function will update the current cluster config and replicate it to all peers.

	We don't allow changing multiple server configurations at once, due to safety reason.

	Change on endpoint will not be accepted (should be removed and then re-added).
	If the server is in new joiner state, it will be rejected.
	If the server ID does not exist, it will also be rejected.

	@param new_config Server configuration to update.
	@return `true` on success, `false` if rejected.
	*/
	raft_server_update_srv_config :: proc(
		rs: ^Raft_Server,
		new_config: ^Raft_Srv_Config_Ptr,
	) ---

	/*
	Get the peer info of the given ID. Only leader will return peer info.

	@param srv_id Server ID
	@return Peer info
	*/
	raft_server_get_peer_info :: proc(
		rs: ^Raft_Server,
		srv_id: i32,
		peer: ^Raft_Server_Peer_Info,
	) -> bool ---

	/*
	Get the info of all peers. Only leader will return peer info.

	@param[out] peers_out vector of peers
	*/
	raft_server_get_peer_info_all :: proc(
		rs: ^Raft_Server,
		peers_out: ^Raft_Server_Peer_Info_Vec,
	) ---

	/*
	Shut down server instance.
	*/
	raft_server_shutdown :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Start internal background threads, initialize election
	*/
	raft_server_start_server :: proc(
		rs: ^Raft_Server,
		skip_initial_election_timeout: bool,
	) ---

	/*
	Stop background commit thread.
	*/
	raft_server_stop_server :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Send reconnect request to leader. Leader will re-establish the connection to this server
	in a few seconds. Only follower will accept this operation.
	*/
	raft_server_send_reconnect_request :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Update Raft parameters.

	@param new_params Parameters to set
	*/
	raft_server_update_params :: proc(
		rs: ^Raft_Server,
		new_params: ^Raft_Params,
	) ---

	/*
	Get the current Raft parameters. Returned instance is the clone of the original one,
	so that user can modify its contents.

	@return Clone of Raft parameters.
	*/
	raft_server_get_current_params :: proc(
		rs: ^Raft_Server,
		params: ^Raft_Params,
	) ---

	/*
	Get the counter number of given stat name.

	@param name Stat name to retrieve.
	@return Counter value.
	*/
	raft_server_get_stat_counter :: proc(
		rs: ^Raft_Server,
		counter: ^Raft_Counter,
	) -> u64 ---

	/*
	Get the gauge number of given stat name.

	@param name Stat name to retrieve.
	@return Gauge value.
	*/
	raft_server_get_stat_gauge :: proc(
		rs: ^Raft_Server,
		gauge: ^Raft_Gauge,
	) -> i64 ---

	/*
	Get the histogram of given stat name.

	@param name Stat name to retrieve.
	@param[out] histogram_out Histogram as a map. Key is the upper bound of a bucket, and
	            value is the counter of that bucket.
	@return `true` on success.
	        `false` if stat does not exist, or is not histogram type.
	*/
	raft_server_get_stat_histogram :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) -> bool ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_counter :: proc(
		rs: ^Raft_Server,
		counter: ^Raft_Counter,
	) ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_gauge :: proc(
		rs: ^Raft_Server,
		gauge: ^Raft_Gauge,
	) ---

	/*
	Reset given stat to zero.

	@param name Stat name to reset.
	*/
	raft_server_reset_histogram :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) ---

	/*
	Reset all existing stats to zero.
	*/
	raft_server_reset_all_stats :: proc(
		rs: ^Raft_Server,
		histogram: ^Raft_Histogram,
	) ---

	/*
	Set a custom callback function for increasing term.
	*/
	raft_server_set_inc_term_func :: proc(
		rs: ^Raft_Server,
		user_data: rawptr,
		handler: Raft_Inc_Term_Handler,
	) ---

	/*
	Pause the background execution of the state machine. If an operation execution is
	currently happening, the state machine may not be paused immediately.

	@param timeout_ms If non-zero, this function will be blocked until
	                  either it completely pauses the state machine execution
	                  or reaches the given time limit in milliseconds.
	                  Otherwise, this function will return immediately, and there
	                  is a possibility that the state machine execution
	                  is still happening.
	*/
	raft_server_pause_state_machine_execution :: proc(
		rs: ^Raft_Server,
		timeout_ms: uintptr,
	) ---

	/*
	Resume the background execution of state machine.
	*/
	raft_server_resume_state_machine_execution :: proc(
		rs: ^Raft_Server,
	) ---

	/*
	Check if the state machine execution is paused.

	@return `true` if paused.
	*/
	raft_server_is_state_machine_execution_paused :: proc(
		rs: ^Raft_Server,
	) -> bool ---

	/*
	Block the current thread and wake it up when the state machine execution is paused.

	@param timeout_ms If non-zero, wake up after the given amount of time
	                  even though the state machine is not paused yet.
  	@return `true` if the state machine is paused.
	*/
	raft_server_wait_for_state_machine_pause :: proc(
		rs: ^Raft_Server,
		timeout_ms: uintptr,
	) -> bool ---

	/*
	(Experimental)
	This API is used when `raft_params::parallel_log_appending_` is set.

	Everytime an asynchronous log appending job is done, users should call this API to notify.
	Raft server to handle the log. Note that calling this API once for multiple logs is acceptable
	and recommended.

	@param ok `true` if appending succeeded.
	*/
	raft_server_notify_log_append_completion :: proc(
		rs: ^Raft_Server,
		ok: bool,
	) ---

	/*
	Manually create a snapshot based on the latest committed log index of the state machine.

	Note that snapshot creation will fail immediately if the previous snapshot task is still running.

	@param serialize_commit
	       If `true`, the background commit will be blocked until `create_snapshot`
	       returns. However, it will not block the commit for the entire duration
	       of the snapshot creation process, as long as your state machine creates
	       the snapshot asynchronously. The purpose of this flag is to ensure that
	       the log index used for the snapshot creation is the most recent one.

    @return Log index number of the created snapshot or`0` if failed.
	*/
	raft_server_create_snapshot :: proc(
		rs: ^Raft_Server,
		serialize_commit: bool,
	) -> u64 ---

	/*
	Manually and asynchronously create a snapshot on the next earliest available commited
	log index.

	Unlike `create_snapshot`, if the previous snapshot task is running, it will wait
	until the previous task is done. Once the snapshot creation is finished, it will be
	notified via the returned `cmd_result` with the log index number of the snapshot.

	@param `handler` instance.
		   `nullptr` if there is already a scheduled snapshot creation.
	*/
	raft_server_schedule_snapshot_creation :: proc(
		rs: ^Raft_Server,
		handler: ^Raft_Async_U64_Ptr,
	) ---

	/*
	Get the log index number of the last snapshot.

	@return Log index number of the last snapshot. `0` if snapshot does not exist.
	*/
	raft_server_get_last_snapshot_idx :: proc(
		rs: ^Raft_Server,
	) -> u64 ---

	raft_mdbx_state_mgr_open :: proc(
		my_srv_config: ^Raft_Srv_Config_Ptr,
		dir: [^]byte,
		dir_size: uintptr,
		logger: ^Raft_Logger_Ptr,
		size_lower: uintptr,
		size_now: uintptr,
		size_upper: uintptr,
		growth_step: uintptr,
		shirnk_threshold: uintptr,
		page_size: uintptr,
		flags: u32,
		mode: u16,
		log_store: ^Raft_Log_Store_Ptr,
	) -> ^Raft_State_Mgr_Ptr ---

	raft_mdbx_log_store_open :: proc(
		dir: [^]byte,
		dir_size: uintptr,
		logger: ^Raft_Logger_Ptr,
		size_lower: uintptr,
		size_now: uintptr,
		size_upper: uintptr,
		growth_step: uintptr,
		shirnk_threshold: uintptr,
		page_size: uintptr,
		flags: u32,
		mode: u16,
		compact_batch_size: uintptr,
	) -> ^Raft_Log_Store_Ptr ---
}

main :: proc() {
	run_alloc()

	bench_co()
}

run_alloc :: proc() {
	fmt.println("libbop")
	p := zalloc(16)
	fmt.println("pointer", cast(uint)uintptr(p))
	dealloc(p)
}


@(test)
test_alloc :: proc(t: ^testing.T) {
	p := alloc(16)
	dealloc(p)
}
