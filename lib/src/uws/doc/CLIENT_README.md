# Client Application Functionality

This document describes the client functionality that has been implemented in the dedicated `ClientApp.h` file, providing comprehensive WebSocket and HTTP client capabilities for uWebSockets applications.

## Overview

The client functionality is implemented in a dedicated `TemplatedClientApp` class that provides comprehensive client capabilities. This maintains the same design patterns and API consistency as the server-side functionality while keeping client and server code completely separated.

## Key Features

- **WebSocket Client**: Connect to WebSocket servers using URLs (`ws://` or `wss://`)
- **HTTP Client**: Make HTTP requests (GET, POST, PUT, DELETE, PATCH)
- **Flexible connection options**: Support for protocols, custom paths, and SSL
- **Consistent API**: Uses the same behavior pattern as server WebSockets
- **Error handling**: Built-in connection error callbacks
- **Connection management**: Methods to check and manage client connections

## Usage

### Basic Client Connection

```cpp
#include "App.h"

int main() {
    // Define user data for the connection
    struct ClientData {
        std::string name;
        int messageCount = 0;
    };

    // Create client app instance
    uWS::ClientApp app;

    // Set up client behavior
    uWS::WebSocketClientBehavior<ClientData> behavior = {
        .open = [](auto *ws) {
            std::cout << "Connected!" << std::endl;
            ws->send("Hello server!", uWS::TEXT);
        },
        .message = [](auto *ws, std::string_view message, uWS::OpCode opCode) {
            std::cout << "Received: " << message << std::endl;
        },
        .close = [](auto *ws, int code, std::string_view message) {
            std::cout << "Disconnected with code: " << code << std::endl;
        },
        .connectError = [](auto *ws, int code) {
            std::cout << "Connection failed: " << code << std::endl;
        }
    };

    // Connect to server
    app.connectWS<ClientData>("ws://localhost:3000", std::move(behavior));
    
    // Run the event loop
    app.run();
    
    return 0;
}
```

### WebSocket Connection Methods

#### URL-based Connection
```cpp
app.connectWS<ClientData>("ws://localhost:3000", behavior);
app.connectWS<ClientData>("wss://secure-server.com:443", behavior);
```

#### Connection with Protocol
```cpp
app.connectWS<ClientData>("ws://localhost:3000", "my-protocol", behavior);
```

#### Separate Host/Port/Path
```cpp
app.connectWS<ClientData>("localhost", 3000, "/websocket", behavior);
```

#### SSL Connections
```cpp
uWS::SSLClientApp sslApp;
sslApp.connectWS<ClientData>("wss://localhost:3001", behavior);
```

### HTTP Client Methods

#### Basic HTTP Requests
```cpp
// GET request
app.get("http://api.example.com/data", behavior);

// POST request with body
app.post("http://api.example.com/create", "{\"name\":\"test\"}", behavior);

// PUT request
app.put("http://api.example.com/update/123", "{\"name\":\"updated\"}", behavior);

// DELETE request
app.del("http://api.example.com/delete/123", behavior);

// PATCH request
app.patch("http://api.example.com/patch/123", "{\"status\":\"active\"}", behavior);
```

#### HTTP Client Behavior Configuration
```cpp
uWS::HttpClientBehavior behavior = {
    // Timeout settings
    .connectTimeout = 5000,    // 5 seconds
    .readTimeout = 30000,      // 30 seconds
    .writeTimeout = 10000,     // 10 seconds
    
    // Request settings
    .followRedirects = true,
    .maxRedirects = 5,
    .keepAlive = true,
    
    // Streaming settings
    .maxChunkSize = 64 * 1024,  // 64KB chunks
    .enableStreaming = true,     // Enable streaming for large responses
    
    // Response handlers
    .success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
        std::cout << "Response: " << statusCode << std::endl;
        std::cout << "Response Headers: " << responseHeaders << std::endl;
        if (isComplete) {
            std::cout << "Complete Body: " << body << std::endl;
        } else {
            std::cout << "Streaming response (body via dataReceived)" << std::endl;
        }
    },
    .error = [](int errorCode, std::string_view errorMessage) {
        std::cout << "Error: " << errorCode << " - " << errorMessage << std::endl;
    },
    .headersReceived = [](std::string_view headers) {
        std::cout << "Headers: " << headers << std::endl;
    },
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        std::cout << "Chunk: " << chunk.length() << " bytes";
        if (isLastChunk) {
            std::cout << " (final chunk)";
        }
        std::cout << std::endl;
    }
};
```

