# Client Functionality Refactoring

This document explains the refactoring that moved all client functionality from `App.h` to a dedicated `ClientApp.h` file.

## Overview

The original approach added client functionality directly to the `TemplatedApp` class in `App.h`. While this worked, it created a mixed-purpose class that handled both server and client operations. The refactoring separates these concerns by creating a dedicated client application class.

## Why This Refactoring Was Needed

### 1. **Separation of Concerns**
- **Before**: `App.h` handled both server and client functionality
- **After**: `App.h` focuses on server functionality, `ClientApp.h` handles client functionality

### 2. **Cleaner Architecture**
- **Before**: Mixed server/client code in one file
- **After**: Clear separation with dedicated client and server classes

### 3. **Better Maintainability**
- **Before**: Changes to client code could affect server code
- **After**: Client and server code are completely isolated

### 4. **Improved API Clarity**
- **Before**: Users had to know which methods were for servers vs clients
- **After**: Clear distinction: `App` for servers, `ClientApp` for clients

## What Was Moved

### From `App.h` to `ClientApp.h`:

#### WebSocket Client Functionality
- `WebSocketClientBehavior` structure
- `connectWS()` methods (all variants)
- WebSocket client connection management

#### HTTP Client Functionality
- `HttpClientBehavior` structure
- `HttpClientRequest` structure
- `get()`, `post()`, `put()`, `del()`, `patch()` methods
- `request()` method
- HTTP client connection management

#### Connection Management
- `getConnectionCount()` method
- `closeAll()` method
- `hasActiveConnections()` method

## New Architecture

### ClientApp.h Structure

```cpp
template <bool SSL>
struct TemplatedClientApp {
    // WebSocket Client functionality
    template <typename UserData>
    struct WebSocketClientBehavior { /* ... */ };
    
    // HTTP Client functionality
    struct HttpClientBehavior { /* ... */ };
    struct HttpClientRequest { /* ... */ };
    
    // WebSocket client methods
    template <typename UserData>
    TemplatedClientApp &&connectWS(std::string url, WebSocketClientBehavior<UserData> &&behavior);
    
    // HTTP client methods
    TemplatedClientApp &&get(std::string url, HttpClientBehavior &&behavior);
    TemplatedClientApp &&post(std::string url, std::string_view body, HttpClientBehavior &&behavior);
    // ... other HTTP methods
    
    // Connection management
    unsigned int getConnectionCount();
    TemplatedClientApp &&closeAll();
    bool hasActiveConnections();
    
    // Event loop
    TemplatedClientApp &&run();
    Loop *getLoop();
};

// Type aliases
typedef TemplatedClientApp<false> ClientApp;
typedef TemplatedClientApp<true> SSLClientApp;
```

### App.h Structure (After Refactoring)

```cpp
template <bool SSL>
struct TemplatedApp {
    // Server-only functionality
    // WebSocket server methods (ws, listen, etc.)
    // HTTP server methods (get, post, etc.)
    // Server-specific features (SNI, child apps, etc.)
    // No client functionality
};

// Type aliases
typedef TemplatedApp<false> App;
typedef TemplatedApp<true> SSLApp;
```

## Usage Changes

### Before Refactoring
```cpp
#include "App.h"

// Mixed server/client usage
uWS::App app;

// Server functionality
app.get("/", [](auto *res, auto *req) { /* ... */ });
app.listen(3000, [](auto *ls) { /* ... */ });

// Client functionality (mixed in)
app.connectWS<ClientData>("ws://localhost:3001", behavior);
app.get("http://api.example.com", httpBehavior);
```

### After Refactoring
```cpp
#include "App.h"
#include "ClientApp.h"

// Server application
uWS::App serverApp;
serverApp.get("/", [](auto *res, auto *req) { /* ... */ });
serverApp.listen(3000, [](auto *ls) { /* ... */ });

// Client application (separate)
uWS::ClientApp clientApp;
clientApp.connectWS<ClientData>("ws://localhost:3001", behavior);
clientApp.get("http://api.example.com", httpBehavior);
```

## Benefits of the Refactoring

### 1. **Clear API Boundaries**
- Server operations use `App`/`SSLApp`
- Client operations use `ClientApp`/`SSLClientApp`
- No confusion about which methods to use

### 2. **Reduced Complexity**
- Each class has a single responsibility
- Easier to understand and maintain
- Smaller, more focused codebases

### 3. **Better Testing**
- Server and client functionality can be tested independently
- Easier to mock and isolate components
- Clearer test boundaries

### 4. **Improved Documentation**
- Separate documentation for server and client APIs
- Clearer examples and use cases
- Better organization of features

### 5. **Future Extensibility**
- Client-specific features can be added without affecting server code
- Server-specific features can be added without affecting client code
- Easier to add new client or server capabilities

## Migration Guide

### For Existing Code

If you have existing code that uses client functionality from `App.h`:

1. **Add ClientApp.h include**:
   ```cpp
   #include "ClientApp.h"
   ```

2. **Change App to ClientApp for client operations**:
   ```cpp
   // Before
   uWS::App app;
   app.clientConnectWS<Data>("ws://...", behavior);
   
   // After
   uWS::ClientApp app;
   app.connectWS<Data>("ws://...", behavior);
   ```

3. **Use separate instances for server and client**:
   ```cpp
   // Server functionality
   uWS::App serverApp;
   serverApp.get("/", handler);
   serverApp.listen(3000, listener);
   
   // Client functionality
uWS::ClientApp clientApp;
clientApp.connectWS<Data>("ws://...", behavior);
   ```

### For New Code

- Use `App`/`SSLApp` for server applications
- Use `ClientApp`/`SSLClientApp` for client applications
- Keep server and client code separate

## Conclusion

This refactoring provides a much cleaner and more maintainable architecture for uWebSockets. The separation of client and server functionality makes the codebase easier to understand, test, and extend while providing a clearer API for users.
