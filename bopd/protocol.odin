package bopd

// import "base:runtime"
// import "core:encoding/endian"
// import "core:fmt"
// import "core:mem"
// import "core:slice"
// import "core:strings"

// Protocol constants
PROTOCOL_VERSION :: 1
MAX_PAYLOAD_SIZE :: 16 * 1024 * 1024 // 16MB max payload
HEADER_SIZE :: 12

// // Message types
// Message_Type :: enum u8 {
// 	Request  = 1,
// 	Response = 2,
// 	Push     = 3,
// 	Error    = 4,
// 	Ping     = 5,
// 	Pong     = 6,
// }

// // Data types for binary encoding
// Data_Type :: enum u8 {
// 	Null     = 0,
// 	Bool     = 1,
// 	I8       = 2,
// 	I16      = 3,
// 	I32      = 4,
// 	I64      = 5,
// 	U8       = 6,
// 	U16      = 7,
// 	U32      = 8,
// 	U64      = 9,
// 	F32      = 10,
// 	F64      = 11,
// 	String   = 12,
// 	Bytes    = 13,
// 	Bits     = 14,
// 	Matrix   = 15,

// 	// Coordinates
// 	Geopoint = 16,  // struct { latitude, longitude: f32 }
// 	Box2D    = 17,  // struct { min, max: [2]f32 } | matrix[f32, 2, 2]
// 	Box3D    = 18,  // struct { min, max: [3]f32 } | matrix[f32, 3, 3]

// 	Enum     = 19,
// 	Union    = 20,
// 	Record   = 21,

// 	Array    = 22,  // fixed size e.g. [16]i32
// 	List     = 23,  // dynamic size e.g. []i32
// 	Set      = 24,
// 	Map      = 25,

// 	Alias    = 26,

// 	// SIMD vectors
// 	I8x16    = 30,
// 	I8x32    = 31,
// 	I8x64    = 32,
// 	I16x8    = 33,
// 	I16x16   = 34,
// 	I16x32   = 35,
// 	I32x4    = 36,
// 	I32x8    = 37,
// 	I32x16   = 38,
// 	I64x2    = 39,
// 	I64x4    = 40,
// 	I64x8    = 41,
// 	F32x4    = 42,
// 	F32x8    = 43,
// 	F32x16   = 44,
// 	F64x2    = 45,
// 	F64x4    = 46,
// 	F64x8    = 47,
// }

// Data_Types :: bit_set[Data_Type; u64]

// Type :: struct {
//     type:     Data_Type,
//     flags:    u8,
//     reserved: u16le,
//     size:     u32le,
//     info:     Type_Info,
// }

// Null :: struct{}

// Number :: struct {
//     bits: u16le,
// }

// Type_Info :: union {
//     Null,
//     Enum,
//     Union,
//     Record,
//     SIMD,
// }

// Type_I8 :: Type { type = .I8, flags = 0, reserved = 0, size = 1 }
// Type_U8 :: Type { type = .U8, flags = 0, reserved = 0, size = 1 }

// Matrix :: struct {
//     N:    u32le,
//     D:    u32le,
//     type: Data_Type,
//     _:    u8,
// }

// SIMD :: struct {
//     type: Data_Type,
//     _:    u8,
//     n:    u16le,
// }

// Enum_Option :: struct {
// 	name:  string,
// 	value: u128,
// }

// Enum :: struct {
// 	name: string,
// 	type: Data_Type,
// }

// Union_Option :: struct {
// 	tag: i32le,
// }

// Union :: struct {
// 	name:    string,
// 	options: []Union_Option,
// }

// Field :: struct {
// 	name:   string,
// 	sort:   u16le,
// 	offset: u16le,
// 	align:  u16le,
// 	_:      u16le,
// 	type:   Type,
// }

// Record :: struct {
// 	name:   string,
// 	fields: []Field,
// }

// Order_Record :: Record {
//     name = "Order",
//     fields = {
//         { name = "id", sort = 0, offset = 0, type = Type { type = .U64, size = 8} },
//         { name = "created", sort = 1, offset = 8, type = Type { type = .U64, size = 8} },
//     },
// }

