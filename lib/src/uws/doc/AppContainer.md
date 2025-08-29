# AppContainer: Unified App Management

## Overview

The `AppContainer` provides a unified solution for managing multiple uWS applications (`App`, `ClientApp`, `TCPApp`) that share the same event loop. It eliminates the problem of having multiple `run()` methods by providing a single, centralized event loop management system.

## Problem Solved

### Before: Multiple `run()` Methods
```cpp
// Problem: Multiple apps, multiple run() calls
App server;
ClientApp client;
TCPApp tcp;

// This would cause issues - which run() to call?
server.run();  // ❌ Blocks here
client.run();  // ❌ Never reached
tcp.run();     // ❌ Never reached
```

### After: Single `run()` Method
```cpp
// Solution: Single container, single run()
AppContainer container;

auto server = container.createHttpApp();
auto client = container.createClientApp();
auto tcp = container.createTcpApp();

// Single run() call for all apps
container.run();  // ✅ Runs all apps in shared loop
```

## Key Features

### 1. **Unified Event Loop Management**
- All apps share the same `Loop` instance
- Single `run()` method for all applications
- Automatic loop lifecycle management

### 2. **Named App Management**
- Create multiple apps with unique names
- Retrieve apps by name for later use
- Remove apps dynamically

### 3. **Multiple App Types Support**
- HTTP/HTTPS Server Apps (`App`, `SSLApp`)
- HTTP/HTTPS Client Apps (`ClientApp`, `SSLClientApp`)
- TCP Server Apps (`TCPApp`, `SSLTCPApp`)
- TCP Client Apps (`TCPClientApp`, `SSLTCPClientApp`)

### 4. **Pre-run and Post-run Callbacks**
- Execute code before the event loop starts
- Execute code after the event loop ends
- Useful for initialization and cleanup

### 5. **Global Access**
- Global `AppContainer` instance for convenience
- Global `run()`, `integrate()`, and `free()` functions

## Usage Patterns

### Basic Usage
```cpp
#include "AppContainer.h"

int main() {
    AppContainer container;
    
    // Create apps
    auto server = container.createHttpApp();
    auto client = container.createClientApp();
    
    // Configure apps
    server->get("/", [](auto* res, auto* req) {
        res->end("Hello World!");
    });
    
    // Single run() call for all apps
    container.run();
    
    return 0;
}
```

### Named Apps
```cpp
#include "AppContainer.h"

int main() {
    AppContainer container;
    
    // Create named apps
    auto apiServer = container.createHttpApp("api");
    auto webServer = container.createHttpApp("web");
    auto client = container.createClientApp("client");
    
    // Configure each app
    apiServer->get("/api/status", [](auto* res, auto* req) {
        res->end("API OK");
    });
    
    webServer->get("/", [](auto* res, auto* req) {
        res->end("Web OK");
    });
    
    // Retrieve apps by name
    auto retrievedApi = container.getHttpApp("api");
    auto retrievedWeb = container.getHttpApp("web");
    
    container.run();
    return 0;
}
```

### Mixed App Types
```cpp
#include "AppContainer.h"

int main() {
    AppContainer container;
    
    // HTTP Server
    auto httpServer = container.createHttpApp("http");
    httpServer->get("/", [](auto* res, auto* req) {
        res->end("HTTP Server");
    });
    
    // HTTPS Server
    auto httpsServer = container.createHttpsApp("https");
    httpsServer->get("/secure", [](auto* res, auto* req) {
        res->end("HTTPS Server");
    });
    
    // HTTP Client
    auto httpClient = container.createClientApp("http-client");
    
    // TCP Server
    auto tcpServer = container.createTcpApp("tcp");
    
    // TCP Client
    auto tcpClient = container.createTcpClientApp("tcp-client");
    
    container.run();
    return 0;
}
```

### Pre-run and Post-run Callbacks
```cpp
#include "AppContainer.h"

int main() {
    AppContainer container;
    
    // Add pre-run callback
    container.onPreRun([]() {
        std::cout << "Starting all applications..." << std::endl;
        // Initialize databases, load configs, etc.
    });
    
    // Add post-run callback
    container.onPostRun([]() {
        std::cout << "All applications stopped." << std::endl;
        // Cleanup resources, save state, etc.
    });
    
    // Create and configure apps...
    
    container.run();
    return 0;
}
```

### Global Access
```cpp
#include "AppContainer.h"

int main() {
    // Use global AppContainer
    auto& container = getAppContainer();
    
    auto server = container.createHttpApp();
    server->get("/", [](auto* res, auto* req) {
        res->end("Global Server");
    });
    
    // Use global run() function
    run();
    
    return 0;
}
```

### Integration with Existing Loop
```cpp
#include "AppContainer.h"

int main() {
    // Get existing loop (e.g., from Node.js integration)
    auto* existingLoop = Loop::get();
    
    // Create container with existing loop
    AppContainer container(existingLoop);
    
    auto server = container.createHttpApp();
    server->get("/", [](auto* res, auto* req) {
        res->end("Integrated Server");
    });
    
    // Integrate with existing loop instead of running
    container.integrate();
    
    return 0;
}
```

## API Reference

### AppContainer Methods

