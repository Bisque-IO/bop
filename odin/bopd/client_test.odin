package bopd

// import "base:runtime"
// import "core:fmt"
// import "core:log"
// import "core:mem"
// import "core:net"
// import "core:time"
// import "core:thread"

// // Binary test client for the streaming protocol
// Binary_Test_Client :: struct {
// 	socket:         net.TCP_Socket,
// 	connected:      bool,
// 	next_msg_id:    u32,
// 	receive_buffer: [dynamic]byte,
// 	pending_header: bool,
// 	expected_len:   u32,
// }

// // Initialize test client
// client_init :: proc(client: ^Binary_Test_Client, allocator := context.allocator) {
// 	client.next_msg_id = 1
// 	client.receive_buffer = make([dynamic]byte, allocator)
// }

// // Clean up test client
// client_destroy :: proc(client: ^Binary_Test_Client, allocator := context.allocator) {
// 	if client.connected {
// 		net.close(client.socket)
// 	}
// 	delete(client.receive_buffer, allocator)
// }

// // Connect to server
// client_connect :: proc(client: ^Binary_Test_Client, host: string, port: int) -> bool {
// 	endpoint := net.Endpoint{
// 		address = net.IP4_Address{127, 0, 0, 1},
// 		port = port,
// 	}

// 	socket, dial_err := net.dial_tcp(endpoint)
// 	if dial_err != nil {
// 		log.errorf("Failed to connect: %v", dial_err)
// 		return false
// 	}

// 	client.socket = socket
// 	client.connected = true
// 	log.infof("Connected to %s:%d", host, port)
// 	return true
// }

// // Disconnect from server
// client_disconnect :: proc(client: ^Binary_Test_Client) {
// 	if client.connected {
// 		net.close(client.socket)
// 		client.connected = false
// 		log.info("Disconnected from server")
// 	}
// }

// // Get next message ID
// client_next_msg_id :: proc(client: ^Binary_Test_Client) -> u32 {
// 	id := client.next_msg_id
// 	client.next_msg_id += 1
// 	return id
// }

// // Send a binary request
// client_send_request :: proc(
// 	client: ^Binary_Test_Client,
// 	method: string,
// 	params: Binary_Value = Null_Value{},
// 	allocator := context.allocator,
// ) -> (u32, bool) {
// 	if !client.connected do return 0, false

// 	msg_id := client_next_msg_id(client)
// 	message, create_err := create_request(msg_id, method, params, allocator)
// 	if create_err != nil {
// 		log.errorf("Failed to create request: %v", create_err)
// 		return 0, false
// 	}
// 	defer message_destroy(&message, allocator)

// 	serialized, serialize_err := serialize_message(message, allocator)
// 	if serialize_err != nil {
// 		log.errorf("Failed to serialize message: %v", serialize_err)
// 		return 0, false
// 	}
// 	defer delete(serialized, allocator)

// 	bytes_sent, send_err := net.send_tcp(client.socket, serialized)
// 	if send_err != nil {
// 		log.errorf("Failed to send message: %v", send_err)
// 		return 0, false
// 	}

// 	if bytes_sent != len(serialized) {
// 		log.errorf("Partial send: %d/%d bytes", bytes_sent, len(serialized))
// 		return 0, false
// 	}

// 	log.infof("Sent request: method='%s', id=%d", method, msg_id)
// 	return msg_id, true
// }

// // Send a ping
// client_send_ping :: proc(client: ^Binary_Test_Client, allocator := context.allocator) -> (u32, bool) {
// 	if !client.connected do return 0, false

// 	msg_id := client_next_msg_id(client)
// 	message := create_ping(msg_id)
// 	defer message_destroy(&message, allocator)

// 	serialized, serialize_err := serialize_message(message, allocator)
// 	if serialize_err != nil {
// 		log.errorf("Failed to serialize ping: %v", serialize_err)
// 		return 0, false
// 	}
// 	defer delete(serialized, allocator)

// 	bytes_sent, send_err := net.send_tcp(client.socket, serialized)
// 	if send_err != nil {
// 		log.errorf("Failed to send ping: %v", send_err)
// 		return 0, false
// 	}

// 	if bytes_sent != len(serialized) {
// 		log.errorf("Partial ping send: %d/%d bytes", bytes_sent, len(serialized))
// 		return 0, false
// 	}

// 	log.infof("Sent ping: id=%d", msg_id)
// 	return msg_id, true
// }