// Record_Header :: struct {
//     base_size:    u32le,
//     version:      u32le, // version checksum
//     base_version: u32le,
//     depth:        u16le,
//     reserved:     u16le,
// }

// // Error codes
// Error_Code :: enum u32 {
// 	None                = 0,
// 	Invalid_Message     = 1,
// 	Message_Too_Large   = 2,
// 	Invalid_Data        = 3,
// 	Method_Not_Found    = 4,
// 	Internal_Error      = 5,
// 	Unauthorized        = 6,
// 	Rate_Limited        = 7,
// 	Connection_Closed   = 8,
// 	Buffer_Overflow     = 9,
// 	Decode_Error        = 10,
// }

// // Binary message header (12 bytes total)
// Message_Header :: struct {
// 	version:        u8,    // 1 byte - protocol version
// 	type:           u8,    // 1 byte - message type
// 	flags:          u8,    // 1 byte - flags (reserved)
// 	reserved:       u8,    // 1 byte - reserved
// 	message_id:     u32le, // 4 bytes - message ID
// 	payload_length: u32le, // 4 bytes - payload length
// }

// Null_Value :: struct {}

// // Complete message structure
// Message :: struct {
// 	header:    Message_Header,
// 	payload:   []byte,
// 	allocator: runtime.Allocator,
// }

// // Request structure (binary encoded)
// Request :: struct {
// 	method: u32,
// }

// // Response structure (binary encoded)
// Response :: struct {
// 	result:        []byte,
// 	error:         Error_Code,
// 	error_message: string,
// }

// // Push message structure (binary encoded)
// Push :: struct {
// 	event: string,
// 	data:  []byte,
// }

// // Protocol error
// Protocol_Error :: union #shared_nil {
// 	runtime.Allocator_Error,
// 	Error_Code,
// }

// // Buffer for reading/writing binary data
// Binary_Buffer :: struct {
// 	data: []byte,
// 	ridx: int,
// 	offset: int,
// 	allocator: runtime.Allocator,
// }

// // buffer_extend :: proc(buf: ^Binary_Buffer, num_bytes: int) -> (index: int, err: runtime.Allocator_Error) {
// //     index = len(buf.data)
// //     if len(buf.data) + num_bytes <= cap(buf.data) {
// //         buf.data.len += num_bytes
// //         return
// //     }

// //     runtime.mem_copy()
// //     runtime.__dynamic_array_append_nothing()
// //     runtime.append_nothing(buf.data)

// //     return
// // }

// // Initialize buffer for writing
// buffer_init_write :: proc(capacity: int, allocator := context.allocator) -> Binary_Buffer {
// 	return Binary_Buffer{
// 		data   = make([]byte, capacity, allocator),
// 		offset = 0,
// 	}
// }

// // Initialize buffer for reading
// buffer_init_read :: proc(data: []byte) -> Binary_Buffer {
// 	return Binary_Buffer{
// 		data   = data,
// 		offset = 0,
// 	}
// }

// // Get remaining space in buffer
// buffer_remaining :: proc(buf: ^Binary_Buffer) -> int {
// 	return len(buf.data) - buf.offset
// }

// // Check if we can read n bytes
// buffer_can_read :: proc(buf: ^Binary_Buffer, n: int) -> bool {
// 	return buf.offset + n <= len(buf.data)
// }

// // Write bytes to buffer
// buffer_write_bytes :: proc(buf: ^Binary_Buffer, data: []byte) -> bool {
// 	if buffer_remaining(buf) < len(data) do return false
// 	copy(buf.data[buf.offset:], data)
// 	buf.offset += len(data)
// 	return true
// }

// // Read bytes from buffer
// buffer_read_bytes :: proc(buf: ^Binary_Buffer, n: int) -> ([]byte, bool) {
// 	if !buffer_can_read(buf, n) do return nil, false
// 	result := buf.data[buf.offset:buf.offset + n]
// 	buf.offset += n
// 	return result, true
// }

// // Write u8
// buffer_write_u8 :: proc(buf: ^Binary_Buffer, value: u8) -> bool {
// 	if buffer_remaining(buf) < 1 do return false
// 	buf.data[buf.offset] = value
// 	buf.offset += 1
// 	return true
// }

