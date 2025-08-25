package bopd

// import "base:runtime"
// import "core:fmt"
// import "core:log"
// import "core:time"
// import "core:thread"
// import "core:sync"

// import us "../odin/usockets"

// // Example usage of the binary streaming server
// main :: proc() {
// 	context.logger = log.create_console_logger()
// 	defer log.destroy_console_logger(context.logger)

// 	// Create and start streaming server
// 	listener, err := listener_make("0.0.0.0", 8080, {}, true)
// 	if err != nil {
// 		log.errorf("Failed to create listener: %v", err)
// 		return
// 	}
// 	defer listener_delete(listener)

// 	// Get the streaming context to register custom handlers
// 	server_context := cast(^Server_Context)us.socket_context_ext(
// 		listener.ssl,
// 		listener.socket_context,
// 	)
// 	streaming_ctx := server_context.streaming_ctx

// 	// Register custom message handlers
// 	register_example_handlers(streaming_ctx)

// 	// Start a background task to send periodic push messages
// 	push_thread := thread.create_and_start(push_message_worker)
// 	push_thread.data = streaming_ctx
// 	defer thread.destroy(push_thread)

// 	log.info("Binary streaming server started on port 8080")
// 	log.info("Protocol: Fully binary with efficient serialization")
// 	log.info("Header: 12 bytes (version, type, flags, reserved, message_id, payload_length)")
// 	log.info("Payload: Binary encoded data structures")
// 	log.info("Supported message types: Request/Response, Push, Ping/Pong")
// 	log.info("Built-in methods: subscribe, unsubscribe")
// 	log.info("Example methods: echo, get_time, math.add, broadcast")

// 	// Run the server (blocks until stopped)
// 	if !listener_run(listener) {
// 		log.error("Failed to run listener")
// 	}
// }

// // Register example message handlers
// register_example_handlers :: proc(ctx: ^Streaming_Context) {
// 	register_handler(ctx, "echo", echo_handler)
// 	register_handler(ctx, "get_time", get_time_handler)
// 	register_handler(ctx, "math.add", math_add_handler)
// 	register_handler(ctx, "math.multiply", math_multiply_handler)
// 	register_handler(ctx, "broadcast", broadcast_handler)
// 	register_handler(ctx, "get_info", get_info_handler)
// }

// // Echo handler - returns the same data that was sent
// echo_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	log.infof("Echo request from client %d", client_id)
// 	return request.params, .None, ""
// }

// // Get current time handler
// get_time_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	now := time.now()
// 	timestamp := time.to_unix_nanoseconds(now)

// 	result_map := make_binary_map()
// 	result_map["timestamp"] = to_binary(i64(timestamp))
// 	result_map["unix_seconds"] = to_binary(f64(timestamp) / 1e9)
// 	result_map["iso8601"] = to_binary(fmt.aprintf("%v", now))
// 	result_map["server_uptime"] = to_binary(i64(time.since(time.now())._nsec))

// 	log.infof("Time request from client %d", client_id)
// 	return result_map, .None, ""
// }

// // Math addition handler
// math_add_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	params_map, is_map := request.params.(map[string]Binary_Value)
// 	if !is_map {
// 		return Null_Value{}, .Invalid_Message, "Parameters must be a map"
// 	}

// 	a_val, has_a := params_map["a"]
// 	b_val, has_b := params_map["b"]

// 	if !has_a || !has_b {
// 		return Null_Value{}, .Invalid_Message, "Missing 'a' or 'b' parameters"
// 	}

// 	a_num, a_ok := extract_binary_value(a_val, f64)
// 	if !a_ok {
// 		return Null_Value{}, .Invalid_Message, "Parameter 'a' must be a number"
// 	}

// 	b_num, b_ok := extract_binary_value(b_val, f64)
// 	if !b_ok {
// 		return Null_Value{}, .Invalid_Message, "Parameter 'b' must be a number"
// 	}

// 	sum := a_num + b_num

// 	result_map := make_binary_map()
// 	result_map["sum"] = to_binary(sum)
// 	result_map["a"] = a_val
// 	result_map["b"] = b_val
// 	result_map["operation"] = to_binary("addition")

