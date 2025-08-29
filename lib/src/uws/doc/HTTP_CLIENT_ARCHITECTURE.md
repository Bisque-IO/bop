# HTTP Client Architecture

This document explains the new `HttpClientContext.h` architecture that was created to properly support HTTP client functionality in uWebSockets.

## Overview

The original `HttpContext.h` was designed specifically for server-side HTTP handling, including request routing, WebSocket upgrades, and server-specific event handling. To properly support HTTP client functionality, we created a separate `HttpClientContext.h` that's specifically designed for client-side HTTP operations.

## Key Differences

### HttpContext (Server-side)
- **Purpose**: Handle incoming HTTP requests from clients
- **Event Flow**: Connection → Request → Response → Disconnect
- **Key Features**:
  - Request routing and handling
  - WebSocket upgrade support
  - Server name indication (SNI)
  - Child app distribution
  - Request filtering

### HttpClientContext (Client-side)
- **Purpose**: Make outgoing HTTP requests to servers
- **Event Flow**: Connect → Send Request → Receive Response → Disconnect
- **Key Features**:
  - Client connection management
  - Response handling
  - Error handling
  - Timeout management
  - Connection pooling (future)

## Architecture Components

### 1. HttpClientContext Class

```cpp
template <bool SSL>
struct HttpClientContext {
    // Client-specific event handlers
    void onClientConnect(MoveOnlyFunction<void(HttpResponse<SSL> *, int, std::string_view)> &&handler);
    void onClientDisconnect(MoveOnlyFunction<void(HttpResponse<SSL> *, int, void *)> &&handler);
    void onClientResponse(MoveOnlyFunction<void(HttpResponse<SSL> *, HttpRequest *)> &&handler);
    void onClientTimeout(MoveOnlyFunction<void(HttpResponse<SSL> *)> &&handler);
    void onClientConnectError(MoveOnlyFunction<void(HttpResponse<SSL> *, int)> &&handler);
    
    // Connection methods
    us_socket_t *connect(const char *host, int port, const char *source_host = nullptr, int options = 0);
    us_socket_t *connectUnix(const char *server_path, int options = 0);
};
```

### 2. HttpContextData Extensions

Added client-specific callback fields to `HttpContextData.h`:

```cpp
/* HTTP Client-specific callbacks */
MoveOnlyFunction<void(HttpResponse<SSL> *, int, std::string_view)> clientConnectHandler = nullptr;
MoveOnlyFunction<void(HttpResponse<SSL> *, int, void *)> clientDisconnectHandler = nullptr;
MoveOnlyFunction<void(HttpResponse<SSL> *, HttpRequest *)> clientResponseHandler = nullptr;
MoveOnlyFunction<void(HttpResponse<SSL> *)> clientTimeoutHandler = nullptr;
MoveOnlyFunction<void(HttpResponse<SSL> *, int)> clientConnectErrorHandler = nullptr;
```

### 3. Event Handling

#### Client Connection Events
- **onClientConnect**: Called when connection to server is established
- **onClientDisconnect**: Called when connection is closed
- **onClientResponse**: Called when HTTP response is received
- **onClientTimeout**: Called when connection times out
- **onClientConnectError**: Called when connection fails

#### Response Processing
- Parses HTTP responses (client-side)
- Handles chunked transfer encoding
- Manages connection state
- Supports keep-alive connections

## Usage in App.h

The `TemplatedApp` class now uses `HttpClientContext` for client operations:

```cpp
/* Create HTTP client context */
auto *httpClientContext = HttpClientContext<SSL>::create(Loop::get(), {});

/* Set up client response handler */
httpClientContext->onClientResponse([behavior = std::move(behavior)](auto *res, auto *req) mutable {
    if (behavior.success) {
        // Handle successful response
        behavior.success(res->getStatus(), headers, body);
    }
});

/* Set up error handlers */
if (behavior.error) {
    httpClientContext->onClientConnectError([behavior = std::move(behavior)](auto *res, int code) mutable {
        behavior.error(code, "Connection failed");
    });
}

/* Connect to the server */
httpClientContext->connect(host.c_str(), port);
```

## Benefits of This Architecture

### 1. **Separation of Concerns**
- Server and client functionality are clearly separated
- Each context is optimized for its specific use case
- Easier to maintain and extend

### 2. **Proper Event Handling**
- Client-specific events are handled appropriately
- Connection errors are properly managed
- Timeout handling is client-focused

### 3. **Performance**
- Client contexts are lightweight
- No unnecessary server-side routing overhead
- Optimized for outgoing connections

### 4. **Extensibility**
- Easy to add client-specific features
- Connection pooling can be implemented
- Custom client protocols can be added

## Future Enhancements

### 1. **Connection Pooling**
- Reuse connections for multiple requests
- Automatic connection management
- Load balancing across multiple servers

### 2. **Advanced Features**
- HTTP/2 support
- Request pipelining
- Automatic retry logic
- Circuit breaker patterns

### 3. **Monitoring**
- Connection metrics
- Performance monitoring
- Debug logging

## Integration with Existing Code

The new `HttpClientContext` integrates seamlessly with the existing uWebSockets architecture:

- Uses the same underlying `us_socket_context_t` infrastructure
- Compatible with existing SSL/TLS handling
- Follows the same event loop patterns
- Maintains consistency with server-side APIs

This architecture provides a solid foundation for HTTP client functionality while maintaining the performance and reliability characteristics of uWebSockets.