// // Read u8
// buffer_read_u8 :: proc(buf: ^Binary_Buffer) -> (u8, bool) {
// 	if !buffer_can_read(buf, 1) do return 0, false
// 	value := buf.data[buf.offset]
// 	buf.offset += 1
// 	return value, true
// }

// // Write u16 (little endian)
// buffer_write_u16 :: proc(buf: ^Binary_Buffer, value: u16) -> bool {
// 	if buffer_remaining(buf) < 2 do return false
// 	endian.put_u16(buf.data[buf.offset:], .Little, value)
// 	buf.offset += 2
// 	return true
// }
// buffer_write_u16be :: proc(buf: ^Binary_Buffer, value: u16be) -> bool {
// 	if buffer_remaining(buf) < 2 do return false
// 	endian.put_u16(buf.data[buf.offset:], .Big, value)
// 	buf.offset += 2
// 	return true
// }

// // Read u16 (little endian)
// buffer_read_u16 :: proc(buf: ^Binary_Buffer) -> (u16, bool) {
// 	if !buffer_can_read(buf, 2) do return 0, false
// 	value := endian.get_u16(buf.data[buf.offset:], .Little)
// 	buf.offset += 2
// 	return value, true
// }

// buffer_read_u16le :: proc(buf: ^Binary_Buffer) -> (u16, bool) {
// 	if !buffer_can_read(buf, 2) do return 0, false
// 	value := u16le(endian.get_u16(buf.data[buf.offset:], .Little))
// 	buf.offset += 2
// 	return value, true
// }

// buffer_read_u16be :: proc(buf: ^Binary_Buffer) -> (u16, bool) {
// 	if !buffer_can_read(buf, 2) do return 0, false
// 	value := u16le(endian.get_u16(buf.data[buf.offset:], .Little))
// 	buf.offset += 2
// 	return value, true
// }

// buffer_write_u32 :: proc{
//     buffer_write_u32_le,
//     buffer_write_u32_be,
//     buffer_write_u32_ne,
// }

// buffer_write_u32_ne :: proc(buf: ^Binary_Buffer, value: u32) -> bool {
//     return buffer_write_u32_le(buf, u32le(value))
// }

// buffer_write_u32_le :: proc(buf: ^Binary_Buffer, value: u32le) -> bool {
// 	if buffer_remaining(buf) < 4 do return false
// 	endian.put_u32(buf.data[buf.offset:], .Little, u32(value))
// 	buf.offset += 4
// 	return true
// }

// buffer_write_u32_be :: proc(buf: ^Binary_Buffer, value: u32be) -> bool {
// 	if buffer_remaining(buf) < 4 do return false
// 	endian.put_u32(buf.data[buf.offset:], .Little, u32(value))
// 	buf.offset += 4
// 	return true
// }

// // Read u32 (little endian)
// buffer_read_u32 :: proc(buf: ^Binary_Buffer) -> (u32, bool) {
// 	if !buffer_can_read(buf, 4) do return 0, false
// 	value := endian.get_u32(buf.data[buf.offset:], .Little)
// 	buf.offset += 4
// 	return value, true
// }

// // Write u64 (little endian)
// buffer_write_u64 :: proc(buf: ^Binary_Buffer, value: u64) -> bool {
// 	if buffer_remaining(buf) < 8 do return false
// 	endian.unchecked_put_u64le(buf.data[buf.offset:], value)
// 	buf.offset += 8
// 	return true
// }

// // Read u64 (little endian)
// buffer_read_u64 :: proc(buf: ^Binary_Buffer) -> (u64, bool) {
// 	if !buffer_can_read(buf, 8) do return 0, false
// 	value := endian.get_u64(buf.data[buf.offset:], .Little)
// 	buf.offset += 8
// 	return value, true
// }

// // Write f32 (little endian)
// buffer_write_f32 :: proc(buf: ^Binary_Buffer, value: f32) -> bool {
// 	return buffer_write_u32(buf, transmute(u32)value)
// }

// // Read f32 (little endian)
// buffer_read_f32 :: proc(buf: ^Binary_Buffer) -> (f32, bool) {
// 	bits, ok := buffer_read_u32(buf)
// 	return transmute(f32)bits, ok
// }