#### Advanced HTTP Requests
```cpp
// Custom HTTP request with full control
uWS::HttpClientRequest request = {
    .method = "POST",
    .url = "http://api.example.com/custom",
    .body = "{\"custom\":\"data\"}",
    .headers = {
        {"Content-Type", "application/json"},
        {"Authorization", "Bearer token123"},
        {"X-Custom-Header", "value"}
    },
    .behavior = behavior
};

app.request(std::move(request));
```

### Client Behavior Configuration

The `WebSocketClientBehavior` structure supports the same configuration options as server WebSockets:

```cpp
uWS::WebSocketClientBehavior<ClientData> behavior = {
    // Settings
    .compression = uWS::DISABLED,
    .maxPayloadLength = 16 * 1024,
    .idleTimeout = 120,
    .maxBackpressure = 64 * 1024,
    .closeOnBackpressureLimit = false,
    .resetIdleTimeoutOnSend = true,
    .sendPingsAutomatically = true,
    .maxLifetime = 0,
    
    // Event handlers
    .open = [](auto *ws) { /* connection opened */ },
    .message = [](auto *ws, std::string_view message, uWS::OpCode opCode) { /* message received */ },
    .close = [](auto *ws, int code, std::string_view message) { /* connection closed */ },
    .connectError = [](auto *ws, int code) { /* connection failed */ },
    .ping = [](auto *ws, std::string_view message) { /* ping received */ },
    .pong = [](auto *ws, std::string_view message) { /* pong received */ },
    .drain = [](auto *ws) { /* backpressure cleared */ },
    .dropped = [](auto *ws, std::string_view message, uWS::OpCode opCode) { /* message dropped */ }
};
```

### Connection Management

```cpp
// Check if any connections are active
if (app.hasActiveConnections()) {
    std::cout << "Active connections: " << app.getConnectionCount() << std::endl;
}

// Close all connections
app.closeAll();

// Close all connections (including servers)
app.close();
```

## Implementation Details

### URL Parsing

The client connection methods automatically parse WebSocket URLs:
- `ws://host:port/path` → non-SSL connection
- `wss://host:port/path` → SSL connection
- Default ports: 80 for ws://, 443 for wss://

### Streaming Support

The HTTP client supports streaming responses for both chunked encoding and large content-length responses:

#### Chunked Encoding
- Automatically detects `Transfer-Encoding: chunked` headers
- Parses chunk size headers and data chunks
- Emits data via `dataReceived` callback with `isLastChunk` flag

#### Content-Length Streaming
- For responses larger than `maxChunkSize` (default 64KB)
- Automatically enables streaming mode
- Emits data in chunks as it arrives

#### Streaming Configuration
```cpp
uWS::HttpClientBehavior behavior = {
    // Streaming settings
    .maxChunkSize = 64 * 1024,  // 64KB default
    .enableStreaming = true,     // Enable streaming
    
    // Streaming callbacks
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Process chunk data
        if (isLastChunk) {
            // Final chunk received
        }
    }
};
```

### WebSocket Context Creation

Client connections create their own `WebSocketContext` with `isServer = false`, allowing them to:
- Handle client-side WebSocket protocol
- Manage connection state
- Process incoming/outgoing messages

### Error Handling

Connection errors are handled through the `connectError` callback, which receives:
- WebSocket instance
- Error code indicating the failure reason

## Compatibility

The client functionality is fully compatible with:
- Existing server functionality
- SSL and non-SSL connections
- All existing WebSocket features (compression, ping/pong, etc.)
- The same event loop and threading model

## Example Applications

See `ClientExample.cpp` for a complete working example of a WebSocket client that:
- Connects to a server
- Sends and receives messages
- Handles connection events
- Demonstrates error handling