#### App Creation
```cpp
// HTTP Server Apps
App* createHttpApp(const std::string& name = "default");
SSLApp* createHttpsApp(const std::string& name = "default");

// HTTP Client Apps
ClientApp* createClientApp(const std::string& name = "default");
SSLClientApp* createSslClientApp(const std::string& name = "default");

// TCP Server Apps
TCPApp* createTcpApp(const std::string& name = "default");
SSLTCPApp* createSslTcpApp(const std::string& name = "default");

// TCP Client Apps
TCPClientApp* createTcpClientApp(const std::string& name = "default");
SSLTCPClientApp* createSslTcpClientApp(const std::string& name = "default");
```

#### App Retrieval
```cpp
// HTTP Server Apps
App* getHttpApp(const std::string& name = "default");
SSLApp* getHttpsApp(const std::string& name = "default");

// HTTP Client Apps
ClientApp* getClientApp(const std::string& name = "default");
SSLClientApp* getSslClientApp(const std::string& name = "default");

// TCP Server Apps
TCPApp* getTcpApp(const std::string& name = "default");
SSLTCPApp* getSslTcpApp(const std::string& name = "default");

// TCP Client Apps
TCPClientApp* getTcpClientApp(const std::string& name = "default");
SSLTCPClientApp* getSslTcpClientApp(const std::string& name = "default");
```

#### App Management
```cpp
// Remove apps
bool removeHttpApp(const std::string& name = "default");
bool removeHttpsApp(const std::string& name = "default");
bool removeClientApp(const std::string& name = "default");
bool removeSslClientApp(const std::string& name = "default");
bool removeTcpApp(const std::string& name = "default");
bool removeSslTcpApp(const std::string& name = "default");
bool removeTcpClientApp(const std::string& name = "default");
bool removeSslTcpClientApp(const std::string& name = "default");

// Clear all apps
void clear();
```

#### Statistics
```cpp
// Get app counts
size_t getAppCount() const;
size_t getHttpAppCount() const;
size_t getHttpsAppCount() const;
size_t getClientAppCount() const;
size_t getSslClientAppCount() const;
size_t getTcpAppCount() const;
size_t getSslTcpAppCount() const;
size_t getTcpClientAppCount() const;
size_t getSslTcpClientAppCount() const;
```

#### Callbacks
```cpp
// Pre-run and post-run callbacks
void onPreRun(MoveOnlyFunction<void()>&& callback);
void onPostRun(MoveOnlyFunction<void()>&& callback);
```

#### Loop Management
```cpp
// Loop access and control
Loop* getLoop() const;
void setLoop(Loop* newLoop);
void run();
void integrate();
void free();
```

### Global Functions
```cpp
// Global AppContainer access
AppContainer& getAppContainer();

// Global convenience functions
void run();
void integrate();
void free();
```

## Migration Guide

### From Multiple `run()` Calls
```cpp
// Old approach (problematic)
App server;
ClientApp client;
TCPApp tcp;

server.run();  // ❌ Blocks here
```

```cpp
// New approach (correct)
AppContainer container;

auto server = container.createHttpApp();
auto client = container.createClientApp();
auto tcp = container.createTcpApp();

container.run();  // ✅ Runs all apps
```

### From Individual App Management
```cpp
// Old approach
App server1;
App server2;
// Hard to manage multiple apps
```

```cpp
// New approach
AppContainer container;

auto server1 = container.createHttpApp("server1");
auto server2 = container.createHttpApp("server2");

// Easy management
container.getHttpApp("server1");
container.removeHttpApp("server2");
```

## Benefits

### 1. **Single Event Loop**
- All apps share the same loop instance
- No conflicts between multiple `run()` calls
- Efficient resource usage

### 2. **Centralized Management**
- Single point of control for all apps
- Easy to add, remove, and configure apps
- Unified lifecycle management

### 3. **Better Organization**
- Named apps for better identification
- Logical grouping of related apps
- Clear separation of concerns

### 4. **Enhanced Flexibility**
- Pre-run and post-run callbacks
- Integration with existing loops
- Global access for convenience

### 5. **Improved Maintainability**
- Single `run()` method to maintain
- Consistent API across all app types
- Better error handling and debugging

## Best Practices

### 1. **Use Named Apps**
```cpp
// Good: Named apps for clarity
auto apiServer = container.createHttpApp("api");
auto webServer = container.createHttpApp("web");

// Avoid: Default names for multiple apps
auto server1 = container.createHttpApp();  // Less clear
auto server2 = container.createHttpApp();
```

### 2. **Group Related Apps**
```cpp
// Good: Logical grouping
auto httpServer = container.createHttpApp("http");
auto httpsServer = container.createHttpsApp("https");
auto httpClient = container.createClientApp("http-client");
auto httpsClient = container.createSslClientApp("https-client");
```

### 3. **Use Pre-run Callbacks for Initialization**
```cpp
container.onPreRun([]() {
    // Initialize databases
    // Load configuration
    // Set up logging
    std::cout << "All systems ready" << std::endl;
});
```

### 4. **Use Post-run Callbacks for Cleanup**
```cpp
container.onPostRun([]() {
    // Save state
    // Close connections
    // Cleanup resources
    std::cout << "Cleanup completed" << std::endl;
});
```

### 5. **Check App Creation Success**
```cpp
auto server = container.createHttpApp("main");
if (!server) {
    std::cerr << "Failed to create server app" << std::endl;
    return 1;
}
```

## Conclusion

The `AppContainer` provides a robust solution for managing multiple uWS applications with a single event loop. It eliminates the problems associated with multiple `run()` methods while providing enhanced functionality for app management, organization, and lifecycle control.

This approach makes it easy to build complex applications that require multiple server and client components while maintaining clean, maintainable code.