// // Write f64 (little endian)
// buffer_write_f64 :: proc(buf: ^Binary_Buffer, value: f64) -> bool {
// 	return buffer_write_u64(buf, transmute(u64)value)
// }

// // Read f64 (little endian)
// buffer_read_f64 :: proc(buf: ^Binary_Buffer) -> (f64, bool) {
// 	bits, ok := buffer_read_u64(buf)
// 	return transmute(f64)bits, ok
// }

// // Write length-prefixed string
// buffer_write_string :: proc(buf: ^Binary_Buffer, str: string) -> bool {
// 	if !buffer_write_u32(buf, u32(len(str))) do return false
// 	return buffer_write_bytes(buf, transmute([]byte)str)
// }

// // Read length-prefixed string
// buffer_read_string :: proc(buf: ^Binary_Buffer, allocator := context.allocator) -> (string, bool) {
// 	length, length_ok := buffer_read_u32(buf)
// 	if !length_ok do return "", false

// 	if length == 0 do return "", true

// 	data, data_ok := buffer_read_bytes(buf, int(length))
// 	if !data_ok do return "", false

// 	return strings.clone_from_bytes(data, allocator), true
// }

// // Write length-prefixed byte array
// buffer_write_byte_array :: proc(buf: ^Binary_Buffer, data: []byte) -> bool {
// 	if !buffer_write_u32(buf, u32(len(data))) do return false
// 	return buffer_write_bytes(buf, data)
// }

// // Read length-prefixed byte array
// buffer_read_byte_array :: proc(buf: ^Binary_Buffer, allocator := context.allocator) -> ([]byte, bool) {
// 	length, length_ok := buffer_read_u32(buf)
// 	if !length_ok do return nil, false

// 	if length == 0 do return nil, true

// 	data, data_ok := buffer_read_bytes(buf, int(length))
// 	if !data_ok do return nil, false

// 	return slice.clone(data, allocator), true
// }

// // Serialize message header
// serialize_header :: proc(header: Message_Header) -> [HEADER_SIZE]byte {
// 	result: [HEADER_SIZE]byte
// 	buf := buffer_init_read(result[:])

// 	buffer_write_u8(&buf, header.version)
// 	buffer_write_u8(&buf, header.type)
// 	buffer_write_u8(&buf, header.flags)
// 	buffer_write_u8(&buf, header.reserved)
// 	// buffer_write_u32(&buf, header.message_id)
// 	// buffer_write_u32(&buf, header.payload_length)

// 	return result
// }

// // Deserialize message header
// deserialize_header :: proc(data: []byte) -> (Message_Header, bool) {
// 	if len(data) < HEADER_SIZE do return {}, false

// 	buf := buffer_init_read(data[:HEADER_SIZE])
// 	header: Message_Header

// 	version, version_ok := buffer_read_u8(&buf)
// 	if !version_ok do return {}, false
// 	header.version = version

// 	type_byte, type_ok := buffer_read_u8(&buf)
// 	if !type_ok do return {}, false
// 	header.type = type_byte

// 	flags, flags_ok := buffer_read_u8(&buf)
// 	if !flags_ok do return {}, false
// 	header.flags = flags

// 	reserved, reserved_ok := buffer_read_u8(&buf)
// 	if !reserved_ok do return {}, false
// 	header.reserved = reserved

// 	message_id, id_ok := buffer_read_u32(&buf)
// 	if !id_ok do return {}, false
// 	// header.message_id = message_id

// 	payload_length, len_ok := buffer_read_u32(&buf)
// 	if !len_ok do return {}, false
// 	// header.payload_length = payload_length

// 	return header, true
// }

// // // Create request message
// // create_request :: proc(
// // 	message_id: u32,
// // 	method: string,
// // 	allocator := context.allocator,
// // ) -> (Message, Protocol_Error) {
// // 	request := Request{
// // 		method = method,
// // 		params = params,
// // 	}

// // 	// Estimate payload size
// // 	payload_buf := buffer_init_write(1024, allocator)
// // 	defer delete(payload_buf.data, allocator)

// // 	// Encode request
// // 	if !buffer_write_string(&payload_buf, request.method) do return {}, Error_Code.Buffer_Overflow
// // 	if !encode_binary_value(&payload_buf, request.params, allocator) do return {}, Error_Code.Encode_Error

