# API Cleanup: Removing "client" Prefix

This document explains the cleanup of the `TemplatedClientApp` API by removing the redundant "client" prefix from method names.

## Overview

After successfully separating client and server functionality into dedicated classes, the "client" prefix in method names became redundant and made the API less clean. Since we now have a dedicated `TemplatedClientApp` class, it's clear that all methods are client operations.

## What Was Changed

### WebSocket Methods
- **Before**: `clientConnectWS()`
- **After**: `connectWS()`

### HTTP Methods
- **Before**: `clientGet()`, `clientPost()`, `clientPut()`, `clientDelete()`, `clientPatch()`
- **After**: `get()`, `post()`, `put()`, `del()`, `patch()`

### Generic HTTP Method
- **Before**: `clientRequest()`
- **After**: `request()`

### Connection Management Methods
- **Before**: `getClientConnectionCount()`, `closeAllClients()`, `hasActiveClients()`
- **After**: `getConnectionCount()`, `closeAll()`, `hasActiveConnections()`

## Benefits of This Cleanup

### 1. **Cleaner API**
- Method names are more concise and readable
- No redundant prefixes that add no value
- Consistent with modern API design principles

### 2. **Better Developer Experience**
- Shorter method names are easier to type
- Less visual clutter in code
- More intuitive method names

### 3. **Consistency**
- Aligns with the server API style (no "server" prefix)
- Follows the principle that the class name indicates the context
- Consistent naming patterns across the library

### 4. **Future-Proof**
- Easier to add new methods without prefix confusion
- Cleaner API surface for documentation
- Better IDE autocomplete experience

## Usage Examples

### Before (with "client" prefix)
```cpp
uWS::ClientApp app;

// WebSocket
app.clientConnectWS<Data>("ws://localhost:3000", behavior);

// HTTP
app.clientGet("http://api.example.com", httpBehavior);
app.clientPost("http://api.example.com", body, httpBehavior);

// Connection management
if (app.hasActiveClients()) {
    std::cout << "Active clients: " << app.getClientConnectionCount() << std::endl;
}
app.closeAllClients();
```

### After (clean API)
```cpp
uWS::ClientApp app;

// WebSocket
app.connectWS<Data>("ws://localhost:3000", behavior);

// HTTP
app.get("http://api.example.com", httpBehavior);
app.post("http://api.example.com", body, httpBehavior);

// Connection management
if (app.hasActiveConnections()) {
    std::cout << "Active connections: " << app.getConnectionCount() << std::endl;
}
app.closeAll();
```

## Migration Guide

### For Existing Code

If you have existing code using the old method names:

1. **WebSocket connections**:
   ```cpp
   // Before
   app.clientConnectWS<Data>("ws://...", behavior);
   
   // After
   app.connectWS<Data>("ws://...", behavior);
   ```

2. **HTTP requests**:
   ```cpp
   // Before
   app.clientGet("http://...", behavior);
   app.clientPost("http://...", body, behavior);
   
   // After
   app.get("http://...", behavior);
   app.post("http://...", body, behavior);
   ```

3. **Connection management**:
   ```cpp
   // Before
   app.getClientConnectionCount();
   app.closeAllClients();
   app.hasActiveClients();
   
   // After
   app.getConnectionCount();
   app.closeAll();
   app.hasActiveConnections();
   ```

### For New Code

- Use the new clean API names
- No "client" prefix needed
- Method names are self-explanatory in the context of `ClientApp`

## API Comparison

### Server vs Client API

| Server (App) | Client (ClientApp) |
|--------------|-------------------|
| `app.get("/path", handler)` | `app.get("http://...", behavior)` |
| `app.post("/path", handler)` | `app.post("http://...", body, behavior)` |
| `app.ws("/path", behavior)` | `app.connectWS<Data>("ws://...", behavior)` |
| `app.listen(port, handler)` | `app.run()` |

The APIs are now consistent and clean, with each class having methods appropriate to its purpose.

## Conclusion

This cleanup provides a much cleaner and more intuitive API for the `TemplatedClientApp` class. The removal of redundant prefixes makes the code more readable and maintainable while preserving all functionality. The API now follows modern design principles and provides a better developer experience.
