# HTTP Client Streaming Requests

## Overview

Added support for streaming HTTP request data, allowing efficient handling of large request bodies by sending data in chunks rather than loading the entire request into memory at once.

## Key Features

### 1. **Streaming Request Methods**

**New Methods Added:**
- `postStream()` - Streaming POST request
- `putStream()` - Streaming PUT request  
- `patchStream()` - Streaming PATCH request
- `requestStream()` - Generic streaming request

### 2. **Request Streaming Callbacks**

**New Callbacks in `HttpClientBehavior`:**
```cpp
MoveOnlyFunction<void()> requestReady = nullptr;    // Called when ready to send request data
MoveOnlyFunction<void()> requestComplete = nullptr; // Called when request data is complete
```

### 3. **Streaming Request Structure**

**New Structure:**
```cpp
struct HttpClientStreamingRequest {
    std::string method = "POST";
    std::string url;
    std::map<std::string, std::string> headers;
    HttpClientBehavior behavior;
    
    /* Streaming request data */
    bool isStreaming = true;
    unsigned int contentLength = 0;  // 0 means chunked transfer encoding
};
```

## Usage Examples

### 1. **Basic Streaming POST Request**

```cpp
app.postStream("http://example.com/upload", {
    .requestReady = []() {
        std::cout << "Ready to send request data" << std::endl;
    },
    .requestComplete = []() {
        std::cout << "Request data complete" << std::endl;
    },
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        std::cout << "Response status: " << status << std::endl;
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        std::cout << "Received response chunk: " << chunk.size() << " bytes" << std::endl;
        if (isLastChunk) {
            std::cout << "Response complete" << std::endl;
        }
    }
});
```

### 2. **Streaming Large File Upload**

```cpp
app.postStream("http://example.com/upload", {
    .requestReady = []() {
        std::cout << "Starting file upload..." << std::endl;
    },
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        if (status == 200) {
            std::cout << "Upload successful" << std::endl;
        }
    }
});

// Get the context and send data in chunks
auto* context = app.getHttpContext(contextId);
if (context) {
    // Send headers first
    context->sendRequestHeaders("POST", "/upload", {
        {"Content-Type", "application/octet-stream"},
        {"Transfer-Encoding", "chunked"}
    });
    
    // Send file data in chunks
    std::ifstream file("large_file.dat", std::ios::binary);
    char buffer[8192];
    while (file.read(buffer, sizeof(buffer))) {
        context->sendRequestData(std::string_view(buffer, file.gcount()));
    }
    
    // Complete the request
    context->completeRequestData();
}
```

### 3. **Streaming JSON Data**

```cpp
app.postStream("http://example.com/api/data", {
    .requestReady = []() {
        std::cout << "Ready to send JSON data" << std::endl;
    },
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        std::cout << "Response status: " << status << std::endl;
    }
});

auto* context = app.getHttpContext(contextId);
if (context) {
    // Send headers
    context->sendRequestHeaders("POST", "/api/data", {
        {"Content-Type", "application/json"},
        {"Transfer-Encoding", "chunked"}
    });
    
    // Send JSON data in chunks
    std::string jsonData = generateLargeJsonData();
    size_t chunkSize = 4096;
    for (size_t i = 0; i < jsonData.length(); i += chunkSize) {
        size_t len = std::min(chunkSize, jsonData.length() - i);
        context->sendRequestData(std::string_view(jsonData.data() + i, len));
    }
    
    // Complete the request
    context->completeRequestData();
}
```

### 4. **Streaming with Content-Length**

```cpp
app.putStream("http://example.com/resource", {
    .requestReady = []() {
        std::cout << "Ready to send data" << std::endl;
    }
});

auto* context = app.getHttpContext(contextId);
if (context) {
    std::string data = "Large data content...";
    
    // Send headers with content-length
    context->sendRequestHeaders("PUT", "/resource", {
        {"Content-Type", "text/plain"},
        {"Content-Length", std::to_string(data.length())}
    });
    
    // Send data
    context->sendRequestData(data);
    
    // Complete the request
    context->completeRequestData();
}
```

## API Reference

### **HttpClientContext Methods**

#### `sendRequestHeaders(method, path, headers)`
Sends HTTP request headers and starts the streaming process.

**Parameters:**
- `method`: HTTP method (GET, POST, PUT, etc.)
- `path`: Request path
- `headers`: Optional map of HTTP headers

**Example:**
```cpp
context->sendRequestHeaders("POST", "/upload", {
    {"Content-Type", "application/octet-stream"},
    {"Transfer-Encoding", "chunked"}
});
```

#### `sendRequestData(data)`
Sends a chunk of request data.

