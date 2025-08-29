# TCPConnection Documentation

## Overview

`TCPConnection` is a struct that extends `AsyncSocket` to provide TCP-specific functionality, following the same design pattern as `WebSocket`. It provides methods for sending data, managing connection state, handling timeouts, and managing backpressure.

## Design Pattern

`TCPConnection` follows the same design pattern as `WebSocket`:

```cpp
template <bool SSL, typename USERDATA = void>
struct TCPConnection : AsyncSocket<SSL> {
    // TCP-specific methods and functionality
};
```

This design provides:
- **Inheritance from AsyncSocket** - Access to low-level socket operations
- **Template specialization** - SSL and non-SSL variants
- **User data support** - Per-connection custom data storage
- **Consistent API** - Same patterns as WebSocket for familiarity

## Key Features

### 1. Send Methods
```cpp
enum class SendStatus {
    SUCCESS,        // Data sent successfully
    BACKPRESSURE,   // Data partially sent, rest buffered due to backpressure
    DROPPED,        // Data dropped due to backpressure limits
    FAILED          // Send failed (connection closed, etc.)
};

SendStatus send(std::string_view data);
```

The `send()` method provides:
- **Backpressure handling** - Returns status indicating if backpressure occurred
- **Large data optimization** - Special path for messages >= 16KB
- **Automatic timeout reset** - Resets idle timeout on successful sends
- **Buffer management** - Handles incomplete sends with buffering

### 2. Corking Support
```cpp
TCPConnection* cork(MoveOnlyFunction<void()> &&handler);
```

Corking allows multiple sends to be batched together:
```cpp
conn->cork([conn]() {
    conn->send("Part 1: ");
    conn->send("Hello");
    conn->send(" - End");
});
```

### 3. Connection State Management
```cpp
bool isClient() const;
bool isServer() const;
std::string_view getRemoteAddress() const;
int getRemotePort() const;
USERDATA* getUserData();
```

### 4. Timeout Management
```cpp
void setTimeout(uint32_t idleTimeoutSeconds, uint32_t longTimeoutMinutes = 60);
std::pair<uint32_t, uint32_t> getTimeout() const;
bool hasTimedOut() const;
```

### 5. Backpressure Tracking
```cpp
uintmax_t getBackpressure() const;
bool hasBackpressure() const;
size_t getBufferedDataSize() const;
SendStatus flush();
```

## Usage Examples

### Basic TCP Server
```cpp
#include "TCPApp.h"
#include "TCPContext.h"

uWS::TCPApp server;
uWS::TCPBehavior<false> behavior;

behavior.onConnected = [](uWS::TCPConnection<false, void>* conn, std::string_view remoteAddr) {
    std::cout << "Client connected from " << remoteAddr << std::endl;
    conn->send("Welcome!");
};

behavior.onData = [](uWS::TCPConnection<false, void>* conn, std::string_view data) {
    // Echo the data back
    auto status = conn->send("Echo: " + std::string(data));
    
    if (status == uWS::SendStatus::BACKPRESSURE) {
        std::cout << "Backpressure detected, pausing sends" << std::endl;
    }
};

auto* context = server.createTCPContext(behavior);
auto* listenSocket = context->listen(nullptr, 8080, 0);
if (listenSocket) {
    std::cout << "Server listening on port 8080" << std::endl;
}
```

### Advanced TCP Server with Corking
```cpp
#include "TCPApp.h"
#include "TCPContext.h"

uWS::TCPApp server;
uWS::TCPBehavior<false> behavior;

behavior.onData = [](uWS::TCPConnection<false, void>* conn, std::string_view data) {
    // Use corking for multiple sends
    conn->cork([conn, data]() {
        conn->send("Response: ");
        conn->send(data);
        conn->send(" - Processed");
    });
    
    // Set connection-specific timeout
    conn->setTimeout(120, 30);  // 2 minutes idle, 30 minutes long timeout
};

behavior.onTimeout = [](uWS::TCPConnection<false, void>* conn) {
    std::cout << "Connection timed out: " << conn->getRemoteAddress() << std::endl;
};

behavior.onLongTimeout = [](uWS::TCPConnection<false, void>* conn) {
    std::cout << "Connection long timeout, closing: " << conn->getRemoteAddress() << std::endl;
    conn->close();
};

auto* context = server.createTCPContext(behavior);
auto* listenSocket = context->listen(nullptr, 8080, 0);
if (listenSocket) {
    std::cout << "Server listening on port 8080" << std::endl;
}
```

### SSL TCP Server
```cpp
#include "TCPApp.h"
#include "TCPContext.h"

uWS::TCPAppSSL sslServer;
uWS::TCPBehavior<true> sslBehavior;

sslBehavior.onData = [](uWS::TCPConnection<true, void>* conn, std::string_view data) {
    // Handle SSL-encrypted data
    conn->send("SSL Response: " + std::string(data));
};

auto* context = sslServer.createTCPContext(sslBehavior);
auto* listenSocket = context->listen(nullptr, 443, 0);
if (listenSocket) {
    std::cout << "SSL Server listening on port 443" << std::endl;
}
```