// // Receive and process messages
// client_receive_messages :: proc(client: ^Binary_Test_Client, timeout_ms: int = 100, allocator := context.allocator) -> bool {
// 	if !client.connected do return false

// 	// Set socket to non-blocking for timeout
// 	// Note: This is a simplified approach - in production you'd use proper select/poll
// 	temp_buffer: [4096]byte
// 	bytes_received, recv_err := net.recv_tcp(client.socket, temp_buffer[:])
// 	if recv_err != nil {
// 		if recv_err == .Would_Block {
// 			return true // No data available, but connection is still good
// 		}
// 		log.errorf("Failed to receive data: %v", recv_err)
// 		return false
// 	}

// 	if bytes_received == 0 {
// 		log.info("Connection closed by server")
// 		client.connected = false
// 		return false
// 	}

// 	// Append to receive buffer
// 	old_len := len(client.receive_buffer)
// 	resize(&client.receive_buffer, old_len + bytes_received)
// 	copy(client.receive_buffer[old_len:], temp_buffer[:bytes_received])

// 	// Process complete messages
// 	messages_processed := 0
// 	for {
// 		buf_len := len(client.receive_buffer)

// 		// Read header if not done yet
// 		if !client.pending_header {
// 			if buf_len < HEADER_SIZE do break // Need more data for header

// 			header, header_ok := deserialize_header(client.receive_buffer[:HEADER_SIZE])
// 			if !header_ok {
// 				log.error("Invalid message header received")
// 				clear(&client.receive_buffer)
// 				break
// 			}

// 			client.expected_len = header.payload_length
// 			client.pending_header = true
// 		}

// 		// Check if we have complete message
// 		total_expected := HEADER_SIZE + int(client.expected_len)
// 		if buf_len < total_expected do break // Need more data

// 		// Extract complete message
// 		header, _ := deserialize_header(client.receive_buffer[:HEADER_SIZE])
// 		payload := client.receive_buffer[HEADER_SIZE:total_expected] if client.expected_len > 0 else nil

// 		message := Message{
// 			header = header,
// 			payload = slice.clone(payload, allocator),
// 		}

// 		// Process the message
// 		client_handle_message(message, allocator)
// 		messages_processed += 1

// 		// Remove processed message from buffer
// 		remaining := client.receive_buffer[total_expected:]
// 		if len(remaining) > 0 {
// 			new_data := make([dynamic]byte, len(remaining), allocator)
// 			copy(new_data[:], remaining)
// 			delete(client.receive_buffer, allocator)
// 			client.receive_buffer = new_data
// 		} else {
// 			clear(&client.receive_buffer)
// 		}

// 		client.pending_header = false
// 		client.expected_len = 0

// 		message_destroy(&message, allocator)
// 	}

// 	return true
// }

// // Handle received message
// client_handle_message :: proc(message: Message, allocator := context.allocator) {
// 	msg_type := Message_Type(message.header.type)

// 	switch msg_type {
// 	case .Response:
// 		response, parse_err := parse_response(message, allocator)
// 		if parse_err != nil {
// 			log.errorf("Failed to parse response: %v", parse_err)
// 			return
// 		}

// 		if response.error != .None {
// 			log.errorf("Response error (id=%d): code=%v, message='%s'",
// 				message.header.message_id, response.error, response.error_message)
// 		} else {
// 			log.infof("Response (id=%d): %v", message.header.message_id, format_binary_value(response.result))
// 		}

// 	case .Push:
// 		push, parse_err := parse_push(message, allocator)
// 		if parse_err != nil {
// 			log.errorf("Failed to parse push: %v", parse_err)
// 			return
// 		}

// 		log.infof("Push message: event='%s', data=%v", push.event, format_binary_value(push.data))

// 	case .Pong:
// 		log.infof("Received pong: id=%d", message.header.message_id)

// 	case .Error:
// 		log.errorf("Received error message: id=%d", message.header.message_id)

// 	case:
// 		log.warnf("Unexpected message type: %v", msg_type)
// 	}
// }

// // Format binary value for display
// format_binary_value :: proc(value: Binary_Value, depth: int = 0) -> string {
// 	if depth > 5 {
// 		return "(...)"
// 	}

