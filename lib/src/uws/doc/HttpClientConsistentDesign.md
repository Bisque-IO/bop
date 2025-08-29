# HttpClient Hybrid Design

## Overview

The HttpClient has been refactored to follow a **hybrid approach** that reduces coupling while maintaining flexibility. `HttpClientConnection` can work both standalone and with context management, providing the best of both worlds.

## Design Philosophy

### Reduced Coupling with Flexibility

The new design follows these principles:

1. **Standalone Capability**: `HttpClientConnection` can work independently without a context
2. **Context Benefits**: When used with context, connections get additional management features
3. **Clear Separation**: Context coordinates but doesn't tightly couple to connections
4. **Flexible Initialization**: Users choose the level of coupling they need

### Architecture Components

```cpp
// Standalone connection (no context dependency)
HttpClientConnection<SSL, USERDATA> : AsyncSocket<SSL>

// Context coordinator (manages but doesn't own connections)
HttpClientContext<SSL> : Coordinator

// Behavior configuration (shared between standalone and context)
HttpClientBehavior<SSL> : Configuration
```

## Key Features

### 1. Standalone Connections

```cpp
// Create standalone connection (no context)
auto connection = app.createStandaloneConnection("example.com", 80, behavior);

// Configure behavior for standalone usage
HttpClientBehavior<false> behavior;
behavior.onConnect = [](auto* conn) { /* handle connect */ };
behavior.onResponse = [](auto* req, auto* res) { /* handle response */ };

// Use connection directly
connection->get("/api/data", [](auto* response) {
    std::cout << "Response: " << response->getStatus() << std::endl;
});
```

### 2. Context-Based Connections

```cpp
// Create context for coordinated management
uint64_t contextId = app.createHttpClientContext(behavior);

// Create connection with context
auto connection = app.connect(contextId, "example.com", 80);

// Context provides additional features
auto context = app.getHttpContext(contextId);
context->setTimeout(60, 120);  // Affects all connections
context->broadcast(data);      // Send to all connections
```

### 3. Mixed Usage

```cpp
// Mix standalone and context connections
auto standalone = app.createStandaloneConnection("api1.com", 80);
auto contextConn = app.connect(contextId, "api2.com", 80);

// Both work the same way for HTTP requests
standalone->get("/data", handler);
contextConn->post("/data", body, handler);
```

### 4. Connection Variants

```cpp
// Hostname and port
auto conn1 = app.connect("example.com", 80);

// Raw IP address
auto conn2 = app.connect("192.168.1.1", 8080, true);

// URL parsing
auto conn3 = app.connect("http://example.com:8080");

// Standalone variants
auto standalone1 = app.createStandaloneConnection("example.com", 80);
auto standalone2 = app.createStandaloneConnection("192.168.1.1", 8080, true);
auto standalone3 = app.createStandaloneConnection("http://example.com:8080");
```

## API Comparison

### Before (Tight Coupling)
```cpp
// Old API - context tightly coupled to connections
HttpClientContext* context = HttpClientContext::create(loop, {});
auto connection = context->createConnection(host, port);
// Connection always depends on context
```

### After (Hybrid Approach)
```cpp
// New API - flexible coupling
// Standalone
auto standalone = app.createStandaloneConnection(host, port, behavior);

// Context-based
uint64_t contextId = app.createHttpClientContext(behavior);
auto contextConn = app.connect(contextId, host, port);

// Both work identically for HTTP requests
standalone->get("/api", handler);
contextConn->get("/api", handler);
```

## Benefits

### 1. **Flexibility**
- Standalone connections for simple use cases
- Context management for complex scenarios
- Easy to switch between approaches

### 2. **Testability**
- Connections can be tested independently
- Context can be tested separately
- Clear separation of concerns

### 3. **Performance**
- No overhead for standalone connections
- Context benefits when needed
- Efficient connection pooling

### 4. **Maintainability**
- Reduced coupling between components
- Clear responsibility boundaries
- Easier to extend and modify

### 5. **Backward Compatibility**
- Existing patterns still work
- Gradual migration possible
- No breaking changes

## Usage Patterns

### Simple HTTP Client
```cpp
#include "ClientApp.h"

int main() {
    ClientApp app;
    
    // Simple standalone connection
    auto connection = app.createStandaloneConnection("httpbin.org", 80);
    connection->get("/get", [](auto* response) {
        std::cout << "Response: " << response->getStatus() << std::endl;
    });
    
    app.run();
    return 0;
}
```

