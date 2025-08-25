# BOP Binary Streaming Server

A high-performance binary streaming server with real-time messaging capabilities, built in Odin using uSockets.

## Features

- **Fully Binary Protocol**: Efficient binary serialization with zero-copy design
- **Real-time Messaging**: Request/Response and Push notification patterns
- **Event Subscriptions**: Client-side subscription management for push events
- **High Performance**: Built on uSockets for maximum throughput
- **Multi-client Support**: Concurrent client connections with independent state
- **SSL/TLS Support**: Optional encrypted connections
- **Ping/Pong Keepalive**: Built-in connection health monitoring

## Binary Protocol Specification

### Message Format

Each message consists of a 12-byte header followed by a variable-length payload:

```
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
| Version| Type   | Flags  | Resrvd | Message ID (4 bytes, LE)      | Payload Length (4 bytes, LE)  |
+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+--------+
| Payload Data (variable length)                                                                           |
+-------------------------------------------------------------------------------------------------------+
```

### Header Fields

- **Version** (1 byte): Protocol version (currently 1)
- **Type** (1 byte): Message type
  - `0x01`: Request (client → server)
  - `0x02`: Response (server → client)
  - `0x03`: Push (server → client)
  - `0x04`: Error (server → client)
  - `0x05`: Ping (bidirectional)
  - `0x06`: Pong (bidirectional)
- **Flags** (1 byte): Reserved for future use
- **Reserved** (1 byte): Reserved for future use
- **Message ID** (4 bytes): Unique identifier for request/response correlation
- **Payload Length** (4 bytes): Length of payload in bytes

### Data Types

Binary values are encoded with type prefixes for efficient parsing:

| Type Code | Type Name | Encoding |
|-----------|-----------|----------|
| `0x00` | Null | No data |
| `0x01` | Boolean | 1 byte (0=false, 1=true) |
| `0x02` | I8 | 1 byte signed integer |
| `0x03` | I16 | 2 bytes signed integer (LE) |
| `0x04` | I32 | 4 bytes signed integer (LE) |
| `0x05` | I64 | 8 bytes signed integer (LE) |
| `0x06` | U8 | 1 byte unsigned integer |
| `0x07` | U16 | 2 bytes unsigned integer (LE) |
| `0x08` | U32 | 4 bytes unsigned integer (LE) |
| `0x09` | U64 | 8 bytes unsigned integer (LE) |
| `0x0A` | F32 | 4 bytes IEEE 754 float (LE) |
| `0x0B` | F64 | 8 bytes IEEE 754 double (LE) |
| `0x0C` | String | Length-prefixed UTF-8 string |
| `0x0D` | Bytes | Length-prefixed byte array |
| `0x0E` | Array | Count-prefixed array of values |
| `0x0F` | Map | Count-prefixed key-value pairs |

### Message Types

#### Request Message
```
[Header with Type=0x01]
[Method Name: String]
[Parameters: Binary Value]
```

#### Response Message
```
[Header with Type=0x02]
[Result: Binary Value]
[Error Code: U32]
[Error Message: String]
```

#### Push Message
```
[Header with Type=0x03]
[Event Name: String]
[Data: Binary Value]
```

## Usage

### Server Mode

Start the streaming server:

```bash
bopd server --port 8080 --verbose
```

Options:
- `--host <addr>`: Server host address (default: 0.0.0.0)
- `--port <port>`: Server port number (default: 8080)
- `--verbose`: Enable verbose logging
- `--ssl`: Enable SSL/TLS
- `--cert <file>`: SSL certificate file
- `--key <file>`: SSL private key file

### Client Mode

Run the test client:

```bash
bopd client --host 127.0.0.1 --port 8080
```

### Performance Testing

Run performance tests:

```bash
bopd perf --host localhost --port 8080
```

## Built-in API Methods

### Core Methods

- **`subscribe`**: Subscribe to push events
  - Parameters: `{"event": "event_name"}`
  - Returns: `{"subscribed": true, "event": "event_name"}`

