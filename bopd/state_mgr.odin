package bopd

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:os/os2"
import "core:path/filepath"
import "core:strings"
import "core:sync"
import "core:time"
import "core:thread"

import "../odin/mdbx"
import "../odin/raft"

STATE_MGR_CLUSTER_CONFIG_KEY : u32 : 1
STATE_MGR_SRV_STATE_KEY 	 : u32 : 2

State_Mgr :: struct {
	mu:		   sync.Mutex,
	allocator: runtime.Allocator,
	path: 	   cstring,
	state_mgr: ^raft.State_Mgr_Ptr,
	log_store: ^Log_Store,
	env:	   ^mdbx.Env,
	dbi:	   mdbx.DBI,

	cluster_config: ^raft.Buffer,
	srv_state: 		^raft.Buffer,
	srv_id: 		i32,
}

/*
Load the last saved cluster config.
This function will be invoked on initialization of
Raft server.

Even at the very first initialization, it should
return proper initial cluster config, not `nullptr`.
The initial cluster config must include the server itself.

@return Cluster config.
*/
state_mgr_load_config :: proc "c" (user_data: rawptr) -> ^raft.Cluster_Config {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)
	if s.cluster_config == nil {
		return nil
	}
	return raft.cluster_config_deserialize(s.cluster_config)
}

/*
Save given cluster config.

@param config Cluster config to save.
*/
state_mgr_save_config :: proc "c" (user_data: rawptr, config: ^raft.Cluster_Config) {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)

	new_config := raft.cluster_config_serialize(config)
	if s.cluster_config != nil {
		raft.buffer_free(s.cluster_config)
	}
	s.cluster_config = new_config

	if s.env == nil {
		return
	}

	txn: ^mdbx.Txn = nil
	err := mdbx.txn_begin_ex(s.env, nil, .Read_Write, &txn, nil)
	if err != nil {
		log.errorf("mdbx txn failed to start: %s", mdbx.liberr2str(err))
		return
	}

	id := u32(STATE_MGR_CLUSTER_CONFIG_KEY)
	key   := mdbx.val_u32(&id)
	value := mdbx.Val{
		base = raft.buffer_data(new_config),
		len = uint(raft.buffer_size(new_config)),
	}

	err = mdbx.put(txn, s.dbi, &key, &value, .Upsert)
	if err != nil {
		mdbx.txn_abort(txn)
		log.errorf("mdbx put cluster_config failed: %s", mdbx.liberr2str(err))
		return
	}

	err = mdbx.txn_commit(txn)
	if err != nil {
		mdbx.txn_abort(txn)
		log.errorf("mdbx put cluster_config failed txn_commit: %s", mdbx.liberr2str(err))
		return
	}
}

/*
Load the last saved server state.
This function will be invoked on initialization of
Raft server

At the very first initialization, it should return
`nullptr`.

@param Server state.
*/
state_mgr_read_state :: proc "c" (user_data: rawptr) -> ^raft.Srv_State {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)
	if s.srv_state == nil {
		return nil
	}
	return raft.srv_state_deserialize(s.srv_state)
}

/*
Save given server state.

@param state Server state to save.
*/
state_mgr_save_state :: proc "c" (user_data: rawptr, state: ^raft.Srv_State) {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)

	new_config := raft.srv_state_serialize(state)
	if s.srv_state != nil {
		raft.buffer_free(s.srv_state)
	}
	s.srv_state = new_config

	if s.env == nil {
		return
	}

	txn: ^mdbx.Txn = nil
	err := mdbx.txn_begin_ex(s.env, nil, .Read_Write, &txn, nil)
	if err != nil {
		log.errorf("mdbx txn failed to start: %s", mdbx.liberr2str(err))
		return
	}

	id := u32(STATE_MGR_SRV_STATE_KEY)
	key   := mdbx.val_u32(&id)
	value := mdbx.Val{
		base = raft.buffer_data(new_config),
		len = uint(raft.buffer_size(new_config)),
	}

	err = mdbx.put(txn, s.dbi, &key, &value, .Upsert)
	if err != nil {
		mdbx.txn_abort(txn)
		log.errorf("mdbx put srv_state failed: %s", mdbx.liberr2str(err))
		return
	}

	err = mdbx.txn_commit(txn)
	if err != nil {
		mdbx.txn_abort(txn)
		log.errorf("mdbx put srv_state failed txn_commit: %s", mdbx.liberr2str(err))
		return
	}
}

/*
Get instance of user-defined Raft log store.

@param Raft log store instance.
*/
state_mgr_load_log_store :: proc "c" (user_data: rawptr) -> ^raft.Log_Store_Ptr {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)
	if s.log_store == nil {
		return nil
	}
	return s.log_store.log_store
}

