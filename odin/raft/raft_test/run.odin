package raft_test

import "base:runtime"
import "core:flags"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:os"
import "core:thread"

import c "core:c/libc"

import bop "../../libbop"
import os2 "core:os/os2"
import strings "core:strings"

Logger_Ctx :: struct {
	ctx: runtime.Context,
}

run_raft :: proc() {
	buf := bop.raft_buffer_new(32)
	defer bop.raft_buffer_free(buf)
	fmt.println("raft_buffer size:", bop.raft_buffer_size(buf), "  ptr:", rawptr(buf))

	context.logger = log.create_console_logger(.Debug)
	logger_ctx := new(Logger_Ctx)
	logger_ctx.ctx = context
	logger := bop.raft_logger_make(
		rawptr(logger_ctx),
		proc "c" (
			user_data: rawptr,
			level: bop.Raft_Log_Level,
			source_file: cstring,
			func_name: cstring,
			line_number: uintptr,
			log_line: [^]byte,
			log_line_size: uintptr,
		) {
			context = (cast(^Logger_Ctx)user_data).ctx
			log.log(
				bop.Raft_Log_Level_To_Odin_Level[level],
				string(log_line[0:log_line_size]),
				location = runtime.Source_Code_Location {
					file_path = string(source_file),
					line = i32(line_number),
					procedure = string(func_name),
					column = 0,
				},
			)
		},
	)

	asio_options := bop.Raft_Asio_Options{}
	asio_options.thread_pool_size = 2
	asio_options.invoke_req_cb_on_empty_meta = true
	asio_options.invoke_resp_cb_on_empty_meta = true
	fmt.println("creating asio service...")
	asio_service := bop.raft_asio_service_make(&asio_options, logger)

	endpoint := "localhost:15001"
	rpc_client := bop.raft_asio_rpc_client_make(
		asio_service,
		raw_data(endpoint),
		c.size_t(len(endpoint)),
	)
	rpc_listener := bop.raft_asio_rpc_listener_make(asio_service, 15001, logger)
	bop.raft_asio_rpc_client_delete(rpc_client)
	bop.raft_asio_rpc_listener_delete(rpc_listener)

	fmt.println("stopping asio service...")
	bop.raft_asio_service_delete(asio_service)
	bop.raft_logger_delete(logger)

	wire_log_store()
	wire_state_mgr()

	run_raft_server()
}

run_raft_server :: proc() {

	/*
    		user_data: rawptr,
		fsm: ^Raft_FSM_Ptr,
		logger: ^Raft_Logger_Ptr,
		port_number: i32,
		asio_service: ^Raft_Asio_Service_Ptr,
		params_given: ^Raft_Params,
		skip_initial_election_timeout: bool,
		start_server_in_constructor: bool,
		test_mode_flag: bool,
		cb_func: rawptr
    */

	context.logger = log.create_console_logger(.Debug)
	logger_ctx := new(Logger_Ctx)
	logger_ctx.ctx = context
	logger := bop.raft_logger_make(
		rawptr(logger_ctx),
		proc "c" (
			user_data: rawptr,
			level: bop.Raft_Log_Level,
			source_file: cstring,
			func_name: cstring,
			line_number: uintptr,
			log_line: [^]byte,
			log_line_size: uintptr,
		) {
			context = (cast(^Logger_Ctx)user_data).ctx
			log.log(
				bop.Raft_Log_Level_To_Odin_Level[level],
				string(log_line[0:log_line_size]),
				location = runtime.Source_Code_Location {
					file_path = string(source_file),
					line = i32(line_number),
					procedure = string(func_name),
					column = 0,
				},
			)
		},
	)

	user_data: rawptr = nil

	asio_options := bop.Raft_Asio_Options{}
	asio_options.thread_pool_size = 2
	asio_options.invoke_req_cb_on_empty_meta = true
	asio_options.invoke_resp_cb_on_empty_meta = true
	fmt.println("creating asio service...")
	asio_service := bop.raft_asio_service_make(&asio_options, logger)

	endpoint := "127.0.0.1:15001"

	//    os.make_directory(state_dir, 0655)

	srv_config := bop.raft_srv_config_make(
		i32(1),
		i32(1),
		raw_data(endpoint),
		c.size_t(len(endpoint)),
		nil,
		0,
		false,
		i32(1),
	)

	srv_config_ptr := bop.raft_srv_config_ptr_make(srv_config)
	defer bop.raft_srv_config_ptr_delete(srv_config_ptr)


	dir := "." + os2.Path_Separator_String + "data"
	//    state_dir := "./data"
	//    logs_dir := "./data"

	log_store := bop.raft_mdbx_log_store_open(
		raw_data(dir),
		c.size_t(len(dir)),
		logger,
		1024 * 1024 * 64,
		1024 * 1024 * 64,
		1024 * 1024 * 1024,
		1024 * 1024 * 64,
		1024 * 1024 * 64,
		4096,
		0,
		655,
		10000,
	)

	ensure(log_store != nil, "log_store is nil")

	state_mgr := bop.raft_mdbx_state_mgr_open(
		srv_config_ptr,
		raw_data(dir),
		c.size_t(len(dir)),
		logger,
		1024 * 64,
		1024 * 64,
		1024 * 64 * 16,
		1024 * 64,
		1024 * 64 * 4,
		4096,
		0,
		655,
		log_store,
	)

	ensure(state_mgr != nil, "state_mgr is nil")

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

	ensure(fsm != nil, "raft_fsm_make returned nil")

	params := bop.raft_params_make()

	raft_server := bop.raft_server_launch(
		user_data,
		fsm,
		state_mgr,
		logger,
		15001,
		asio_service,
		params,
		false,
		true,
		false,
		nil,
	)

	bop.raft_server_stop(raft_server, 5)

	bop.raft_log_store_delete(log_store)
	bop.raft_state_mgr_delete(state_mgr)
	bop.raft_logger_delete(logger)
}

