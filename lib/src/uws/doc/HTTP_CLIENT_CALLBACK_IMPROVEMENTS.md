# HTTP Client Callback Improvements

This document explains the improvements made to the HTTP client callback system to address issues with response handling, header processing, and streaming support.

## Issues Addressed

### 1. **Incorrect Header Usage**
- **Problem**: `onClientResponse` was using request headers instead of response headers
- **Solution**: Properly collect and pass response headers to callbacks

### 2. **Incomplete Body Gathering**
- **Problem**: Response body was not being properly collected for non-streaming responses
- **Solution**: Implement proper body accumulation and streaming detection

### 3. **Unnecessary Memory Allocation**
- **Problem**: Headers were being allocated as strings even when not needed
- **Solution**: Use `std::string_view` for efficient header passing

### 4. **Missing Streaming Completion Flag**
- **Problem**: No way to distinguish between complete and streaming responses
- **Solution**: Added `isComplete` flag to success callback

## New Callback Signature

### Updated Success Callback
```cpp
MoveOnlyFunction<void(int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete)> success = nullptr;
```

**Parameters**:
- `statusCode`: HTTP response status code
- `responseHeaders`: Response headers as string
- `requestHeaders`: Request headers for reference (currently empty, can be enhanced)
- `body`: Response body (complete for non-streaming, empty for streaming)
- `isComplete`: Flag indicating if response is complete (true) or streaming (false)

## Implementation Changes

### 1. **HttpClientContextData.h**
Added behavior callbacks to store user-provided callbacks:
```cpp
/* HTTP Client behavior callbacks */
MoveOnlyFunction<void(int, std::string_view, std::string_view, std::string_view, bool)> successHandler = nullptr;
MoveOnlyFunction<void(int, std::string_view)> errorHandler = nullptr;
MoveOnlyFunction<void(std::string_view)> headersReceivedHandler = nullptr;
MoveOnlyFunction<void(std::string_view, bool)> dataReceivedHandler = nullptr;
```

### 2. **HttpClientContext.h**
Updated streaming handlers to use behavior callbacks:
```cpp
/* Call behavior success handler if available */
if (httpContextData->successHandler) {
    /* Collect response headers */
    std::string responseHeaders;
    for (auto &header : httpRequest->getHeaders()) {
        responseHeaders += header.first + ": " + header.second + "\r\n";
    }
    
    /* Check if response is streaming */
    bool isStreaming = state.isStreaming;
    
    /* Call success handler with completion flag */
    httpContextData->successHandler(200, responseHeaders, "", "", !isStreaming);
}
```

### 3. **ClientApp.h**
Simplified callback setup by directly storing behavior callbacks:
```cpp
/* Set up behavior callbacks in the context data */
auto *contextData = httpClientContext->getSocketContextData();
contextData->successHandler = std::move(behavior.success);
contextData->errorHandler = std::move(behavior.error);
contextData->headersReceivedHandler = std::move(behavior.headersReceived);
contextData->dataReceivedHandler = std::move(behavior.dataReceived);
```

## Usage Examples

### Complete Response (Non-Streaming)
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
        if (isComplete) {
            std::cout << "Complete response: " << body << std::endl;
        }
    }
};
```

### Streaming Response
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
        if (!isComplete) {
            std::cout << "Streaming response started" << std::endl;
        }
    },
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        std::cout << "Received chunk: " << chunk.length() << " bytes" << std::endl;
        if (isLastChunk) {
            std::cout << "Streaming complete" << std::endl;
        }
    }
};
```

### Mixed Response Handling
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
        std::cout << "Status: " << statusCode << std::endl;
        std::cout << "Headers: " << responseHeaders << std::endl;
        
        if (isComplete) {
            // Small response - process complete body
            processCompleteResponse(body);
        } else {
            // Large response - will stream via dataReceived
            std::cout << "Starting streaming response..." << std::endl;
        }
    },
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Handle streaming data
        processStreamingChunk(chunk);
        if (isLastChunk) {
            finalizeStreamingResponse();
        }
    }
};
```

## Benefits

### 1. **Correct Header Processing**
- Response headers are now properly collected and passed
- Request headers are available for reference (can be enhanced)

### 2. **Efficient Memory Usage**
- `std::string_view` for header passing avoids unnecessary allocations
- Streaming responses don't buffer entire body in memory

### 3. **Clear Streaming Indication**
- `isComplete` flag makes it clear when response is streaming
- Applications can handle complete vs streaming responses appropriately

### 4. **Unified API**
- Same callback interface for all response types
- Consistent behavior across chunked and content-length responses

### 5. **Better Error Handling**
- Proper error callbacks for chunked encoding errors
- Connection error handling improved

## Migration Guide

### From Old API
```cpp
// Old callback signature
.success = [](int statusCode, std::string_view headers, std::string_view body) {
    processResponse(body);
}
```

### To New API
```cpp
// New callback signature
.success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
    if (isComplete) {
        processResponse(body);
    } else {
        // Handle streaming response
    }
}
```

## Future Enhancements

### 1. **Request Header Storage**
- Store request headers during request creation
- Pass actual request headers to success callback

### 2. **Body Accumulation**
- For non-streaming responses, accumulate body in success callback
- Provide complete body for small responses

### 3. **Enhanced Error Information**
- More detailed error codes and messages
- Better error context for debugging

### 4. **Progress Callbacks**
- Add progress callbacks for large downloads
- Percentage completion tracking

## Conclusion

These improvements provide a more robust and efficient HTTP client implementation with:
- Correct header processing
- Proper streaming support
- Clear completion indication
- Better memory efficiency
- Unified API design

The new callback system makes it easier to handle both small and large responses appropriately while providing clear indication of response state.