**Parameters:**
- `data`: Data chunk as `std::string_view`

**Example:**
```cpp
context->sendRequestData("Hello, World!");
```

#### `completeRequestData()`
Signals that request data streaming is complete.

**Example:**
```cpp
context->completeRequestData();
```

### **TemplatedClientApp Methods**

#### `getHttpContext(contextId)`
Retrieves an HTTP client context by ID for streaming operations.

**Parameters:**
- `contextId`: Context ID returned from request methods

**Returns:**
- `HttpClientContext<SSL>*` or `nullptr` if not found

**Example:**
```cpp
auto* context = app.getHttpContext(contextId);
if (context) {
    context->sendRequestHeaders("POST", "/upload");
}
```

## Streaming Flow

### 1. **Request Setup**
```cpp
app.postStream("http://example.com/upload", behavior);
```

### 2. **Connection Established**
- HTTP client context is created
- Connection to server is established
- `requestReady` callback is called when ready

### 3. **Send Headers**
```cpp
context->sendRequestHeaders("POST", "/upload", headers);
```

### 4. **Send Data Chunks**
```cpp
context->sendRequestData(chunk1);
context->sendRequestData(chunk2);
// ... more chunks
```

### 5. **Complete Request**
```cpp
context->completeRequestData();
```

### 6. **Receive Response**
- `onHeaders` called with response headers
- `onChunk` called for each response chunk
- `isLastChunk` flag indicates completion

## Error Handling

### **Request Errors**
```cpp
app.postStream("http://example.com/upload", {
    .onError = [](int errorCode, std::string_view errorMessage) {
        std::cout << "Request error: " << errorCode << " - " << errorMessage << std::endl;
    }
});
```

### **Streaming Errors**
- Headers not sent before data: Error callback triggered
- Connection lost during streaming: Error callback triggered
- Invalid context ID: Returns `nullptr`

## Performance Benefits

### **Memory Efficiency**
- **No large buffers**: Data sent in chunks, not accumulated
- **Reduced memory usage**: Only current chunk in memory
- **Scalable**: Handles arbitrarily large requests

### **Network Efficiency**
- **Immediate sending**: Data sent as soon as available
- **Backpressure handling**: Built-in flow control
- **Connection reuse**: Keep-alive connections supported

### **User Experience**
- **Progress tracking**: Can track upload progress
- **Cancellation support**: Can abort streaming requests
- **Real-time feedback**: Immediate response processing

## Best Practices

### 1. **Chunk Size Selection**
```cpp
// Good: Reasonable chunk sizes
const size_t CHUNK_SIZE = 8192;  // 8KB chunks

// Avoid: Too small or too large chunks
const size_t BAD_CHUNK_SIZE = 1;        // Too small
const size_t BAD_CHUNK_SIZE2 = 1000000; // Too large
```

### 2. **Error Handling**
```cpp
auto* context = app.getHttpContext(contextId);
if (!context) {
    std::cerr << "Context not found" << std::endl;
    return;
}

// Always check for errors
if (!context->sendRequestHeaders("POST", "/upload")) {
    std::cerr << "Failed to send headers" << std::endl;
    return;
}
```

### 3. **Resource Cleanup**
```cpp
// Clean up contexts when done
app.removeHttpContext(contextId);
```

### 4. **Content-Type Headers**
```cpp
// Always set appropriate content-type
context->sendRequestHeaders("POST", "/upload", {
    {"Content-Type", "application/json"},
    {"Transfer-Encoding", "chunked"}
});
```

## Migration from Regular Requests

### **Before (Regular Request)**
```cpp
std::string largeData = generateLargeData();
app.post("http://example.com/upload", largeData, {
    .onSuccess = [](int status, std::string_view headers, 
                   std::string_view requestHeaders, std::string_view body, bool complete) {
        // Handle response
    }
});
```

### **After (Streaming Request)**
```cpp
app.postStream("http://example.com/upload", {
    .requestReady = []() {
        // Start streaming data
    },
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        // Handle response headers
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Handle response chunks
    }
});

// Stream data in chunks
auto* context = app.getHttpContext(contextId);
context->sendRequestHeaders("POST", "/upload");
context->sendRequestData(generateLargeData());
context->completeRequestData();
```

## Conclusion

The streaming request functionality provides:

1. **Memory efficiency** - No need to load entire requests into memory
2. **Performance** - Immediate data transmission and processing
3. **Scalability** - Handle arbitrarily large requests
4. **Flexibility** - Support for various content types and transfer encodings
5. **Error handling** - Comprehensive error detection and reporting

This makes the HTTP client suitable for high-performance applications requiring efficient handling of large data transfers.