// 	log.infof("Math.add request from client %d: %f + %f = %f", client_id, a_num, b_num, sum)
// 	return result_map, .None, ""
// }

// // Math multiplication handler
// math_multiply_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	params_map, is_map := request.params.(map[string]Binary_Value)
// 	if !is_map {
// 		return Null_Value{}, .Invalid_Message, "Parameters must be a map"
// 	}

// 	a_val, has_a := params_map["a"]
// 	b_val, has_b := params_map["b"]

// 	if !has_a || !has_b {
// 		return Null_Value{}, .Invalid_Message, "Missing 'a' or 'b' parameters"
// 	}

// 	a_num, a_ok := extract_binary_value(a_val, f64)
// 	if !a_ok {
// 		return Null_Value{}, .Invalid_Message, "Parameter 'a' must be a number"
// 	}

// 	b_num, b_ok := extract_binary_value(b_val, f64)
// 	if !b_ok {
// 		return Null_Value{}, .Invalid_Message, "Parameter 'b' must be a number"
// 	}

// 	product := a_num * b_num

// 	result_map := make_binary_map()
// 	result_map["product"] = to_binary(product)
// 	result_map["a"] = a_val
// 	result_map["b"] = b_val
// 	result_map["operation"] = to_binary("multiplication")

// 	log.infof("Math.multiply request from client %d: %f * %f = %f", client_id, a_num, b_num, product)
// 	return result_map, .None, ""
// }

// // Broadcast handler - sends a message to all subscribers of an event
// broadcast_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	params_map, is_map := request.params.(map[string]Binary_Value)
// 	if !is_map {
// 		return Null_Value{}, .Invalid_Message, "Parameters must be a map"
// 	}

// 	event_val, has_event := params_map["event"]
// 	data_val, has_data := params_map["data"]

// 	if !has_event {
// 		return Null_Value{}, .Invalid_Message, "Missing 'event' parameter"
// 	}

// 	event_name, is_string := extract_binary_value(event_val, string)
// 	if !is_string {
// 		return Null_Value{}, .Invalid_Message, "Event name must be a string"
// 	}

// 	broadcast_data := data_val if has_data else Null_Value{}
// 	broadcast_push(ctx, event_name, broadcast_data)

// 	result_map := make_binary_map()
// 	result_map["broadcasted"] = to_binary(true)
// 	result_map["event"] = event_val
// 	result_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))

// 	log.infof("Broadcast request from client %d for event '%s'", client_id, event_name)
// 	return result_map, .None, ""
// }

// // Get server info handler
// get_info_handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string) {
// 	sync.rw_mutex_shared_guard(&ctx.clients_mu)
// 	client_count := len(ctx.clients)

// 	sync.rw_mutex_shared_guard(&ctx.channels_mu)
// 	event_count := len(ctx.broadcast_channels)

// 	sync.rw_mutex_shared_guard(&ctx.handlers_mu)
// 	handler_count := len(ctx.message_handlers)

// 	result_map := make_binary_map()
// 	result_map["protocol_version"] = to_binary(u8(PROTOCOL_VERSION))
// 	result_map["server_name"] = to_binary("BOP Binary Streaming Server")
// 	result_map["connected_clients"] = to_binary(u32(client_count))
// 	result_map["registered_events"] = to_binary(u32(event_count))
// 	result_map["registered_handlers"] = to_binary(u32(handler_count))
// 	result_map["max_payload_size"] = to_binary(u32(MAX_PAYLOAD_SIZE))
// 	result_map["header_size"] = to_binary(u8(HEADER_SIZE))
// 	result_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))

// 	// Add handler list
// 	handler_array := make([]Binary_Value, 0, handler_count)
// 	for method in ctx.message_handlers {
// 		append(&handler_array, to_binary(method))
// 	}
// 	result_map["available_methods"] = handler_array

// 	log.infof("Server info request from client %d", client_id)
// 	return result_map, .None, ""
// }

// // Background worker that sends periodic push messages
// push_message_worker :: proc(thread: ^thread.Thread) {
// 	ctx := cast(^Streaming_Context)thread.data
// 	counter := 0

// 	for {
// 		time.sleep(5 * time.Second)

// 		counter += 1

