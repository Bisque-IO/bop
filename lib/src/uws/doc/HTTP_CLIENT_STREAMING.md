# HTTP Client Streaming Support

This document explains the comprehensive streaming support implemented in the HTTP client, which handles both chunked encoding and large content-length responses with a unified API.

## Overview

The HTTP client now supports streaming responses through a unified interface that works for:
- **Chunked Encoding**: `Transfer-Encoding: chunked` responses
- **Large Content-Length**: Responses larger than a configurable threshold
- **Small Responses**: Buffered responses for small content-length

## Streaming Architecture

### Unified Streaming API

All streaming responses use the same callback interface:

```cpp
MoveOnlyFunction<void(std::string_view chunk, bool isLastChunk)> dataReceived = nullptr;
```

The `isLastChunk` flag indicates when the final chunk has been received, allowing applications to:
- Process data incrementally
- Know when the response is complete
- Handle both streaming and buffered responses uniformly

### Streaming Configuration

```cpp
struct HttpClientBehavior {
    // Streaming settings
    unsigned int maxChunkSize = 64 * 1024;  // 64KB default
    bool enableStreaming = true;             // Enable streaming
    
    // Streaming callback
    MoveOnlyFunction<void(std::string_view chunk, bool isLastChunk)> dataReceived = nullptr;
};
```

## Response Types and Behavior

### 1. Chunked Encoding Responses

**Detection**: Automatically detected via `Transfer-Encoding: chunked` header

**Behavior**:
- Parses chunk size headers (hex format)
- Emits data chunks as they arrive
- Handles chunk boundaries automatically
- Sets `isLastChunk = true` on final chunk (size 0)

**Example**:
```cpp
uWS::HttpClientBehavior behavior = {
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        std::cout << "Chunk: " << chunk.length() << " bytes";
        if (isLastChunk) {
            std::cout << " (chunked response complete)";
        }
        std::cout << std::endl;
    }
};
```

### 2. Large Content-Length Responses

**Detection**: Content-Length > `maxChunkSize` (default 64KB)

**Behavior**:
- Automatically enables streaming mode
- Emits data in chunks as it arrives
- Tracks received bytes against content-length
- Sets `isLastChunk = true` when all data received

**Example**:
```cpp
uWS::HttpClientBehavior behavior = {
    .maxChunkSize = 32 * 1024,  // 32KB chunks
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Process large file download
        if (isLastChunk) {
            std::cout << "Download complete!" << std::endl;
        }
    }
};
```

### 3. Small Content-Length Responses

**Detection**: Content-Length ≤ `maxChunkSize`

**Behavior**:
- Buffers entire response in memory
- Calls `success` callback with complete body
- No streaming callbacks invoked

**Example**:
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view headers, std::string_view body) {
        // Small response - body contains complete data
        std::cout << "Complete response: " << body << std::endl;
    }
};
```

## Implementation Details

### Streaming State Management

The client maintains streaming state for each connection:

```cpp
struct StreamingState {
    bool isChunked = false;           // Chunked encoding detected
    bool isStreaming = false;         // Streaming mode active
    uint64_t chunkedState = 0;        // Chunked parsing state
    std::string buffer;               // Buffer for small responses
    unsigned int contentLength = 0;   // Expected content length
    unsigned int receivedBytes = 0;   // Bytes received so far
    unsigned int maxChunkSize = 64 * 1024;
    bool enableStreaming = true;
};
```

### Chunked Encoding Parser

Uses the existing `ChunkedEncoding.h` infrastructure:

```cpp
// Parse chunk size headers
consumeHexNumber(chunk, state.chunkedState);

// Check chunk size
unsigned int chunkSize = uWS::chunkSize(state.chunkedState);

// Decrement as data is consumed
decChunkSize(state.chunkedState, bytesConsumed);

// Check for completion
if (chunkSize == 0) {
    // End of chunked response
}
```

### Content-Length Tracking

For non-chunked responses:

```cpp
// Track received bytes
state.receivedBytes += length;

// Check for completion
bool isLastChunk = (state.receivedBytes >= state.contentLength);

// Emit chunk
dataReceived(chunk, isLastChunk);
```

## Usage Examples

### File Download with Progress

```cpp
uWS::HttpClientBehavior behavior = {
    .maxChunkSize = 16 * 1024,  // 16KB chunks for progress updates
    
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        static size_t totalBytes = 0;
        totalBytes += chunk.length();
        
        std::cout << "Downloaded: " << totalBytes << " bytes";
        if (isLastChunk) {
            std::cout << " (complete)";
        }
        std::cout << std::endl;
    }
};

app.get("http://example.com/large-file.zip", std::move(behavior));
```

### Streaming JSON Processing

```cpp
uWS::HttpClientBehavior behavior = {
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Process JSON chunks incrementally
        jsonBuffer.append(chunk);
        
        if (isLastChunk) {
            // Process complete JSON
            auto json = nlohmann::json::parse(jsonBuffer);
            processJson(json);
        }
    }
};
```

### Chunked API Response

```cpp
uWS::HttpClientBehavior behavior = {
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Handle streaming API response
        processApiChunk(chunk);
        
        if (isLastChunk) {
            finalizeApiResponse();
        }
    }
};
```

## Error Handling

### Chunked Encoding Errors

```cpp
if (isParsingInvalidChunkedEncoding(state.chunkedState)) {
    // Handle chunked encoding error
    if (contextData->error) {
        contextData->error(-1, "Invalid chunked encoding");
    }
}
```

### Connection Errors

```cpp
uWS::HttpClientBehavior behavior = {
    .error = [](int errorCode, std::string_view errorMessage) {
        std::cerr << "Streaming error: " << errorCode << " - " << errorMessage << std::endl;
    }
};
```

## Performance Considerations

### Memory Usage

- **Small responses**: Buffered in memory (≤ maxChunkSize)
- **Large responses**: Streamed with minimal buffering
- **Chunked responses**: Minimal buffering, immediate processing

### Network Efficiency

- **Keep-alive connections**: Reused for multiple requests
- **Chunked parsing**: Efficient hex parsing with minimal overhead
- **Streaming**: No need to buffer entire response

### Configuration Tuning

```cpp
// For memory-constrained environments
.maxChunkSize = 8 * 1024,    // 8KB chunks
.enableStreaming = true,

// For high-throughput scenarios
.maxChunkSize = 128 * 1024,  // 128KB chunks
.enableStreaming = true,
```

## Migration from Non-Streaming

### Before (Buffered Only)
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view headers, std::string_view body) {
        // Process complete response
        processResponse(body);
    }
};
```

### After (With Streaming)
```cpp
uWS::HttpClientBehavior behavior = {
    .success = [](int statusCode, std::string_view responseHeaders, std::string_view requestHeaders, std::string_view body, bool isComplete) {
        // Check if response is complete or streaming
        if (isComplete) {
            processResponse(body);
        } else {
            // Response is streaming, body will come via dataReceived
            std::cout << "Streaming response started" << std::endl;
        }
    },
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        // Large responses use streaming
        processChunk(chunk);
        if (isLastChunk) {
            finalizeProcessing();
        }
    }
};
```

## Conclusion

The unified streaming API provides:
- **Consistent interface** for all response types
- **Automatic detection** of streaming needs
- **Efficient memory usage** for large responses
- **Backward compatibility** with existing code
- **Flexible configuration** for different use cases

This implementation makes the HTTP client suitable for handling responses of any size efficiently, from small API responses to large file downloads.
