# Client Context Management Optimization

This document explains the optimization from `std::vector` to `std::unordered_map` for managing HTTP and WebSocket client contexts in `ClientApp.h`.

## Problem with std::vector Approach

### Original Implementation Issues

The original implementation used `std::vector` for storing contexts:

```cpp
std::vector<HttpClientContext<SSL> *> httpClientContexts;
std::vector<WebSocketContext<SSL, false, int> *> webSocketContexts;
std::vector<MoveOnlyFunction<void()>> contextDeleters;
```

### Performance Problems

1. **O(n) Removal Operations**: When contexts are closed or removed, finding and removing them from the vector requires O(n) time
2. **Fragmentation**: As contexts are added and removed, the vector becomes fragmented with gaps
3. **Inefficient Iteration**: When closing all connections, we iterate through potentially many inactive contexts
4. **Memory Inefficiency**: Vectors maintain contiguous memory, which can be wasteful when contexts are frequently added/removed

### Example Scenario

```cpp
// With std::vector - O(n) removal
for (auto it = httpClientContexts.begin(); it != httpClientContexts.end(); ++it) {
    if (*it == contextToRemove) {
        httpClientContexts.erase(it);  // O(n) operation
        break;
    }
}
```

## Solution: std::unordered_map Approach

### Optimized Implementation

The new implementation uses `std::unordered_map` with unique identifiers:

```cpp
std::unordered_map<unsigned int, HttpClientContext<SSL> *> httpClientContexts;
std::unordered_map<unsigned int, WebSocketContext<SSL, false, int> *> webSocketContexts;
std::unordered_map<unsigned int, MoveOnlyFunction<void()>> contextDeleters;

/* Context ID counter for unique identification */
unsigned int nextContextId = 1;
```

### Performance Benefits

1. **O(1) Lookup and Removal**: Direct access to contexts by ID
2. **No Fragmentation**: Maps handle sparse data efficiently
3. **Efficient Iteration**: Only active contexts are stored
4. **Memory Efficiency**: Maps only allocate space for active contexts

### Example Usage

```cpp
// Generate unique context ID
unsigned int contextId = nextContextId++;

// Store context with O(1) insertion
httpClientContexts[contextId] = httpClientContext;
contextDeleters[contextId] = [httpClientContext]() {
    httpClientContext->free();
};

// Remove context with O(1) lookup and removal
bool removeHttpContext(unsigned int contextId) {
    auto httpIt = httpClientContexts.find(contextId);
    if (httpIt != httpClientContexts.end()) {
        us_socket_context_close(SSL, (struct us_socket_context_t *) httpIt->second);
        httpClientContexts.erase(httpIt);  // O(1) operation
        contextDeleters.erase(contextId);  // O(1) operation
        return true;
    }
    return false;
}
```

## Implementation Details

### Context ID Generation

```cpp
/* Generate unique context ID */
unsigned int contextId = nextContextId++;
```

- Simple incrementing counter ensures unique IDs
- No collision detection needed
- Efficient and thread-safe for single-threaded event loop

### Context Storage

```cpp
/* Store the context for cleanup */
contextDeleters[contextId] = [httpClientContext]() {
    httpClientContext->free();
};

httpClientContexts[contextId] = httpClientContext;
```

- Each context gets a unique ID
- Context and its deleter are stored with the same ID
- O(1) insertion and lookup

### Context Removal

```cpp
bool removeHttpContext(unsigned int contextId) {
    auto httpIt = httpClientContexts.find(contextId);
    if (httpIt != httpClientContexts.end()) {
        /* Close the context */
        us_socket_context_close(SSL, (struct us_socket_context_t *) httpIt->second);
        /* Remove from maps */
        httpClientContexts.erase(httpIt);
        contextDeleters.erase(contextId);
        return true;
    }
    return false;
}
```

- O(1) lookup using `find()`
- O(1) removal using `erase()`
- Proper cleanup of both context and deleter

### Iteration Patterns

```cpp
/* Close all connections */
TemplatedClientApp &&closeAll() {
    for (auto &[id, context] : httpClientContexts) {
        us_socket_context_close(SSL, (struct us_socket_context_t *) context);
    }
    for (auto &[id, context] : webSocketContexts) {
        us_socket_context_close(SSL, (struct us_socket_context_t *) context);
    }
    httpClientContexts.clear();
    webSocketContexts.clear();
    contextDeleters.clear();
    return std::move(static_cast<TemplatedClientApp &&>(*this));
}
```

- Structured bindings for clean iteration
- Only active contexts are processed
- Efficient clearing of all maps

## Performance Comparison

### Time Complexity

| Operation | std::vector | std::unordered_map |
|-----------|-------------|-------------------|
| Insertion | O(1) amortized | O(1) average |
| Lookup | O(n) | O(1) average |
| Removal | O(n) | O(1) average |
| Iteration | O(n) | O(n) |

### Space Complexity

| Operation | std::vector | std::unordered_map |
|-----------|-------------|-------------------|
| Memory Usage | O(n) with fragmentation | O(n) efficient |
| Fragmentation | High | Low |
| Cache Locality | Good | Moderate |

### Real-World Impact

For applications with many concurrent connections:

- **100 connections**: Minimal difference
- **1,000 connections**: Noticeable improvement
- **10,000+ connections**: Significant performance gain

## Benefits Summary

### 1. **O(1) Operations**
- Fast context lookup and removal
- Efficient connection management
- Scalable to large numbers of connections

### 2. **No Fragmentation**
- Maps handle sparse data efficiently
- Memory usage scales with active connections
- No need for compaction or reallocation

### 3. **Better Resource Management**
- Individual context removal capability
- Proper cleanup of resources
- Reduced memory overhead

### 4. **Improved Scalability**
- Performance doesn't degrade with connection count
- Efficient for high-concurrency applications
- Better for long-running applications

### 5. **Enhanced API**
- Individual context management
- Unique identification of contexts
- Better error handling and debugging

## Migration Impact

### Backward Compatibility
- Public API remains unchanged
- Internal implementation optimization only
- No breaking changes for users

### Performance Gains
- Immediate improvement for applications with many connections
- Better resource utilization
- Reduced memory fragmentation

### Future Enhancements
- Individual context monitoring
- Connection pooling capabilities
- Advanced connection management features

## Conclusion

The optimization from `std::vector` to `std::unordered_map` provides significant performance improvements for client context management:

- **O(1) operations** for context management
- **Efficient memory usage** with no fragmentation
- **Better scalability** for high-concurrency applications
- **Enhanced API** for individual context control

This change makes the client application more suitable for production environments with many concurrent connections while maintaining the same public API.