// // 	// Create final payload
// // 	payload := make([]byte, payload_buf.offset, allocator)
// // 	copy(payload, payload_buf.data[:payload_buf.offset])

// // 	if len(payload) > MAX_PAYLOAD_SIZE {
// // 		delete(payload, allocator)
// // 		return {}, Error_Code.Message_Too_Large
// // 	}

// // 	header := Message_Header{
// // 		version = PROTOCOL_VERSION,
// // 		type = u8(Message_Type.Request),
// // 		flags = 0,
// // 		reserved = 0,
// // 		message_id = message_id,
// // 		payload_length = u32(len(payload)),
// // 	}

// // 	return Message{header = header, payload = payload}, nil
// // }

// // // Create response message
// // create_response :: proc(
// // 	message_id: u32,
// // 	result: Binary_Value = Null_Value{},
// // 	error_code: Error_Code = .None,
// // 	error_message: string = "",
// // 	allocator := context.allocator,
// // ) -> (Message, Protocol_Error) {
// // 	response := Response{
// // 		result = result,
// // 		error = error_code,
// // 		error_message = error_message,
// // 	}

// // 	// Estimate payload size
// // 	payload_buf := buffer_init_write(1024, allocator)
// // 	defer delete(payload_buf.data, allocator)

// // 	// Encode response
// // 	if !encode_binary_value(&payload_buf, response.result, allocator) do return {}, Error_Code.Encode_Error
// // 	if !buffer_write_u32(&payload_buf, u32(response.error)) do return {}, Error_Code.Buffer_Overflow
// // 	if !buffer_write_string(&payload_buf, response.error_message) do return {}, Error_Code.Buffer_Overflow

// // 	// Create final payload
// // 	payload := make([]byte, payload_buf.offset, allocator)
// // 	copy(payload, payload_buf.data[:payload_buf.offset])

// // 	if len(payload) > MAX_PAYLOAD_SIZE {
// // 		delete(payload, allocator)
// // 		return {}, Error_Code.Message_Too_Large
// // 	}

// // 	header := Message_Header{
// // 		version = PROTOCOL_VERSION,
// // 		type = u8(Message_Type.Response),
// // 		flags = 0,
// // 		reserved = 0,
// // 		message_id = message_id,
// // 		payload_length = u32(len(payload)),
// // 	}

// // 	return Message{header = header, payload = payload}, nil
// // }

// // // Create push message
// // create_push :: proc(
// // 	message_id: u32,
// // 	event: string,
// // 	data: Binary_Value = Null_Value{},
// // 	allocator := context.allocator,
// // ) -> (Message, Protocol_Error) {
// // 	push := Push{
// // 		event = event,
// // 		data = data,
// // 	}

// // 	// Estimate payload size
// // 	payload_buf := buffer_init_write(1024, allocator)
// // 	defer delete(payload_buf.data)

// // 	// Encode push
// // 	if !buffer_write_string(&payload_buf, push.event) do return {}, Error_Code.Buffer_Overflow
// // 	if !encode_binary_value(&payload_buf, push.data, allocator) do return {}, Error_Code.Encode_Error

// // 	// Create final payload
// // 	payload := make([]byte, payload_buf.offset, allocator)
// // 	copy(payload, payload_buf.data[:payload_buf.offset])

// // 	if len(payload) > MAX_PAYLOAD_SIZE {
// // 		delete(payload, allocator)
// // 		return {}, Error_Code.Message_Too_Large
// // 	}

// // 	header := Message_Header{
// // 		version = PROTOCOL_VERSION,
// // 		type = u8(Message_Type.Push),
// // 		flags = 0,
// // 		reserved = 0,
// // 		message_id = message_id,
// // 		payload_length = u32(len(payload)),
// // 	}

// // 	return Message{header = header, payload = payload}, nil
// // }

// // // Create ping message
// // create_ping :: proc(message_id: u32) -> Message {
// // 	header := Message_Header{
// // 		version = PROTOCOL_VERSION,
// // 		type = u8(Message_Type.Ping),
// // 		flags = 0,
// // 		reserved = 0,
// // 		message_id = message_id,
// // 		payload_length = 0,
// // 	}