### Advanced HTTP Client with Context
```cpp
#include "ClientApp.h"

int main() {
    ClientApp app;
    
    // Configure behavior
    HttpClientBehavior<false> behavior;
    behavior.onConnect = [](auto* conn) {
        std::cout << "Connected to " << conn->getHost() << std::endl;
    };
    
    // Create context
    uint64_t contextId = app.createHttpClientContext(behavior);
    
    // Multiple connections with context management
    auto conn1 = app.connect(contextId, "api1.com", 80);
    auto conn2 = app.connect(contextId, "api2.com", 80);
    
    conn1->get("/data1", handler1);
    conn2->post("/data2", body, handler2);
    
    // Context-level operations
    auto context = app.getHttpContext(contextId);
    context->setTimeout(60, 120);
    context->setMaxBackpressure(128 * 1024);
    
    app.run();
    return 0;
}
```

### Mixed Approach
```cpp
#include "ClientApp.h"

int main() {
    ClientApp app;
    
    // Standalone connection for simple requests
    auto simple = app.createStandaloneConnection("simple-api.com", 80);
    simple->get("/status", [](auto* res) { /* handle */ });
    
    // Context for complex management
    uint64_t contextId = app.createHttpClientContext(behavior);
    auto complex = app.connect(contextId, "complex-api.com", 80);
    complex->post("/upload", data, [](auto* res) { /* handle */ });
    
    // Both work the same way
    simple->get("/data", handler);
    complex->get("/data", handler);
    
    app.run();
    return 0;
}
```

## Connection Lifecycle

### Standalone Connection
```cpp
// 1. Create connection
auto connection = app.createStandaloneConnection(host, port, behavior);

// 2. Use for HTTP requests
connection->get("/api", handler);

// 3. Connection manages its own lifecycle
// - Automatic cleanup when connection closes
// - No context dependencies
```

### Context Connection
```cpp
// 1. Create context
uint64_t contextId = app.createHttpClientContext(behavior);

// 2. Create connection with context
auto connection = app.connect(contextId, host, port);

// 3. Context manages connection lifecycle
// - Automatic registration/unregistration
// - Context-level operations affect all connections
// - Centralized cleanup
```

## Migration Guide

### From Tight Coupling to Hybrid

#### Old Pattern
```cpp
// Always required context
HttpClientContext* context = HttpClientContext::create(loop, {});
auto connection = context->createConnection(host, port);
```

#### New Pattern - Standalone
```cpp
// No context needed for simple cases
auto connection = app.createStandaloneConnection(host, port, behavior);
```

#### New Pattern - Context-Based
```cpp
// Context for complex management
uint64_t contextId = app.createHttpClientContext(behavior);
auto connection = app.connect(contextId, host, port);
```

## Advanced Features

### Connection State Management
```cpp
auto connection = app.connect("example.com", 80);

// Check connection state
if (connection->getState() == HttpClientConnectionState::CONNECTED) {
    // Connection is ready
}

// Get connection info
std::cout << "Host: " << connection->getHost() << std::endl;
std::cout << "Port: " << connection->getRemotePort() << std::endl;
std::cout << "Is standalone: " << connection->isStandaloneConnection() << std::endl;
std::cout << "Has context: " << (connection->getContext() ? "yes" : "no") << std::endl;
```

### Context Management
```cpp
auto context = app.getHttpContext(contextId);

// Context-level operations
context->setTimeout(60, 120);  // Affects all connections
context->setMaxBackpressure(128 * 1024);
context->broadcast(data);  // Send to all connections

// Get context info
std::cout << "Active connections: " << context->getConnectionCount() << std::endl;
std::cout << "Active requests: " << context->getRequestCount() << std::endl;
```

### Behavior Configuration
```cpp
HttpClientBehavior<false> behavior;

// Connection-level callbacks
behavior.onConnect = [](auto* conn) { /* handle connect */ };
behavior.onDisconnect = [](auto* conn, int code) { /* handle disconnect */ };
behavior.onTimeout = [](auto* conn) { /* handle timeout */ };

// Request-level callbacks
behavior.onResponse = [](auto* req, auto* res) { /* handle response */ };
behavior.onData = [](auto* req, auto* chunk) { /* handle data */ };
behavior.onError = [](auto* req, int code) { /* handle error */ };

// Configuration
behavior.idleTimeoutSeconds = 30;
behavior.longTimeoutMinutes = 60;
behavior.maxBackpressure = 64 * 1024;
behavior.userAgent = "MyApp/1.0";
behavior.keepAlive = true;
```

## Conclusion

The hybrid approach provides:

1. **Flexibility** - Choose the right level of coupling for your use case
2. **Simplicity** - Standalone connections for simple scenarios
3. **Power** - Context management for complex scenarios
4. **Performance** - No overhead when context isn't needed
5. **Maintainability** - Clear separation of concerns

This design ensures that HttpClient is both simple to use for basic cases and powerful enough for advanced scenarios, while maintaining clean architecture and reducing coupling between components.
