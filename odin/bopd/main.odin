package bopd

import "base:runtime"

import c "core:c/libc"
import "core:fmt"
import "core:log"
import "core:os"
import "core:os/os2"
import "core:strconv"
import "core:strings"
import "core:time"

import us "../odin/usockets"

import "../odin/raft"

Logger_Ctx :: struct {
	ctx: runtime.Context,
}

// Program modes
Program_Mode :: enum {
	Server,
	Client,
	Performance,
	Help,
}

// Configuration
Config :: struct {
	mode:         Program_Mode,
	host:         string,
	port:         int,
	verbose:      bool,
	enable_ssl:   bool,
	cert_file:    string,
	key_file:     string,
}

// Default configuration
DEFAULT_CONFIG :: Config{
	mode    = .Server,
	host    = "0.0.0.0",
	port    = 8080,
	verbose = false,
}

// Print usage information
print_usage :: proc() {
	fmt.println("BOP Binary Streaming Server")
	fmt.println("===========================")
	fmt.println()
	fmt.println("A high-performance binary streaming server with real-time messaging.")
	fmt.println()
	fmt.println("USAGE:")
	fmt.println("    bopd [MODE] [OPTIONS]")
	fmt.println()
	fmt.println("MODES:")
	fmt.println("    server      Start the streaming server (default)")
	fmt.println("    client      Run the test client")
	fmt.println("    perf        Run performance tests")
	fmt.println("    help        Show this help message")
	fmt.println()
	fmt.println("OPTIONS:")
	fmt.println("    --host <addr>       Server host address (default: 0.0.0.0)")
	fmt.println("    --port <port>       Server port number (default: 8080)")
	fmt.println("    --verbose           Enable verbose logging")
	fmt.println("    --ssl               Enable SSL/TLS")
	fmt.println("    --cert <file>       SSL certificate file")
	fmt.println("    --key <file>        SSL private key file")
	fmt.println()
	fmt.println("EXAMPLES:")
	fmt.println("    bopd server --port 9000 --verbose")
	fmt.println("    bopd client --host 127.0.0.1 --port 9000")
	fmt.println("    bopd perf --host localhost")
	fmt.println()
	fmt.println("PROTOCOL:")
	fmt.println("    Binary streaming protocol with 12-byte headers")
	fmt.println("    - Efficient binary serialization")
	fmt.println("    - Request/Response and Push messaging")
	fmt.println("    - Built-in subscription management")
	fmt.println("    - Ping/Pong keepalive")
	fmt.println()
	fmt.println("FEATURES:")
	fmt.println("    - Zero-copy binary protocol")
	fmt.println("    - Real-time push notifications")
	fmt.println("    - Event subscription system")
	fmt.println("    - High-performance uSockets backend")
	fmt.println("    - Multi-client support")
	fmt.println("    - Configurable SSL/TLS")
}

