# HTTP Client Server Separation Fixes

## Overview

Fixed several issues in the HTTP client implementation to properly separate client and server concerns, and added configurable idle timeout functionality.

## Key Issues Fixed

### 1. **Removed Server-Side Dependencies**

**Before:**
```cpp
// HttpClientContext.h was incorrectly using server-side types
template <bool> friend struct HttpResponse;  // Server type
static const int HTTP_CLIENT_IDLE_TIMEOUT_S = 30;  // Hard-coded timeout
HttpResponseData<SSL>* httpResponseData = ...;  // Server response data
```

**After:**
```cpp
// HttpClientContext.h now uses client-specific types
template <bool> friend struct HttpClientResponse;  // Client type
// Timeout is now configurable per context
HttpClientResponse<SSL>* httpClientResponse = ...;  // Client response data
```

### 2. **Added Configurable Idle Timeout**

**Before:**
```cpp
// Hard-coded timeout in HttpClientContext
static const int HTTP_CLIENT_IDLE_TIMEOUT_S = 30;
us_socket_timeout(SSL, s, HTTP_CLIENT_IDLE_TIMEOUT_S);
```

**After:**
```cpp
// Configurable timeout in HttpClientContextData
unsigned int idleTimeoutSeconds = 30;  // Default value

// In HttpClientContext.h
us_socket_timeout(SSL, s, httpContextData->idleTimeoutSeconds);

// New method to configure timeout
void setIdleTimeout(unsigned int timeoutSeconds) {
    getSocketContextData()->idleTimeoutSeconds = timeoutSeconds;
}
```

### 3. **Fixed State Machine References**

**Before:**
```cpp
// References to non-existent HttpClientContextState
contextData->transitionTo(HttpClientContextState<SSL>::ProcessingState::COMPLETE);
```

**After:**
```cpp
// References to merged HttpClientContextData
contextData->transitionTo(HttpClientContextData<SSL>::ProcessingState::COMPLETE);
```

## Changes Made

### HttpClientContextData.h
- **Added configurable idle timeout**: `unsigned int idleTimeoutSeconds = 30;`
- **Merged state machine**: All state machine logic is now in this single file

### HttpClientContext.h
- **Removed server dependencies**: No more references to `HttpResponse` or `HttpResponseData`
- **Updated friend declarations**: Now friends with `HttpClientResponse` instead of `HttpResponse`
- **Removed hard-coded timeout**: No more `HTTP_CLIENT_IDLE_TIMEOUT_S` constant
- **Added timeout configuration**: New `setIdleTimeout()` method
- **Fixed state machine references**: All references now use `HttpClientContextData<SSL>::ProcessingState`
- **Updated response handling**: Uses `HttpClientResponse` instead of `HttpResponseData`

### HttpClientResponse.h
- **Client-specific response type**: Properly designed for client-side operations
- **Response state management**: `HTTP_RESPONSE_PENDING`, `HTTP_RESPONSE_COMPLETE`, `HTTP_CONNECTION_CLOSE`
- **Client methods**: `isPending()`, `isComplete()`, `close()`, etc.

## Benefits of Fixes

### 1. **Proper Separation of Concerns**
- Client code no longer depends on server-side types
- Clear distinction between client and server response handling
- No confusion about which types to use

### 2. **Configurable Timeout**
- Each client context can have its own idle timeout
- Default value of 30 seconds can be overridden
- More flexible for different use cases

### 3. **Cleaner Architecture**
- All state machine logic is properly merged
- No more references to non-existent types
- Consistent use of client-specific structures

### 4. **Better Maintainability**
- Clear separation between client and server code
- Easier to understand and modify
- Reduced coupling between components

## Usage Examples

### Configuring Idle Timeout
```cpp
uWS::ClientApp app;

// Set custom idle timeout for this client context
app.setIdleTimeout(60);  // 60 seconds instead of default 30

app.get("http://example.com/api", {
    .success = [](int status, std::string_view headers, 
                  std::string_view body, bool complete) {
        // Handle response
    }
});
```

### Client Response Handling
```cpp
// Now uses HttpClientResponse instead of HttpResponse
void onClientConnect(HttpClientResponse<SSL>* response, int is_client, std::string_view ip) {
    if (response->isPending()) {
        // Handle pending response
    }
    
    if (response->shouldClose()) {
        // Handle connection close
    }
}
```

## Technical Details

### State Machine Integration
The state machine is now fully integrated into `HttpClientContextData`:
- `ProcessingState` enum for state transitions
- All parsing state variables and methods
- Proper state management without external dependencies

### Timeout Configuration
- **Default**: 30 seconds
- **Configurable**: Per client context via `setIdleTimeout()`
- **Dynamic**: Can be changed at runtime
- **Per-connection**: Each connection uses the context's timeout setting

### Response Type Safety
- **Client-specific**: `HttpClientResponse` for client operations
- **Server-specific**: `HttpResponse` for server operations
- **No mixing**: Clear separation prevents type confusion
- **Compile-time safety**: Type system enforces proper usage

## Conclusion

The fixes successfully:

1. **Eliminated server dependencies** - Client code no longer uses server types
2. **Added configurable timeout** - Flexible idle timeout per client context
3. **Fixed state machine** - All references now use merged structure
4. **Improved type safety** - Clear separation between client and server types
5. **Enhanced maintainability** - Cleaner, more focused architecture

The HTTP client implementation now properly separates client and server concerns while providing flexible configuration options and maintaining all existing functionality.
