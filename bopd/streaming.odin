package bopd

// import bop "../odin/libbop"
// import "base:runtime"
// import c "core:c/libc"
// import "core:fmt"
// import "core:log"
// import "core:mem"
// import "core:slice"
// import "core:strings"
// import "core:sync"
// import "core:time"

// import us "../odin/usockets"

// // Message buffer for partial message assembly
// Message_Buffer :: struct {
// 	data:         [dynamic]byte,
// 	expected_len: u32,
// 	header_read:  bool,
// }

// // Extended server socket for streaming
// Streaming_Socket :: struct {
// 	using base:    Server_Socket,
// 	client_state:  ^Client_State,
// 	msg_buffer:    Message_Buffer,
// 	last_msg_id:   u32,
// }

// // Client subscription information
// Client_State :: struct {
// 	id:            uintptr,
// 	last_ping:     u64,           // timestamp
// 	subscriptions: map[string]bool, // event name -> subscribed
// 	send_queue:    [dynamic]Message,
// }

// // Streaming server context
// Streaming_Context :: struct {
// 	using base:         Server_Context,
// 	clients:            map[uintptr]^Client_State,
// 	clients_mu:         sync.RW_Mutex,
// 	next_message_id:    u32,
// 	message_handlers:   map[string]Message_Handler,
// 	handlers_mu:        sync.RW_Mutex,
// 	broadcast_channels: map[string][dynamic]uintptr, // event -> client_ids
// 	channels_mu:        sync.RW_Mutex,
// }

// // Message handler function type
// Message_Handler :: proc(
// 	client_id: uintptr,
// 	request: Request,
// 	ctx: ^Streaming_Context,
// ) -> (result: Binary_Value, error_code: Error_Code, error_message: string)

// // Initialize streaming context
// streaming_context_init :: proc(ctx: ^Streaming_Context, allocator := context.allocator) {
// 	ctx.clients = make(map[uintptr]^Client_State, allocator)
// 	ctx.message_handlers = make(map[string]Message_Handler, allocator)
// 	ctx.broadcast_channels = make(map[string][dynamic]uintptr, allocator)
// 	ctx.next_message_id = 1
// }

// // Clean up streaming context
// streaming_context_destroy :: proc(ctx: ^Streaming_Context, allocator := context.allocator) {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	for id, client in ctx.clients {
// 		client_state_destroy(client, allocator)
// 	}
// 	delete(ctx.clients)

// 	sync.rw_mutex_guard(&ctx.handlers_mu)
// 	delete(ctx.message_handlers)

// 	sync.rw_mutex_guard(&ctx.channels_mu)
// 	for event, clients in ctx.broadcast_channels {
// 		delete(clients)
// 	}
// 	delete(ctx.broadcast_channels)
// }

// // Initialize client state
// client_state_init :: proc(
// 	id: uintptr,
// 	allocator := context.allocator,
// ) -> ^Client_State {
// 	state := new(Client_State, allocator)
// 	state.id = id
// 	state.last_ping = u64(time.now()._nsec)
// 	state.subscriptions = make(map[string]bool, allocator)
// 	state.send_queue = make([dynamic]Message, allocator)
// 	return state
// }

// // Clean up client state
// client_state_destroy :: proc(state: ^Client_State, allocator := context.allocator) {
// 	if state == nil do return

// 	delete(state.subscriptions)
// 	for &msg in state.send_queue {
// 		message_destroy(&msg, allocator)
// 	}
// 	delete(state.send_queue)
// 	free(state, allocator)
// }

// // Register a message handler
// register_handler :: proc(
// 	ctx: ^Streaming_Context,
// 	method: string,
// 	handler: Message_Handler,
// 	allocator := context.allocator,
// ) {
// 	sync.rw_mutex_guard(&ctx.handlers_mu)
// 	method_copy := strings.clone(method, allocator)
// 	ctx.message_handlers[method_copy] = handler
// }

// // Get next message ID atomically
// get_next_message_id :: proc(ctx: ^Streaming_Context) -> u32 {
// 	return sync.atomic_add(&ctx.next_message_id, 1)
// }

// // Add client to streaming context
// add_client :: proc(ctx: ^Streaming_Context, client_id: uintptr, allocator := context.allocator) {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	ctx.clients[client_id] = client_state_init(client_id, allocator)
// }