// Parse command line arguments
parse_args :: proc(args: []string) -> Config {
	config := DEFAULT_CONFIG

	if len(args) == 0 {
		return config
	}

	// Parse mode
	mode_str := args[0]
	switch mode_str {
	case "server", "s":
		config.mode = .Server
	case "client", "c":
		config.mode = .Client
	case "perf", "performance", "p":
		config.mode = .Performance
	case "help", "h", "--help", "-h":
		config.mode = .Help
	case:
		// If first arg starts with --, treat as option and default to server mode
		if strings.has_prefix(mode_str, "--") {
			config.mode = .Server
			// Parse from index 0
		} else {
			fmt.printf("Unknown mode: %s\n", mode_str)
			config.mode = .Help
			return config
		}
	}

	// Parse options
	start_idx := 1 if config.mode != .Server || !strings.has_prefix(args[0], "--") else 0

	for i := start_idx; i < len(args); i += 1 {
		arg := args[i]

		if strings.has_prefix(arg, "--") {
			option := strings.trim_prefix(arg, "--")

			switch option {
			case "verbose", "v":
				config.verbose = true
			case "ssl":
				config.enable_ssl = true
			case "host":
				if i + 1 < len(args) {
					config.host = args[i + 1]
					i += 1
				} else {
					fmt.println("Error: --host requires a value")
					config.mode = .Help
					return config
				}
			case "port":
				if i + 1 < len(args) {
					port_str := args[i + 1]
					if port, ok := strconv.parse_i64(port_str); ok && port > 0 && port <= 65535 {
						config.port = int(port)
						i += 1
					} else {
						fmt.printf("Error: Invalid port number: %s\n", port_str)
						config.mode = .Help
						return config
					}
				} else {
					fmt.println("Error: --port requires a value")
					config.mode = .Help
					return config
				}
			case "cert":
				if i + 1 < len(args) {
					config.cert_file = args[i + 1]
					i += 1
				} else {
					fmt.println("Error: --cert requires a file path")
					config.mode = .Help
					return config
				}
			case "key":
				if i + 1 < len(args) {
					config.key_file = args[i + 1]
					i += 1
				} else {
					fmt.println("Error: --key requires a file path")
					config.mode = .Help
					return config
				}
			case:
				fmt.printf("Error: Unknown option: %s\n", arg)
				config.mode = .Help
				return config
			}
		} else {
			fmt.printf("Error: Unexpected argument: %s\n", arg)
			config.mode = .Help
			return config
		}
	}

	return config
}

// Setup logging based on configuration
setup_logging :: proc(config: Config) {
	level := log.Level.Info
	if config.verbose {
		level = log.Level.Debug
	}

	context.logger = log.create_console_logger(level)
}

// Run server mode
run_server :: proc(config: Config) -> int {
	log.infof("Starting bop raft %s:%d", config.host, config.port)
	run_raft_server()

	log.infof("Starting bop binary API server on %s:%d", config.host, config.port)

	if config.verbose {
		log.info("Verbose logging enabled")
	}

	if config.enable_ssl {
		log.info("SSL/TLS enabled")
		if config.cert_file == "" || config.key_file == "" {
			log.error("SSL enabled but certificate or key file not specified")
			log.info("Use --cert and --key options to specify SSL certificate files")
			return 1
		}

		if !os.exists(config.cert_file) {
			log.errorf("Certificate file not found: %s", config.cert_file)
			return 1
		}

		if !os.exists(config.key_file) {
			log.errorf("Private key file not found: %s", config.key_file)
			return 1
		}

		log.infof("Using certificate: %s", config.cert_file)
		log.infof("Using private key: %s", config.key_file)
	}

	// Setup SSL options
	ssl_options := us.Socket_Context_Options{}
	if config.enable_ssl {
		ssl_options.cert_file_name = strings.clone_to_cstring(config.cert_file)
		ssl_options.key_file_name = strings.clone_to_cstring(config.key_file)
		defer delete(ssl_options.cert_file_name)
		defer delete(ssl_options.key_file_name)
	}

	// Create and start server
	host_cstr := strings.clone_to_cstring(config.host)
	defer delete(host_cstr)

	listener, err := listener_make(host_cstr, u16(config.port), ssl_options)
	if err != nil {
		log.errorf("Failed to create server: %v", err)
		return 1
	}
	defer listener_delete(listener)

	log.info("Server started successfully")
	log.info("Protocol: Binary streaming with efficient serialization")
	log.info("Features: Request/Response, Push messaging, Subscriptions")
	log.infof("Max payload size: %d bytes", MAX_PAYLOAD_SIZE)
	log.infof("Header size: %d bytes", HEADER_SIZE)

	// Run the server (blocks until stopped)
	if !listener_run(listener) {
		log.error("Server failed to run")
		return 1
	}

	log.info("Server stopped")
	return 0
}