setup_options :: proc() {
	Options :: struct {
		//        file: os.Handle `args:"pos=0,required,file=r" usage:"Input file."`,
		//        output: os.Handle `args:"pos=1,file=cw" usage:"Output file."`,
		//
		//        hub: net.Host_Or_Endpoint `usage:"Internet address to contact for updates."`,
		//        schedule: datetime.DateTime `usage:"Launch tasks at this time."`,
		//
		//        opt: Optimization_Level `usage:"Optimization level."`,
		//        todo: [dynamic]string `usage:"Todo items."`,
		//
		//        accuracy: Fixed_Point1_1 `args:"required" usage:"Lenience in FLOP calculations."`,
		iterations: int `usage:"Run this many times."`,


		// Many different requirement styles:

		// gadgets: [dynamic]string `args:"required=1" usage:"gadgets"`,
		// widgets: [dynamic]string `args:"required=<3" usage:"widgets"`,
		// foos: [dynamic]string `args:"required=2<4"`,
		// bars: [dynamic]string `args:"required=3<4"`,
		// bots: [dynamic]string `args:"required"`,

		// (Maps) Only available in Odin style:

		// assignments: map[string]u8 `args:"name=assign" usage:"Number of jobs per worker."`,

		// (Manifold) Only available in UNIX style:

		// bots: [dynamic]string `args:"manifold=2,required"`,
		verbose:    bool `usage:"Show verbose output."`,
		debug:      bool `args:"hidden" usage:"print debug info"`,
		overflow:   [dynamic]string `usage:"Any extra arguments go here."`,
	}

	opt: Options
	style: flags.Parsing_Style = .Odin

	//    flags.register_type_setter(my_custom_type_setter)
	//    flags.register_flag_checker(my_custom_flag_checker)
	flags.parse_or_exit(&opt, os.args, style)

	fmt.printfln("%#v", opt)
	fmt.println("")

	//    if opt.output != 0 {
	//        os.write_string(opt.output, "Hellope!\n")
	//    }
}

main :: proc() {
	run_raft_server()
}

main0 :: proc() {
	setup_options()

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

	ensure(fsm != nil, "raft_fsm_make returned nil")

	fsm_get_next_batch_size_hint_in_bytes(nil)
	fsm_get_next_batch_size_hint_in_bytes(nil)

	bop.raft_fsm_delete(fsm)

	run_raft()

	fmt.println("passed!")
}