// 		// Send periodic heartbeat
// 		heartbeat_map := make_binary_map()
// 		heartbeat_map["counter"] = to_binary(u32(counter))
// 		heartbeat_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))
// 		heartbeat_map["message"] = to_binary("Server heartbeat")
// 		heartbeat_map["type"] = to_binary("heartbeat")

// 		broadcast_push(ctx, "heartbeat", heartbeat_map)
// 		log.infof("Sent heartbeat #%d", counter)

// 		// Send random data every 10 heartbeats
// 		if counter % 10 == 0 {
// 			random_map := make_binary_map()
// 			random_map["type"] = to_binary("random_event")
// 			random_map["value"] = to_binary(i32(counter * 42))
// 			random_map["description"] = to_binary("This is a periodic random event")
// 			random_map["counter"] = to_binary(u32(counter))
// 			random_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))

// 			// Add some binary data
// 			binary_data := make([]byte, 16)
// 			for i in 0..<16 {
// 				binary_data[i] = u8(counter + i)
// 			}
// 			random_map["binary_data"] = to_binary(binary_data)

// 			broadcast_push(ctx, "random", random_map)
// 			log.infof("Sent random event #%d", counter / 10)
// 		}

// 		// Send status update every 20 heartbeats
// 		if counter % 20 == 0 {
// 			sync.rw_mutex_shared_guard(&ctx.clients_mu)
// 			client_count := len(ctx.clients)

// 			status_map := make_binary_map()
// 			status_map["type"] = to_binary("status_update")
// 			status_map["connected_clients"] = to_binary(u32(client_count))
// 			status_map["uptime_heartbeats"] = to_binary(u32(counter))
// 			status_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))

// 			broadcast_push(ctx, "status", status_map)
// 			log.infof("Sent status update: %d clients connected", client_count)
// 		}
// 	}
// }

// /*
// Binary Protocol Documentation:

// Message Format:
// Header (12 bytes):
//   - version (1 byte): Protocol version (currently 1)
//   - type (1 byte): Message type (Request=1, Response=2, Push=3, Error=4, Ping=5, Pong=6)
//   - flags (1 byte): Reserved for future use
//   - reserved (1 byte): Reserved for future use
//   - message_id (4 bytes): Unique message identifier (little endian)
//   - payload_length (4 bytes): Length of payload in bytes (little endian)

// Payload (variable):
//   Binary encoded data structures using efficient type prefixes

// Data Types:
//   - Null (0x00): No data
//   - Bool (0x01): 1 byte (0=false, 1=true)
//   - I8/I16/I32/I64 (0x02-0x05): Signed integers (little endian)
//   - U8/U16/U32/U64 (0x06-0x09): Unsigned integers (little endian)
//   - F32/F64 (0x0A-0x0B): Floating point (little endian)
//   - String (0x0C): Length-prefixed UTF-8 string
//   - Bytes (0x0D): Length-prefixed byte array
//   - Array (0x0E): Count-prefixed array of binary values
//   - Map (0x0F): Count-prefixed key-value pairs

// Example Usage:

// 1. Connect to server on port 8080
// 2. Send binary messages with proper headers
// 3. Handle responses and push messages

// Available Methods:
// - "echo": Returns the same parameters that were sent
// - "get_time": Returns current server time in various formats
// - "math.add": Adds two numbers (requires 'a' and 'b' parameters)
// - "math.multiply": Multiplies two numbers (requires 'a' and 'b' parameters)
// - "broadcast": Sends a push message to all subscribers (requires 'event' and optional 'data')
// - "get_info": Returns server information and statistics
// - "subscribe": Subscribe to push events (requires 'event' parameter)
// - "unsubscribe": Unsubscribe from push events (requires 'event' parameter)

// Push Events:
// - "heartbeat": Sent every 5 seconds with counter and timestamp
// - "random": Sent every 50 seconds with random data and binary payload
// - "status": Sent every 100 seconds with server status information
// - Custom events via the "broadcast" method

// Benefits of Binary Protocol:
// - Efficient serialization (much smaller than JSON)
// - Type safety with explicit type encoding
// - Fast parsing without string processing
// - Support for binary data without base64 encoding
// - Extensible with reserved fields for future features
// */