// Run client mode
run_client :: proc(config: Config) -> int {
	log.infof("Starting test client, connecting to %s:%d", config.host, config.port)

	// Run the binary test client
	// test_binary_client_main()

	return 0
}

// Run performance test mode
run_performance :: proc(config: Config) -> int {
	log.infof("Starting performance test, connecting to %s:%d", config.host, config.port)

	// Run performance tests
	// performance_test()

	return 0
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
	logger := raft.logger_make(
		rawptr(logger_ctx),
		proc "c" (
			user_data: rawptr,
			level: raft.Log_Level,
			source_file: cstring,
			func_name: cstring,
			line_number: uintptr,
			log_line: [^]byte,
			log_line_size: uintptr,
		) {
			context = (cast(^Logger_Ctx)user_data).ctx
			text := string(log_line[0:log_line_size])

			_, was_allocation := strings.replace_all(text, "\n", ",")
			if !was_allocation {
			    text = strings.clone(text)
			}

			log.log(
				raft.Log_Level_To_Odin_Level[level],
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

	asio_options := raft.Asio_Options{}
	asio_options.thread_pool_size = 1
	asio_options.invoke_req_cb_on_empty_meta = true
	asio_options.invoke_resp_cb_on_empty_meta = true
	fmt.println("creating asio service...")
	asio_service := raft.asio_service_make(&asio_options, logger)

	endpoint := "127.0.0.1:15001"

	//    os.make_directory(state_dir, 0655)

	srv_config := raft.srv_config_make(
		i32(1),
		i32(1),
		raw_data(endpoint),
		c.size_t(len(endpoint)),
		nil,
		0,
		false,
		i32(1),
	)

	srv_config_ptr := raft.srv_config_ptr_make(srv_config)
	defer raft.srv_config_ptr_delete(srv_config_ptr)


	dir := "." + os2.Path_Separator_String + "data"
	//    state_dir := "./data"
	//    logs_dir := "./data"
	//


	log_store := raft.mdbx_log_store_open(
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

	// state_mgr, err := state_mgr_make(dir)

	state_mgr := raft.mdbx_state_mgr_open(
		srv_config_ptr,
		raw_data(dir),
		c.size_t(len(dir)),
		logger,
		1024 * 4,
		1024 * 4,
		1024 * 64,
		1024 * 4,
		1024 * 4,
		256,
		0,
		655,
		log_store,
	)

	ensure(state_mgr != nil, "state_mgr is nil")

	current_conf: ^raft.Cluster_Config = nil
	rollback_conf: ^raft.Cluster_Config = nil
	fsm := raft.fsm_make(
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

	params := raft.params_make()

	raft_server := raft.server_launch(
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

	// raft.server_stop(raft_server, 5)
	// raft.log_store_delete(log_store)
	// raft.state_mgr_delete(state_mgr)
	// raft.logger_delete(logger)
}

// Main entry point
main :: proc() {
	// Use the thread-local context
	context = tls_context()

	// Parse command line arguments
	args := os.args[1:] // Skip program name
	config := parse_args(args)

	// Handle help mode immediately
	if config.mode == .Help {
		print_usage()
		os.exit(0)
	}

	// Setup logging
	setup_logging(config)
	defer log.destroy_console_logger(context.logger)

	// Print startup banner
	log.info("BOP Binary Streaming Server v1.0")
	log.info("High-performance binary streaming with real-time messaging")

	if config.verbose {
		log.debugf("Configuration: %v", config)
	}

	// Run the appropriate mode
	exit_code: int
	switch config.mode {
	case .Server:
		exit_code = run_server(config)
	case .Client:
		exit_code = run_client(config)
	case .Performance:
		exit_code = run_performance(config)
	case .Help:
		// Already handled above
		exit_code = 0
	}

	// Exit with appropriate code
	if exit_code != 0 {
		log.errorf("Program exited with error code: %d", exit_code)
		os.exit(exit_code)
	}

	log.info("Program completed successfully")
}