/*
Get ID of this Raft server.

@return Server ID.
*/
state_mgr_server_id :: proc "c" (user_data: rawptr) -> i32 {
	s := cast(^State_Mgr)user_data
	context = tls_context()
	sync.mutex_guard(&s.mu)
	return s.srv_id
}

/*
System exit handler. This function will be invoked on
abnormal termination of Raft server.

@param exit_code Error code.
*/
state_mgr_system_exit :: proc "c" (user_data: rawptr, exit_code: i32) {
	s := cast(^State_Mgr)user_data
}

state_mgr_destroy :: proc(s: ^State_Mgr) {
	if s == nil {
		return
	}

	if s.state_mgr != nil {
		raft.state_mgr_delete(s.state_mgr)
		s.state_mgr = nil
	}

	if s.cluster_config != nil {
		raft.buffer_free(s.cluster_config)
		s.cluster_config = nil
	}

	if s.srv_state != nil {
		raft.buffer_free(s.srv_state)
		s.srv_state = nil
	}

	if s.env != nil {
		mdbx.env_close_ex(s.env, true)
		s.env = nil
	}

	if s.path != nil {
		delete(s.path, s.allocator)
		s.path = nil
	}

	free(s, s.allocator)
}

State_Mgr_Make_Error :: union #shared_nil {
	runtime.Allocator_Error,
	os2.Error,
	mdbx.Error,
}

/*

*/
state_mgr_make :: proc(
	path: string,
	allocator := context.allocator,
) -> (
	s: ^State_Mgr,
	err: State_Mgr_Make_Error,
) {
	txn : ^mdbx.Txn = nil
	dir : string = ""
	defer {
		delete(dir, allocator)
		if err != nil {
			if txn != nil {
				mdbx.txn_abort(txn)
				txn = nil
			}
			state_mgr_destroy(s)
		}
	}
	
	s = new(State_Mgr, allocator) or_return
	s.allocator = allocator

	if len(path) > 0 {
		dir = filepath.dir(path, allocator)
		if e := os2.make_directory_all(dir, 0755); e != nil {
			return nil, e
		}
		s.path = strings.clone_to_cstring(path, allocator) or_return
	}

	// make the raft state_mgr
	s.state_mgr = raft.state_mgr_make(
		rawptr(s),
		state_mgr_load_config,
		state_mgr_save_config,
		state_mgr_read_state,
		state_mgr_save_state,
		state_mgr_load_log_store,
		state_mgr_server_id,
		state_mgr_system_exit,
	)

	ensure(s.state_mgr != nil, "state_mgr is nil")

	// non-durable state_mgr for testing?
	if len(path) == 0 {
		return
	}

	mdbx.env_create(&s.env) or_return

	// Setup geometry. Try to utilize only a single file-system page for entire db.
	mdbx.env_set_geometry(
		s.env,
		runtime.Kilobyte*4,
		runtime.Kilobyte*4,
		runtime.Kilobyte*64,
		runtime.Kilobyte*4,
		runtime.Kilobyte*4,
		512,
	) or_return

	// Max DBs can be small.
	mdbx.env_set_option(s.env, mdbx.Option.Max_DB, 2) or_return
	mdbx.env_set_option(s.env, mdbx.Option.Max_Readers, 64) or_return

	mdbx.env_open(
		s.env,
		s.path,
		.No_Sub_Dir | .No_Mem_Init | .Sync_Durable | .Lifo_Reclaim | .Write_Map,
		0755,
	) or_return

	mdbx.txn_begin_ex(s.env, nil, .Read_Write, &txn, nil) or_return
	mdbx.dbi_open(txn, "state", .Integer_Key | .Create, &s.dbi)

	id := STATE_MGR_CLUSTER_CONFIG_KEY
	key, value: mdbx.Val
	key = mdbx.val_u32(&id)
	err = mdbx.get(txn, s.dbi, &key, &value)
	if err != nil {
		if err != State_Mgr_Make_Error(mdbx.Error.Not_Found) do return
		err = nil
	} else {
		s.cluster_config = raft.buffer_new_from_io_vec(value) or_return
		cluster_config := raft.cluster_config_deserialize(s.cluster_config)
		if cluster_config == nil {
			raft.buffer_free(s.cluster_config)
			s.cluster_config = nil
		} else {
			raft.cluster_config_free(cluster_config)
		}
	}

	id = STATE_MGR_SRV_STATE_KEY
	key = mdbx.val_u32(&id)
	value = mdbx.Val{}
	err = mdbx.get(txn, s.dbi, &key, &value)
	if err != nil {
		if err != State_Mgr_Make_Error(mdbx.Error.Not_Found) do return
		err = nil
	} else {
		s.srv_state = raft.buffer_new_from_io_vec(value) or_return
		srv_state := raft.srv_state_deserialize(s.srv_state)
		if srv_state == nil {
			raft.buffer_free(s.srv_state)
			s.srv_state = nil
		} else {
			raft.srv_state_delete(srv_state)
		}
	}

	return
}