// // Remove client from streaming context
// remove_client :: proc(ctx: ^Streaming_Context, client_id: uintptr, allocator := context.allocator) {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	if client, exists := ctx.clients[client_id]; exists {
// 		// Remove from all subscriptions
// 		sync.rw_mutex_guard(&ctx.channels_mu)
// 		for event, &clients in ctx.broadcast_channels {
// 			for i := len(clients) - 1; i >= 0; i -= 1 {
// 				if clients[i] == client_id {
// 					ordered_remove(&clients, i)
// 					break
// 				}
// 			}
// 		}

// 		client_state_destroy(client, allocator)
// 		delete_key(&ctx.clients, client_id)
// 	}
// }

// // Subscribe client to an event
// subscribe_client :: proc(
// 	ctx: ^Streaming_Context,
// 	client_id: uintptr,
// 	event: string,
// 	allocator := context.allocator,
// ) -> bool {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	client, client_exists := ctx.clients[client_id]
// 	if !client_exists do return false

// 	event_copy := strings.clone(event, allocator)
// 	client.subscriptions[event_copy] = true

// 	sync.rw_mutex_guard(&ctx.channels_mu)
// 	if event not_in ctx.broadcast_channels {
// 		ctx.broadcast_channels[event_copy] = make([dynamic]uintptr, allocator)
// 	}

// 	// Check if already subscribed
// 	for existing_id in ctx.broadcast_channels[event_copy] {
// 		if existing_id == client_id do return true
// 	}

// 	append(&ctx.broadcast_channels[event_copy], client_id)
// 	return true
// }

// // Unsubscribe client from an event
// unsubscribe_client :: proc(
// 	ctx: ^Streaming_Context,
// 	client_id: uintptr,
// 	event: string,
// 	allocator := context.allocator,
// ) -> bool {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	client, client_exists := ctx.clients[client_id]
// 	if !client_exists do return false

// 	delete_key(&client.subscriptions, event)

// 	sync.rw_mutex_guard(&ctx.channels_mu)
// 	if clients, exists := &ctx.broadcast_channels[event]; exists {
// 		for i := len(clients) - 1; i >= 0; i -= 1 {
// 			if clients[i] == client_id {
// 				ordered_remove(clients, i)
// 				break
// 			}
// 		}
// 	}
// 	return true
// }

// // // Broadcast push message to all subscribers
// // broadcast_push :: proc(
// // 	ctx: ^Streaming_Context,
// // 	event: string,
// // 	data: Binary_Value,
// // 	allocator := context.allocator,
// // ) {
// // 	sync.rw_mutex_shared_guard(&ctx.channels_mu)
// // 	clients, exists := ctx.broadcast_channels[event]
// // 	if !exists do return

// // 	message_id := get_next_message_id(ctx)
// // 	push_msg, create_err := create_push(message_id, event, data, allocator)
// // 	if create_err != nil {
// // 		log.errorf("Failed to create push message: %v", create_err)
// // 		return
// // 	}
// // 	defer message_destroy(&push_msg, allocator)

// // 	sync.rw_mutex_shared_guard(&ctx.clients_mu)
// // 	for client_id in clients {
// // 		if client, client_exists := ctx.clients[client_id]; client_exists {
// // 			// Queue message for sending (could be sent immediately if socket is writable)
// // 			msg_copy := push_msg
// // 			msg_copy.payload = slice.clone(push_msg.payload, allocator)
// // 			append(&client.send_queue, msg_copy)
// // 		}
// // 	}
// // }

// // Process incoming message buffer
// process_message_buffer :: proc(
// 	socket: ^Streaming_Socket,
// 	ctx: ^Streaming_Context,
// 	data: [^]byte,
// 	length: c.int,
// 	allocator := context.allocator,
// ) {
// 	if length <= 0 do return

// 	// Append new data to buffer
// 	old_len := len(socket.msg_buffer.data)
// 	resize(&socket.msg_buffer.data, old_len + int(length))
// 	copy(socket.msg_buffer.data[old_len:], data[:length])

// 	// Process complete messages
// 	for {
// 		buf_len := len(socket.msg_buffer.data)

// 		// Read header if not done yet
// 		if !socket.msg_buffer.header_read {
// 			if buf_len < HEADER_SIZE do break // Need more data for header

// 			header, header_ok := deserialize_header(socket.msg_buffer.data[:HEADER_SIZE])
// 			if !header_ok {
// 				log.error("Invalid message header received")
// 				clear(&socket.msg_buffer.data)
// 				socket.msg_buffer.header_read = false
// 				break
// 			}

// 			validation_err := validate_header(header)
// 			if validation_err != .None {
// 				log.errorf("Header validation failed: %v", validation_err)
// 				clear(&socket.msg_buffer.data)
// 				socket.msg_buffer.header_read = false
// 				break
// 			}