// 	switch v in value {
// 	case Null_Value:
// 		return "null"
// 	case bool:
// 		return fmt.aprintf("%t", v)
// 	case i8, i16, i32, i64:
// 		return fmt.aprintf("%v", v)
// 	case u8, u16, u32, u64:
// 		return fmt.aprintf("%v", v)
// 	case f32, f64:
// 		return fmt.aprintf("%.6f", v)
// 	case string:
// 		return fmt.aprintf("\"%s\"", v)
// 	case []byte:
// 		if len(v) <= 16 {
// 			return fmt.aprintf("bytes[%d]:%v", len(v), v)
// 		} else {
// 			return fmt.aprintf("bytes[%d]:[%v...]", len(v), v[:16])
// 		}
// 	case []Binary_Value:
// 		if len(v) == 0 {
// 			return "[]"
// 		}
// 		if len(v) > 5 {
// 			return fmt.aprintf("[%s, ... (%d total)]", format_binary_value(v[0], depth+1), len(v))
// 		}
// 		result := "["
// 		for i, item in v {
// 			if i > 0 do result = fmt.aprintf("%s, ", result)
// 			result = fmt.aprintf("%s%s", result, format_binary_value(item, depth+1))
// 		}
// 		result = fmt.aprintf("%s]", result)
// 		return result
// 	case map[string]Binary_Value:
// 		if len(v) == 0 {
// 			return "{}"
// 		}
// 		result := "{"
// 		count := 0
// 		for key, val in v {
// 			if count > 0 do result = fmt.aprintf("%s, ", result)
// 			result = fmt.aprintf("%s\"%s\": %s", result, key, format_binary_value(val, depth+1))
// 			count += 1
// 			if count >= 5 {
// 				result = fmt.aprintf("%s, ... (%d total)", result, len(v))
// 				break
// 			}
// 		}
// 		result = fmt.aprintf("%s}", result)
// 		return result
// 	}
// 	return "unknown"
// }

// // Main test client function
// test_binary_client_main :: proc() {
// 	context.logger = log.create_console_logger()
// 	defer log.destroy_console_logger(context.logger)

// 	client: Binary_Test_Client
// 	client_init(&client)
// 	defer client_destroy(&client)

// 	// Connect to server
// 	if !client_connect(&client, "127.0.0.1", 8080) {
// 		log.error("Failed to connect to server")
// 		return
// 	}
// 	defer client_disconnect(&client)

// 	log.info("Starting binary test client...")

// 	// Test 1: Get server info
// 	log.info("Test 1: Get server info")
// 	client_send_request(&client, "get_info")
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 2: Echo request with various data types
// 	log.info("Test 2: Echo request with mixed data types")
// 	echo_map := make_binary_map()
// 	echo_map["string"] = to_binary("Hello, binary world!")
// 	echo_map["integer"] = to_binary(i32(42))
// 	echo_map["float"] = to_binary(f64(3.14159))
// 	echo_map["boolean"] = to_binary(true)
// 	echo_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))

// 	// Add binary data
// 	binary_data := make([]byte, 8)
// 	for i in 0..<8 {
// 		binary_data[i] = u8(i * 16 + i)
// 	}
// 	echo_map["binary"] = to_binary(binary_data)

// 	// Add nested array
// 	nested_array := make([]Binary_Value, 3)
// 	nested_array[0] = to_binary("first")
// 	nested_array[1] = to_binary(i32(123))
// 	nested_array[2] = to_binary(f32(2.5))
// 	echo_map["array"] = nested_array

// 	client_send_request(&client, "echo", echo_map)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 3: Subscribe to events
// 	log.info("Test 3: Subscribe to heartbeat events")
// 	subscribe_map := make_binary_map()
// 	subscribe_map["event"] = to_binary("heartbeat")
// 	client_send_request(&client, "subscribe", subscribe_map)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 4: Subscribe to more events
// 	log.info("Test 4: Subscribe to random and status events")
// 	subscribe_random := make_binary_map()
// 	subscribe_random["event"] = to_binary("random")
// 	client_send_request(&client, "subscribe", subscribe_random)

// 	subscribe_status := make_binary_map()
// 	subscribe_status["event"] = to_binary("status")
// 	client_send_request(&client, "subscribe", subscribe_status)

// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 5: Get time
// 	log.info("Test 5: Get server time")
// 	client_send_request(&client, "get_time")
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 6: Math operations
// 	log.info("Test 6: Math operations")
// 	math_add_map := make_binary_map()
// 	math_add_map["a"] = to_binary(f64(15.5))
// 	math_add_map["b"] = to_binary(f64(24.3))
// 	client_send_request(&client, "math.add", math_add_map)

// 	math_multiply_map := make_binary_map()
// 	math_multiply_map["a"] = to_binary(f64(7.0))
// 	math_multiply_map["b"] = to_binary(f64(6.0))
// 	client_send_request(&client, "math.multiply", math_multiply_map)

// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 7: Send ping
// 	log.info("Test 7: Send ping")
// 	client_send_ping(&client)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 8: Broadcast a custom message
// 	log.info("Test 8: Broadcast custom message")
// 	broadcast_map := make_binary_map()
// 	broadcast_map["event"] = to_binary("custom_test")

// 	broadcast_data := make_binary_map()
// 	broadcast_data["sender"] = to_binary("binary_test_client")
// 	broadcast_data["message"] = to_binary("Hello from binary client!")
// 	broadcast_data["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))
// 	broadcast_data["test_number"] = to_binary(u32(12345))

// 	broadcast_map["data"] = broadcast_data
// 	client_send_request(&client, "broadcast", broadcast_map)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 9: Test with large data
// 	log.info("Test 9: Test with large binary data")
// 	large_data := make([]byte, 4096)
// 	for i in 0..<len(large_data) {
// 		large_data[i] = u8(i % 256)
// 	}

// 	large_echo_map := make_binary_map()
// 	large_echo_map["large_binary"] = to_binary(large_data)
// 	large_echo_map["description"] = to_binary("Testing large binary payload")
// 	client_send_request(&client, "echo", large_echo_map)
// 	time.sleep(200 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 10: Invalid method (should return error)
// 	log.info("Test 10: Invalid method (should return error)")
// 	client_send_request(&client, "invalid_method")
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Test 11: Invalid parameters
// 	log.info("Test 11: Math operation with invalid parameters")
// 	invalid_math_map := make_binary_map()
// 	invalid_math_map["a"] = to_binary("not_a_number")
// 	invalid_math_map["b"] = to_binary(f64(5.0))
// 	client_send_request(&client, "math.add", invalid_math_map)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Listen for push messages for a while
// 	log.info("Listening for push messages for 30 seconds...")
// 	start_time := time.now()
// 	message_count := 0

// 	for time.since(start_time) < 30 * time.Second {
// 		if client_receive_messages(&client) {
// 			message_count += 1
// 		}
// 		time.sleep(250 * time.Millisecond)

// 		// Send a ping every 10 seconds to keep connection alive
// 		if int(time.since(start_time).seconds) % 10 == 0 && int(time.since(start_time).seconds) > 0 {
// 			client_send_ping(&client)
// 		}
// 	}

// 	// Test unsubscribe
// 	log.info("Test 12: Unsubscribe from events")
// 	unsubscribe_map := make_binary_map()
// 	unsubscribe_map["event"] = to_binary("heartbeat")
// 	client_send_request(&client, "unsubscribe", unsubscribe_map)
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	// Final server info check
// 	log.info("Final: Get server info again")
// 	client_send_request(&client, "get_info")
// 	time.sleep(100 * time.Millisecond)
// 	client_receive_messages(&client)

// 	log.info("Binary test client finished")
// }

// // Alternative main function for running just the client test
// run_binary_test_client :: proc() {
// 	test_binary_client_main()
// }

// // Performance test function
// performance_test :: proc() {
// 	context.logger = log.create_console_logger()
// 	defer log.destroy_console_logger(context.logger)

// 	client: Binary_Test_Client
// 	client_init(&client)
// 	defer client_destroy(&client)

// 	if !client_connect(&client, "127.0.0.1", 8080) {
// 		log.error("Failed to connect to server")
// 		return
// 	}
// 	defer client_disconnect(&client)

// 	log.info("Starting performance test...")

// 	// Performance test: Send many small requests
// 	start_time := time.now()
// 	request_count := 1000

// 	echo_map := make_binary_map()
// 	echo_map["test"] = to_binary("performance_test")
// 	echo_map["counter"] = to_binary(i32(0))

// 	for i in 0..<request_count {
// 		echo_map["counter"] = to_binary(i32(i))
// 		client_send_request(&client, "echo", echo_map)

// 		// Process responses periodically
// 		if i % 10 == 0 {
// 			client_receive_messages(&client)
// 		}
// 	}

// 	// Process remaining responses
// 	for i in 0..<10 {
// 		client_receive_messages(&client)
// 		time.sleep(10 * time.Millisecond)
// 	}

// 	elapsed := time.since(start_time)
// 	requests_per_second := f64(request_count) / elapsed.seconds

// 	log.infof("Performance test completed:")
// 	log.infof("- Requests sent: %d", request_count)
// 	log.infof("- Time elapsed: %.3f seconds", elapsed.seconds)
// 	log.infof("- Requests per second: %.1f", requests_per_second)
// }
