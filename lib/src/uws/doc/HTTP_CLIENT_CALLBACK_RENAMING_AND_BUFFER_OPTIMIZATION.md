# HTTP Client Callback Renaming and Buffer Optimization

## Overview

Renamed callback handlers to be more consistent and optimized buffer usage to minimize copying and utilize OS buffers more efficiently.

## Key Changes Made

### 1. **Callback Handler Renaming**

**Before:**
```cpp
// HttpClientContextData.h
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, std::string_view)> clientConnectHandler = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, void*)> clientDisconnectHandler = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*)> clientTimeoutHandler = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int)> clientConnectErrorHandler = nullptr;

MoveOnlyFunction<void(int, std::string_view, std::string_view, std::string_view, bool)> successHandler = nullptr;
MoveOnlyFunction<void(int, std::string_view)> errorHandler = nullptr;
MoveOnlyFunction<void(std::string_view)> headersReceivedHandler = nullptr;
MoveOnlyFunction<void(std::string_view, bool)> dataReceivedHandler = nullptr;
```

**After:**
```cpp
// HttpClientContextData.h
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, std::string_view)> onConnected = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, void*)> onDisconnected = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*)> onTimeout = nullptr;
MoveOnlyFunction<void(HttpClientResponse<SSL>*, int)> onConnectError = nullptr;

MoveOnlyFunction<void(int, std::string_view, std::string_view, std::string_view, bool)> onSuccess = nullptr;
MoveOnlyFunction<void(int, std::string_view)> onError = nullptr;
MoveOnlyFunction<void(std::string_view)> onHeaders = nullptr;
MoveOnlyFunction<void(std::string_view, bool)> onChunk = nullptr;
```

### 2. **Method Renaming in HttpClientContext**

**Before:**
```cpp
// HttpClientContext.h
void onClientConnect(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, std::string_view)>&& handler);
void onClientDisconnect(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, void*)>&& handler);
void onClientTimeout(MoveOnlyFunction<void(HttpClientResponse<SSL>*)>&& handler);
void onClientConnectError(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int)>&& handler);
```

**After:**
```cpp
// HttpClientContext.h
void onConnected(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, std::string_view)>&& handler);
void onDisconnected(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int, void*)>&& handler);
void onTimeout(MoveOnlyFunction<void(HttpClientResponse<SSL>*)>&& handler);
void onConnectError(MoveOnlyFunction<void(HttpClientResponse<SSL>*, int)>&& handler);
```

### 3. **Buffer Optimization**

**Before:**
```cpp
// HttpClientContextData.h
std::string responseMessage;  // Always copied from OS buffer

// HttpClientContext.h
contextData->responseMessage = std::string(msg, msg_len);  // Unnecessary copying
```

**After:**
```cpp
// HttpClientContextData.h
/* responseMessage is not stored - we use OS buffer directly when possible */

// HttpClientContext.h
/* Don't copy responseMessage - use OS buffer directly */
```

## Benefits of Changes

### 1. **Consistent Naming Convention**
- **Clearer intent**: `onConnected` vs `clientConnectHandler`
- **Consistent pattern**: All callbacks follow `onEvent` naming
- **Better readability**: More intuitive method names
- **Reduced verbosity**: Shorter, cleaner names

### 2. **Improved Buffer Efficiency**
- **Reduced memory allocation**: No unnecessary `responseMessage` string copying
- **OS buffer utilization**: Use OS-supplied buffers directly when possible
- **Single buffer strategy**: Use `buffer` only when partial data requires accumulation
- **Zero-copy optimization**: Minimize data copying between buffers

### 3. **Performance Improvements**
- **Less memory pressure**: Reduced string allocations
- **Faster processing**: No unnecessary copying of response messages
- **Better cache locality**: Fewer memory allocations and copies
- **Reduced GC pressure**: Less temporary string objects

## Technical Details

### Callback Usage Examples

**Before:**
```cpp
// Setting up callbacks
app.onClientConnect([](HttpClientResponse<SSL>* response, int is_client, std::string_view ip) {
    // Handle connection
});

app.get("http://example.com/api", {
    .successHandler = [](int status, std::string_view headers, 
                        std::string_view body, bool complete) {
        // Handle success
    },
    .dataReceivedHandler = [](std::string_view chunk, bool isLast) {
        // Handle streaming data
    }
});
```

**After:**
```cpp
// Setting up callbacks
app.onConnected([](HttpClientResponse<SSL>* response, int is_client, std::string_view ip) {
    // Handle connection
});

app.get("http://example.com/api", {
    .onSuccess = [](int status, std::string_view headers, 
                   std::string_view body, bool complete) {
        // Handle success
    },
    .onChunk = [](std::string_view chunk, bool isLast) {
        // Handle streaming data
    }
});
```

### Buffer Strategy

**Optimized Buffer Usage:**
```cpp
// Only use buffer when necessary for partial data
if (parsed == -2) {
    // Incomplete headers - accumulate in buffer
    contextData->buffer.append(data, length);
} else if (parsed > 0) {
    // Complete headers - process immediately without copying
    // Use OS buffer directly via string_view
    contextData->onHeaders(std::string_view(msg, msg_len));
}
```

**Single Buffer Approach:**
- **Primary buffer**: `contextData->buffer` for partial data accumulation
- **OS buffer utilization**: Use `std::string_view` to reference OS buffers directly
- **Minimal copying**: Only copy when partial data requires accumulation
- **Efficient streaming**: Pass data chunks directly without intermediate buffers

## Migration Impact

### ✅ **Breaking Changes**
- **Method names changed**: `onClientConnect` → `onConnected`, etc.
- **Callback names changed**: `successHandler` → `onSuccess`, etc.
- **User code updates required**: All callback assignments need updating

### ✅ **Performance Benefits**
- **Reduced memory allocations**: No more `responseMessage` copying
- **Better buffer utilization**: OS buffers used directly when possible
- **Improved streaming**: More efficient chunk processing

### ✅ **Code Quality**
- **Consistent naming**: All callbacks follow same pattern
- **Clearer intent**: Method names are more descriptive
- **Better maintainability**: Easier to understand and modify

## Usage Migration Guide

### 1. **Update Method Calls**
```cpp
// Before
app.onClientConnect(handler);
app.onClientDisconnect(handler);
app.onClientTimeout(handler);
app.onClientConnectError(handler);

// After
app.onConnected(handler);
app.onDisconnected(handler);
app.onTimeout(handler);
app.onConnectError(handler);
```

### 2. **Update Callback Assignments**
```cpp
// Before
app.get("http://example.com/api", {
    .successHandler = successCallback,
    .errorHandler = errorCallback,
    .headersReceivedHandler = headersCallback,
    .dataReceivedHandler = dataCallback
});

// After
app.get("http://example.com/api", {
    .onSuccess = successCallback,
    .onError = errorCallback,
    .onHeaders = headersCallback,
    .onChunk = dataCallback
});
```

## Conclusion

The changes successfully:

1. **Improved naming consistency** - All callbacks follow `onEvent` pattern
2. **Enhanced buffer efficiency** - Reduced unnecessary copying and memory allocations
3. **Better OS buffer utilization** - Use OS-supplied buffers directly when possible
4. **Optimized streaming performance** - More efficient chunk processing
5. **Clearer API design** - More intuitive and consistent method names

The HTTP client implementation now has a cleaner, more efficient API with better performance characteristics while maintaining all existing functionality.
