# HTTP Client Pipelining Support in HttpClientContext

## Overview

The `HttpClientContext` has been enhanced to fully support HTTP pipelining, allowing multiple HTTP responses to be processed efficiently on a single connection. This implementation works seamlessly with the enhanced `HttpParser::consumeResponsePostPadded` method to handle pipelined responses.

## Key Pipelining Features

### 1. **Automatic Response State Reset**

When a response completes, the context automatically resets for the next response:

```cpp
if (isLast) {
    /* Response complete - reset state for next response in pipeline */
    ctx->transitionTo(HttpClientContextData<SSL>::ProcessingState::PARSING_HEADERS);
    ctx->headersParsed = false;
    ctx->responseStatus = 0;
    ctx->responseMinorVersion = 0;
    ctx->responseHeaders = HttpResponseHeaders();  /* Reset headers for next response */
}
```

### 2. **Smart Buffer Management**

The context intelligently handles buffered data for pipelining:

```cpp
/* Process any buffered data first for pipelining */
if (!contextData->buffer.empty()) {
    /* Combine buffered data with new data */
    contextData->buffer.append(data, length);
    
    /* Process the combined data */
    processHeadersPhase(contextData, s, (char*)contextData->buffer.data(), (int)contextData->buffer.length());
    
    /* Clear the buffer after processing */
    contextData->buffer.clear();
    return;
}
```

### 3. **Immediate Response Detection**

When remaining data looks like a new HTTP response, it's processed immediately:

```cpp
/* Check if remaining data looks like the start of a new HTTP response */
if (remainingLength >= 4) {
    if ((remainingData[0] == 'H' && remainingData[1] == 'T' && remainingData[2] == 'T' && remainingData[3] == 'P') ||
        (remainingData[0] == 'h' && remainingData[1] == 't' && remainingData[2] == 't' && remainingData[3] == 'p')) {
        /* This looks like the start of a new response - process it immediately */
        processHeadersPhase(contextData, s, remainingData, remainingLength);
        return;
    }
}
```

### 4. **Complete State Handling**

Even in `COMPLETE` state, the context checks for pipelined data:

```cpp
case HttpClientContextData<SSL>::ProcessingState::COMPLETE:
    /* Response already complete, but check for pipelined data */
    processHeadersPhase(contextData, s, data, length);
    break;
```

## Pipelining Flow

### **Scenario 1: Multiple Complete Responses**

**Input Data:**
```
HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHelloHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nWorld
```

**Processing Flow:**
1. **First Call**: Parse first response
   - Parse headers and body for "Hello"
   - `isLast = true` → Reset state for next response
   - Detect remaining data starts with "HTTP"
   - Process second response immediately

2. **Second Response**: Parse "World" response
   - Parse headers and body for "World"
   - `isLast = true` → Reset state for next response
   - No more data → Complete

### **Scenario 2: Partial Response + Complete Response**

**Input Data:**
```
HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nPartialData...
```

**Processing Flow:**
1. **First Call**: Start content-length streaming
   - Parse headers, set `remainingStreamingBytes = 1000`
   - Process partial body
   - Buffer remaining data

2. **Second Call**: Continue streaming + next response
   - Combine buffered data with new data
   - Continue content-length streaming
   - When complete, detect and process next response

### **Scenario 3: Chunked Response + Next Response**

**Input Data:**
```
HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nWorld
```

**Processing Flow:**
1. **First Call**: Parse chunked response
   - Parse headers, set chunked state
   - Process chunked body: "4\r\ndata\r\n0\r\n\r\n"
   - `isLast = true` → Reset state
   - Detect next response starts with "HTTP"
   - Process second response immediately

## Implementation Details

### **State Machine Enhancements**

The state machine now properly handles pipelining:

```cpp
enum class ProcessingState {
    PARSING_HEADERS,    // Parsing HTTP response headers
    PROCESSING_CHUNKED, // Processing chunked transfer encoding
    PROCESSING_CONTENT, // Processing content-length based content
    COMPLETE,           // Response processing complete (but check for pipelined data)
    FAILURE             // Error state
};
```

### **Buffer Management Strategy**