- **`unsubscribe`**: Unsubscribe from push events
  - Parameters: `{"event": "event_name"}`
  - Returns: `{"unsubscribed": true, "event": "event_name"}`

### Example Methods

- **`echo`**: Returns the same data that was sent
  - Parameters: Any binary value
  - Returns: Same binary value

- **`get_time`**: Returns current server time
  - Parameters: None
  - Returns: `{"timestamp": i64, "unix_seconds": f64, "iso8601": string, ...}`

- **`get_info`**: Returns server information
  - Parameters: None
  - Returns: Server stats and configuration

- **`math.add`**: Adds two numbers
  - Parameters: `{"a": number, "b": number}`
  - Returns: `{"sum": number, "a": number, "b": number, "operation": "addition"}`

- **`math.multiply`**: Multiplies two numbers
  - Parameters: `{"a": number, "b": number}`
  - Returns: `{"product": number, "a": number, "b": number, "operation": "multiplication"}`

- **`broadcast`**: Send push message to all subscribers
  - Parameters: `{"event": "event_name", "data": any_value}`
  - Returns: `{"broadcasted": true, "event": "event_name", "timestamp": i64}`

## Push Events

The server automatically broadcasts these events:

- **`heartbeat`**: Sent every 5 seconds
  ```json
  {
    "counter": 123,
    "timestamp": 1234567890,
    "message": "Server heartbeat",
    "type": "heartbeat"
  }
  ```

- **`random`**: Sent every 50 seconds
  ```json
  {
    "type": "random_event",
    "value": 42,
    "description": "This is a periodic random event",
    "counter": 10,
    "timestamp": 1234567890,
    "binary_data": [bytes]
  }
  ```

- **`status`**: Sent every 100 seconds
  ```json
  {
    "type": "status_update",
    "connected_clients": 5,
    "uptime_heartbeats": 200,
    "timestamp": 1234567890
  }
  ```

## Building

### Prerequisites

- Odin compiler
- C compiler (for uSockets)
- uSockets library

### Build Commands

Build the server:
```bash
odin build . -out:bopd
```

Or use the build script:
```bash
odin run build.odin -- all
```

Build options:
- `server`: Build only the server
- `client`: Build only the test client
- `all`: Build both (default)
- `clean`: Clean build artifacts

## Protocol Benefits

### Efficiency
- **Compact encoding**: Much smaller than JSON or XML
- **Zero-copy parsing**: Direct memory access without string processing
- **Type safety**: Explicit type encoding prevents parsing errors
- **Fast serialization**: Binary format optimized for speed

### Extensibility
- **Reserved fields**: Future protocol extensions without breaking changes
- **Flexible data types**: Support for all common data types including binary data
- **Custom message types**: Easy to add new message patterns

### Performance
- **uSockets backend**: High-performance event loop
- **Connection pooling**: Efficient multi-client management
- **Minimal overhead**: 12-byte headers with no parsing ambiguity

## Error Handling

All errors are returned as Response messages with:
- `result`: null
- `error`: Error code (non-zero)
- `error_message`: Human-readable error description

Common error codes:
- `1`: Invalid message format
- `2`: Message too large
- `3`: Invalid data encoding
- `4`: Method not found
- `5`: Internal server error

## Example Client Implementation

```odin
// Connect to server
client: Binary_Test_Client
client_init(&client)
defer client_destroy(&client)

if !client_connect(&client, "127.0.0.1", 8080) {
    log.error("Failed to connect")
    return
}

// Subscribe to events
subscribe_map := make_binary_map()
subscribe_map["event"] = to_binary("heartbeat")
client_send_request(&client, "subscribe", subscribe_map)

// Send echo request
echo_map := make_binary_map()
echo_map["message"] = to_binary("Hello, server!")
echo_map["timestamp"] = to_binary(i64(time.to_unix_nanoseconds(time.now())))
client_send_request(&client, "echo", echo_map)

// Receive messages
for {
    client_receive_messages(&client)
    time.sleep(100 * time.Millisecond)
}
```

## License

This project is part of the BOP (Binary Operations Protocol) suite.