## Connection Lifecycle

### 1. Connection Establishment
```cpp
behavior.onConnected = [](uWS::TCPConnection<false, void>* conn, std::string_view remoteAddr) {
    // Connection is established
    conn->setTimeout(60, 30);  // Set timeouts
    conn->send("Welcome message");
};
```

### 2. Data Reception
```cpp
behavior.onData = [](uWS::TCPConnection<false, void>* conn, std::string_view data) {
    // Handle incoming data
    auto status = conn->send("Response");
    
    switch (status) {
        case uWS::SendStatus::SUCCESS:
            // Continue sending
            break;
        case uWS::SendStatus::BACKPRESSURE:
            // Pause sending, wait for onWritable
            break;
        case uWS::SendStatus::DROPPED:
            // Data was dropped due to backpressure limits
            break;
    }
};
```

### 3. Backpressure Handling
```cpp
behavior.onWritable = [](uWS::TCPConnection<false, void>* conn, std::string_view data) {
    // Socket is writable again, resume sending
    std::cout << "Socket writable, resuming sends" << std::endl;
};

behavior.onDrain = [](uWS::TCPConnection<false, void>* conn) {
    // Backpressure buffer has been drained
    std::cout << "Backpressure buffer drained" << std::endl;
};
```

### 4. Timeout Handling
```cpp
behavior.onTimeout = [](uWS::TCPConnection<false, void>* conn) {
    // Idle timeout occurred
    std::cout << "Connection idle timeout" << std::endl;
};

behavior.onLongTimeout = [](uWS::TCPConnection<false, void>* conn) {
    // Long timeout occurred, force disconnect
    std::cout << "Connection long timeout, closing" << std::endl;
    conn->close();
};
```

### 5. Connection Closure
```cpp
behavior.onDisconnected = [](uWS::TCPConnection<false, void>* conn, int code, void* reason) {
    // Connection was closed
    std::cout << "Connection closed with code: " << code << std::endl;
    std::cout << "Remote address: " << conn->getRemoteAddress() << std::endl;
    std::cout << "Remote port: " << conn->getRemotePort() << std::endl;
};
```

## Performance Considerations

### 1. Large Data Optimization
For messages >= 16KB, `TCPConnection` uses a special optimization path:
- Direct socket write for non-SSL connections
- Reduced memory allocation overhead
- Better performance for large data transfers

### 2. Backpressure Management
- Automatic backpressure detection
- Configurable backpressure limits
- Buffer management for incomplete sends
- Drain callbacks for flow control

### 3. Memory Management
- Zero-copy operations where possible
- Efficient buffer reuse
- Automatic cleanup on connection close

## Type Aliases

For convenience, type aliases are provided:

```cpp
template <typename USERDATA = void>
using TCPConnectionSSL = TCPConnection<true, USERDATA>;

template <typename USERDATA = void>
using TCPConnectionNonSSL = TCPConnection<false, USERDATA>;
```

## User Data Support

`TCPConnection` supports per-connection user data through the `USERDATA` template parameter:

```cpp
// Define custom user data
struct MyUserData {
    std::string sessionId;
    int messageCount = 0;
    bool authenticated = false;
};

// Use with custom user data
uWS::TCPConnection<false, MyUserData>* conn = /* ... */;
MyUserData* userData = conn->getUserData();
userData->sessionId = "session123";
userData->messageCount++;
```

## Integration with TCPContext

`TCPConnection` works seamlessly with `TCPContext`:

```cpp
// Create context and use TCPConnection methods
auto* context = server.createTCPContext();
if (context) {
    // Set context-level timeouts
    context->setTimeout(60, 30);
}
```

## Error Handling

### Connection State Validation
```cpp
auto status = conn->send(data);
if (conn->isClosed() || conn->isShutDown()) {
    // Connection is not in valid state for sending
    return;
}
```

### Timeout Management
```cpp
if (conn->hasTimedOut()) {
    // Connection has timed out
    conn->close();
    return;
}
```

### Buffer Management
```cpp
if (conn->hasBackpressure()) {
    // Handle backpressure
    auto flushStatus = conn->flush();
    if (flushStatus == uWS::SendStatus::SUCCESS) {
        std::cout << "Buffer flushed successfully" << std::endl;
    }
}
```

## Best Practices

1. **Always check send status** - Handle BACKPRESSURE and DROPPED cases
2. **Use corking for multiple sends** - Improves performance for batched operations
3. **Set appropriate timeouts** - Both idle and long timeout values
4. **Handle backpressure** - Implement onWritable and onDrain callbacks
5. **Validate connection state** - Check state before sending data
6. **Clean up resources** - Close connections properly
7. **Monitor performance** - Track write offsets and buffer usage

## Related Components

- **[TCPApp](TCPAPP_README.md)** - TCP server application framework
- **[TCPContext](TCPContext.md)** - TCP context management
- **[TCPConnectionData](TCPConnectionData.md)** - Connection data structures
- **[AsyncSocket](AsyncSocket.md)** - Base socket functionality