// // 	return Message{header = header, payload = nil}
// // }

// // // Create pong message
// // create_pong :: proc(message_id: u32le) -> Message {
// // 	header := Message_Header{
// // 		version = PROTOCOL_VERSION,
// // 		type = u8(Message_Type.Pong),
// // 		flags = 0,
// // 		reserved = 0,
// // 		message_id = message_id,
// // 		payload_length = 0,
// // 	}

// // 	return Message{header = header, payload = nil}
// // }

// // // Parse request message
// // parse_request :: proc(msg: Message, allocator := context.allocator) -> (Request, Protocol_Error) {
// // 	if Message_Type(msg.header.type) != .Request do return {}, Error_Code.Invalid_Message

// // 	buf := buffer_init_read(msg.payload)

// // 	method, method_ok := buffer_read_string(&buf, allocator)
// // 	if !method_ok do return {}, Error_Code.Decode_Error

// // 	params, params_ok := decode_binary_value(&buf, allocator)
// // 	if !params_ok do return {}, Error_Code.Decode_Error

// // 	return Request{method = method, params = params}, nil
// // }

// // // Parse response message
// // parse_response :: proc(msg: Message, allocator := context.allocator) -> (Response, Protocol_Error) {
// // 	if Message_Type(msg.header.type) != .Response do return {}, Error_Code.Invalid_Message

// // 	buf := buffer_init_read(msg.payload)

// // 	result, result_ok := decode_binary_value(&buf, allocator)
// // 	if !result_ok do return {}, Error_Code.Decode_Error

// // 	error_code, error_ok := buffer_read_u32(&buf)
// // 	if !error_ok do return {}, Error_Code.Decode_Error

// // 	error_message, message_ok := buffer_read_string(&buf, allocator)
// // 	if !message_ok do return {}, Error_Code.Decode_Error

// // 	return Response{
// // 		result = result,
// // 		error = Error_Code(error_code),
// // 		error_message = error_message,
// // 	}, nil
// // }

// // // Parse push message
// // parse_push :: proc(msg: Message, allocator := context.allocator) -> (Push, Protocol_Error) {
// // 	if Message_Type(msg.header.type) != .Push do return {}, Error_Code.Invalid_Message

// // 	buf := buffer_init_read(msg.payload)

// // 	event, event_ok := buffer_read_string(&buf, allocator)
// // 	if !event_ok do return {}, Error_Code.Decode_Error

// // 	data, data_ok := decode_binary_value(&buf, allocator)
// // 	if !data_ok do return {}, Error_Code.Decode_Error

// // 	return Push{event = event, data = data}, nil
// // }

// // Serialize complete message
// serialize_message :: proc(msg: Message, allocator := context.allocator) -> (data: []byte, err: Protocol_Error) {
// 	header_bytes := serialize_header(msg.header)
// 	total_size := HEADER_SIZE + len(msg.payload)

// 	result := make([]byte, total_size, allocator) or_return

// 	copy(result[0:HEADER_SIZE], header_bytes[:])
// 	if len(msg.payload) > 0 {
// 		copy(result[HEADER_SIZE:], msg.payload)
// 	}

// 	return result, nil
// }

// // Clean up message resources
// message_destroy :: proc(msg: ^Message, allocator := context.allocator) {
// 	if msg.payload != nil {
// 		delete(msg.payload, allocator)
// 		msg.payload = nil
// 	}
// }

// // Validate message header
// validate_header :: proc(header: Message_Header) -> Error_Code {
// 	if header.version != PROTOCOL_VERSION do return .Invalid_Message

// 	msg_type := Message_Type(header.type)
// 	switch msg_type {
// 	case .Request, .Response, .Push, .Error, .Ping, .Pong:
// 		// Valid types
// 	case:
// 		return .Invalid_Message
// 	}

// 	if header.payload_length > MAX_PAYLOAD_SIZE do return .Message_Too_Large

// 	return .None
// }

// // Check if message type requires payload
// requires_payload :: proc(msg_type: Message_Type) -> bool {
// 	switch msg_type {
// 	case .Request, .Response, .Push, .Error:
// 		return true
// 	case .Ping, .Pong:
// 		return false
// 	}
// 	return false
// }
