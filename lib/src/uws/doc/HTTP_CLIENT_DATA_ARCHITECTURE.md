# HTTP Client Data Architecture

This document explains the separation of HTTP client data structures into a dedicated `HttpClientContextData.h` file, following the same architectural pattern established with `HttpClientContext.h`.

## Overview

Following the successful refactoring that separated client and server functionality into dedicated files, we've now applied the same principle to HTTP data structures. This creates a clean separation between server-side and client-side HTTP data management.

## Architecture Components

### 1. HttpContextData.h (Server-focused)
Contains data structures specifically for server-side HTTP operations:

```cpp
template <bool SSL>
struct alignas(16) HttpContextData {
    // Server-specific data
    std::vector<MoveOnlyFunction<void(HttpResponse<SSL> *, int)>> filterHandlers;
    MoveOnlyFunction<void(const char *hostname)> missingServerNameHandler;
    
    // Router management
    HttpRouter<RouterData> *currentRouter = &router;
    HttpRouter<RouterData> router;
    
    // Server state
    void *upgradedWebSocket = nullptr;
    bool isParsingHttp = false;
    
    // Child app distribution
    std::vector<void *> childApps;
    unsigned int roundRobin = 0;
};
```

### 2. HttpClientContextData.h (Client-focused)
Contains data structures specifically for client-side HTTP operations:

```cpp
template <bool SSL>
struct alignas(16) HttpClientContextData {
    // Client-specific callbacks
    MoveOnlyFunction<void(HttpResponse<SSL> *, int, std::string_view)> clientConnectHandler = nullptr;
    MoveOnlyFunction<void(HttpResponse<SSL> *, int, void *)> clientDisconnectHandler = nullptr;
    MoveOnlyFunction<void(HttpResponse<SSL> *, HttpRequest *)> clientResponseHandler = nullptr;
    MoveOnlyFunction<void(HttpResponse<SSL> *)> clientTimeoutHandler = nullptr;
    MoveOnlyFunction<void(HttpResponse<SSL> *, int)> clientConnectErrorHandler = nullptr;
    
    // Client-specific state tracking
    bool isClientContext = true;
    unsigned int activeConnections = 0;
    unsigned int totalRequests = 0;
    unsigned int failedRequests = 0;
};
```

## Key Differences

### HttpContextData (Server)
- **Purpose**: Handle incoming HTTP requests and server-side routing
- **Key Features**:
  - Request filtering and routing
  - WebSocket upgrade management
  - Child app distribution
  - Server name indication (SNI)
  - Request parsing state management

### HttpClientContextData (Client)
- **Purpose**: Handle outgoing HTTP requests and client-side responses
- **Key Features**:
  - Client connection callbacks
  - Response handling callbacks
  - Error and timeout callbacks
  - Connection state tracking
  - Request statistics

## Benefits of This Separation

### 1. **Clear Data Boundaries**
- Server and client data are completely separated
- No confusion about which data belongs to which context
- Easier to understand and maintain

### 2. **Optimized Memory Usage**
- Client contexts only allocate memory for client-specific data
- Server contexts only allocate memory for server-specific data
- No wasted memory for unused fields

### 3. **Type Safety**
- Compile-time guarantees about data structure usage
- Prevents accidental mixing of server and client data
- Clearer API boundaries

### 4. **Extensibility**
- Easy to add client-specific features without affecting server code
- Easy to add server-specific features without affecting client code
- Independent evolution of client and server capabilities

## Usage in HttpClientContext

The `HttpClientContext` now uses `HttpClientContextData` exclusively:

```cpp
template <bool SSL>
struct HttpClientContext {
    // Uses HttpClientContextData instead of HttpContextData
    HttpClientContextData<SSL> *getSocketContextData() {
        return (HttpClientContextData<SSL> *) us_socket_context_ext(SSL, getSocketContext());
    }
    
    // All client callbacks are stored in HttpClientContextData
    void onClientConnect(MoveOnlyFunction<void(HttpResponse<SSL> *, int, std::string_view)> &&handler) {
        getSocketContextData()->clientConnectHandler = std::move(handler);
    }
    
    // Client-specific state tracking
    unsigned int getActiveConnections() {
        return getSocketContextData()->activeConnections;
    }
};
```

## Migration Impact

### For Existing Code
- **Server code**: No changes required - continues to use `HttpContextData`
- **Client code**: Now uses `HttpClientContextData` through `HttpClientContext`
- **Mixed applications**: Can use both data structures independently

### For New Code
- **Server applications**: Use `HttpContextData` through `HttpContext`
- **Client applications**: Use `HttpClientContextData` through `HttpClientContext`
- **Hybrid applications**: Use both as needed

## Future Enhancements

### Client-Side Features
With dedicated client data structures, we can easily add:
- Connection pooling statistics
- Request/response metrics
- Client-side caching data
- Retry logic state
- Circuit breaker patterns

### Server-Side Features
Server data structures can be enhanced with:
- Request rate limiting data
- Server-side caching
- Load balancing state
- Authentication/authorization data

## Conclusion

This separation provides a clean, maintainable, and extensible architecture for HTTP client and server functionality. The dedicated data structures ensure that each context only contains the data it needs, leading to better performance, clearer code, and easier maintenance.