```cpp
/* Handle remaining data for pipelining */
if (consumed < length) {
    int remainingLength = length - consumed;
    char* remainingData = data + consumed;
    
    /* Check if remaining data looks like the start of a new HTTP response */
    if (remainingLength >= 4) {
        if (/* looks like HTTP response start */) {
            /* Process immediately */
            processHeadersPhase(contextData, s, remainingData, remainingLength);
            return;
        }
    }
    
    /* Otherwise, accumulate in buffer for next call */
    contextData->buffer.append(remainingData, remainingLength);
}
```

### **Response Reset Logic**

```cpp
/* Reset context for next response in pipeline */
static void resetForNextResponse(HttpClientContextData<SSL>* contextData) {
    contextData->transitionTo(HttpClientContextData<SSL>::ProcessingState::PARSING_HEADERS);
    contextData->headersParsed = false;
    contextData->responseStatus = 0;
    contextData->responseMinorVersion = 0;
    contextData->responseHeaders = HttpResponseHeaders();
    contextData->buffer.clear();
}
```

## Benefits of HttpClientContext Pipelining

### **1. Performance**
- **Efficient Processing**: Multiple responses processed in single data call
- **Reduced Overhead**: No need to wait for complete responses
- **Better Throughput**: Higher data processing rates

### **2. Resource Efficiency**
- **Single Connection**: Multiple requests/responses on one connection
- **Memory Optimization**: Smart buffer management
- **CPU Efficiency**: Reduced context switching

### **3. Robustness**
- **Error Handling**: Proper error state management
- **State Recovery**: Automatic state reset between responses
- **Buffer Management**: Intelligent handling of partial data

### **4. Compatibility**
- **HTTP/1.1 Standard**: Full compliance with HTTP pipelining
- **Backward Compatible**: Works with non-pipelined responses
- **Server Agnostic**: Works with any HTTP/1.1 server

## Usage Patterns

### **Automatic Pipelining**

The client automatically handles pipelining without any special configuration:

```cpp
// Send multiple requests
client.get("http://example.com/api1", {
    .onHeaders = [](int status, auto headers, size_t count, auto ctx) {
        // Handle first response headers
    },
    .onChunk = [](std::string_view data, bool isLast) {
        // Handle first response data
        if (isLast) {
            // First response complete, second response will be processed automatically
        }
    }
});

client.get("http://example.com/api2", {
    .onHeaders = [](int status, auto headers, size_t count, auto ctx) {
        // Handle second response headers (processed automatically)
    },
    .onChunk = [](std::string_view data, bool isLast) {
        // Handle second response data
    }
});
```

### **Manual Pipelining Control**

For advanced use cases, you can control pipelining behavior:

```cpp
// Reset context manually if needed
HttpClientContextData<SSL>* contextData = client.getContextData();
HttpClientContext<SSL>::resetForNextResponse(contextData);
```

## Testing Scenarios

### **Test 1: Simple Pipelining**
```
Request: GET /api1\r\nGET /api2
Response: HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHelloHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nWorld
```

### **Test 2: Chunked Pipelining**
```
Request: GET /api1\r\nGET /api2
Response: HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nHello\r\n0\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nWorld
```

### **Test 3: Mixed Content Types**
```
Request: GET /api1\r\nGET /api2\r\nGET /api3
Response: HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHelloHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nWorld\r\n0\r\n\r\nHTTP/1.1 204 No Content\r\n\r\n
```

### **Test 4: Partial Responses**
```
Request: GET /api1\r\nGET /api2
Response: HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\nPartialData...
```

## Error Handling

### **Parsing Errors**
```cpp
/* Handle parsing errors */
if (user == FULLPTR) {
    contextData->transitionTo(HttpClientContextData<SSL>::ProcessingState::FAILURE);
    if (contextData->onError) {
        contextData->onError(-1, "HTTP response parse error");
    }
    return;
}
```

### **State Recovery**
- **Automatic Reset**: State automatically resets after response completion
- **Error Recovery**: Proper error state handling
- **Buffer Cleanup**: Buffer cleared after processing

## Conclusion

The enhanced `HttpClientContext` provides comprehensive HTTP pipelining support by:

1. **Automatic State Management**: Properly resets state between responses
2. **Smart Buffer Handling**: Intelligently manages partial data
3. **Immediate Processing**: Detects and processes new responses immediately
4. **Robust Error Handling**: Maintains proper error states
5. **Seamless Integration**: Works with existing callback patterns

This enables efficient HTTP client implementations that can take full advantage of HTTP pipelining for improved performance and resource utilization, while maintaining backward compatibility with non-pipelined HTTP responses.