// 			socket.msg_buffer.expected_len = header.payload_length
// 			socket.msg_buffer.header_read = true
// 		}

// 		// Check if we have complete message
// 		total_expected := HEADER_SIZE + int(socket.msg_buffer.expected_len)
// 		if buf_len < total_expected do break // Need more data

// 		// Extract complete message
// 		header, _ := deserialize_header(socket.msg_buffer.data[:HEADER_SIZE])
// 		payload := socket.msg_buffer.data[HEADER_SIZE:total_expected] if socket.msg_buffer.expected_len > 0 else nil

// 		message := Message{
// 			header = header,
// 			payload = slice.clone(payload, allocator),
// 		}

// 		// Process the message
// 		handle_message(socket.client_state.id, message, ctx, allocator)

// 		// Remove processed message from buffer
// 		remaining := socket.msg_buffer.data[total_expected:]
// 		if len(remaining) > 0 {
// 			new_data := make([dynamic]byte, len(remaining), allocator)
// 			copy(new_data[:], remaining)
// 			delete(socket.msg_buffer.data, allocator)
// 			socket.msg_buffer.data = new_data
// 		} else {
// 			clear(&socket.msg_buffer.data)
// 		}

// 		socket.msg_buffer.header_read = false
// 		socket.msg_buffer.expected_len = 0

// 		message_destroy(&message, allocator)
// 	}
// }

// // Handle a complete message
// handle_message :: proc(
// 	client_id: uintptr,
// 	message: Message,
// 	ctx: ^Streaming_Context,
// 	allocator := context.allocator,
// ) {
// 	msg_type := Message_Type(message.header.type)
// 	switch msg_type {
// 	case .Request:
// 		handle_request(client_id, message, ctx, allocator)
// 	case .Ping:
// 		handle_ping(client_id, message, ctx, allocator)
// 	case .Response, .Push, .Error, .Pong:
// 		log.warnf("Unexpected message type from client: %v", msg_type)
// 	}
// }

// // Handle API request
// handle_request :: proc(
// 	client_id: uintptr,
// 	message: Message,
// 	ctx: ^Streaming_Context,
// 	allocator := context.allocator,
// ) {
// 	request, parse_err := parse_request(message, allocator)
// 	if parse_err != nil {
// 		error_msg, _ := create_response(
// 			message.header.message_id,
// 			Null_Value{},
// 			parse_err.(Error_Code) or_else .Invalid_Message,
// 			"Failed to parse request",
// 			allocator,
// 		)
// 		queue_message_for_client(ctx, client_id, error_msg, allocator)
// 		return
// 	}

// 	// Look up handler
// 	sync.rw_mutex_shared_guard(&ctx.handlers_mu)
// 	handler, handler_exists := ctx.message_handlers[request.method]
// 	if !handler_exists {
// 		error_msg, _ := create_response(
// 			message.header.message_id,
// 			Null_Value{},
// 			.Method_Not_Found,
// 			fmt.aprintf("Method '%s' not found", request.method),
// 			allocator,
// 		)
// 		queue_message_for_client(ctx, client_id, error_msg, allocator)
// 		return
// 	}

// 	// Call handler
// 	result, error_code, error_message := handler(client_id, request, ctx)

// 	// Send response
// 	response_msg, create_err := create_response(
// 		message.header.message_id,
// 		result,
// 		error_code,
// 		error_message,
// 		allocator,
// 	)
// 	if create_err != nil {
// 		log.errorf("Failed to create response: %v", create_err)
// 		return
// 	}

// 	queue_message_for_client(ctx, client_id, response_msg, allocator)
// }

// // Handle ping message
// handle_ping :: proc(
// 	client_id: uintptr,
// 	message: Message,
// 	ctx: ^Streaming_Context,
// 	allocator := context.allocator,
// ) {
// 	pong_msg := create_pong(message.header.message_id)
// 	queue_message_for_client(ctx, client_id, pong_msg, allocator)

// 	// Update last ping time
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	if client, exists := ctx.clients[client_id]; exists {
// 		client.last_ping = u64(time.now()._nsec)
// 	}
// }

// // Queue message for a specific client
// queue_message_for_client :: proc(
// 	ctx: ^Streaming_Context,
// 	client_id: uintptr,
// 	message: Message,
// 	allocator := context.allocator,
// ) {
// 	sync.rw_mutex_guard(&ctx.clients_mu)
// 	if client, exists := ctx.clients[client_id]; exists {
// 		msg_copy := message
// 		msg_copy.payload = slice.clone(message.payload, allocator)
// 		append(&client.send_queue, msg_copy)
// 	}
// }
