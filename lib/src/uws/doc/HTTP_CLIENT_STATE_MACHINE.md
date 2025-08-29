# HTTP Client State Machine Implementation

## Overview

The HTTP client implementation now uses a proper state machine to separate the different phases of HTTP response processing: header parsing, content chunking, and content streaming. All state is managed within the `StreamingState` structure to ensure proper isolation between connections.

## State Machine Design

### Processing States

```cpp
enum class ProcessingState {
    PARSING_HEADERS,    // Parsing HTTP response headers
    PROCESSING_CHUNKED, // Processing chunked transfer encoding
    PROCESSING_CONTENT, // Processing content-length based content
    COMPLETE,           // Response processing complete
    ERROR               // Error state
};
```

### State Transitions

```
PARSING_HEADERS → PROCESSING_CHUNKED (if chunked encoding)
PARSING_HEADERS → PROCESSING_CONTENT (if content-length)
PARSING_HEADERS → COMPLETE (if no content)
PROCESSING_CHUNKED → COMPLETE (when chunked data complete)
PROCESSING_CONTENT → COMPLETE (when content-length reached)
Any State → ERROR (on parsing/processing errors)
```

## Enhanced StreamingState Structure

### Complete State Management

```cpp
struct StreamingState {
    /* State machine for HTTP response processing */
    enum class ProcessingState {
        PARSING_HEADERS,    // Parsing HTTP response headers
        PROCESSING_CHUNKED, // Processing chunked transfer encoding
        PROCESSING_CONTENT, // Processing content-length based content
        COMPLETE,           // Response processing complete
        ERROR               // Error state
    };
    
    ProcessingState currentState = ProcessingState::PARSING_HEADERS;
    
    /* Basic streaming flags */
    bool isChunked = false;
    bool isStreaming = false;
    bool enableStreaming = true;
    
    /* Buffer for incomplete data */
    std::string buffer;
    
    /* Content-length tracking */
    unsigned int contentLength = 0;
    unsigned int receivedBytes = 0;
    unsigned int maxChunkSize = 64 * 1024;
    
    /* picohttpparser chunked decoder state */
    phr_chunked_decoder chunkedDecoder = {0};
    
    /* HTTP response parsing state */
    bool headersParsed = false;
    int responseStatus = 0;
    int responseMinorVersion = 0;
    std::string responseMessage;
    
    /* Header parsing state */
    static const size_t MAX_HEADERS = 64;
    http_header headers[MAX_HEADERS];
    size_t numHeaders = 0;
    
    /* Header parsing progress */
    size_t headerParseOffset = 0;  // How much of the buffer has been parsed for headers
    
    /* Legacy chunked state (for compatibility) */
    uint64_t chunkedState = 0;
    
    /* State machine methods */
    void transitionTo(ProcessingState newState);
    bool isInState(ProcessingState state) const;
    void reset();
};
```

## State Machine Implementation

### Main Processing Function

```cpp
static void processHttpResponseData(HttpClientContextData<SSL>* contextData, 
                                   us_socket_t* s, char* data, int length) {
    auto& state = contextData->streamingState;
    
    switch (state.currentState) {
        case ProcessingState::PARSING_HEADERS:
            processHeadersPhase(contextData, s, data, length);
            break;
            
        case ProcessingState::PROCESSING_CHUNKED:
            processChunkedPhase(contextData, s, data, length);
            break;
            
        case ProcessingState::PROCESSING_CONTENT:
            processContentPhase(contextData, s, data, length);
            break;
            
        case ProcessingState::COMPLETE:
            /* Response already complete, ignore additional data */
            break;
            
        case ProcessingState::ERROR:
            /* In error state, ignore data */
            break;
    }
}
```

### Phase 1: Header Parsing

```cpp
static void processHeadersPhase(HttpClientContextData<SSL>* contextData, 
                               us_socket_t* s, char* data, int length) {
    auto& state = contextData->streamingState;
    
    /* Add new data to buffer */
    state.buffer.append(data, length);
    
    /* Try to parse headers from accumulated buffer */
    size_t num_headers = state.MAX_HEADERS;
    int minor_version, status;
    const char* msg;
    size_t msg_len;
    
    bool has_connection, has_close, has_chunked, has_content_length;
    int64_t content_length;
    
    int parsed = phr_parse_response(
        state.buffer.data(), state.buffer.length(), &minor_version, &status, &msg, &msg_len,
        state.headers, &num_headers, state.headerParseOffset,
        has_connection, has_close, has_chunked, has_content_length, content_length
    );
    
    if (parsed > 0) {
        /* Headers successfully parsed */
        state.responseStatus = status;
        state.responseMinorVersion = minor_version;
        state.responseMessage = std::string(msg, msg_len);
        state.numHeaders = num_headers;
        state.headersParsed = true;
        state.headerParseOffset = parsed;
        
        /* Determine next state based on response type */
        if (has_chunked) {
            state.isChunked = true;
            state.isStreaming = true;
            state.transitionTo(ProcessingState::PROCESSING_CHUNKED);
        } else if (has_content_length && content_length > 0) {
            state.contentLength = (unsigned int)content_length;
            if (state.enableStreaming && state.contentLength > state.maxChunkSize) {
                state.isStreaming = true;
                state.transitionTo(ProcessingState::PROCESSING_CONTENT);
            } else {
                /* Small content, accumulate in buffer */
                state.transitionTo(ProcessingState::PROCESSING_CONTENT);
            }
        } else {
            /* No content-length, assume complete */
            state.transitionTo(ProcessingState::COMPLETE);
        }
        
        /* Process any remaining data after headers */
        if (parsed < (int)state.buffer.length()) {
            char* remainingData = (char*)state.buffer.data() + parsed;
            int remainingLength = (int)(state.buffer.length() - parsed);
            
            /* Process remaining data according to new state */
            switch (state.currentState) {
                case ProcessingState::PROCESSING_CHUNKED:
                    processChunkedPhase(contextData, s, remainingData, remainingLength);
                    break;
                case ProcessingState::PROCESSING_CONTENT:
                    processContentPhase(contextData, s, remainingData, remainingLength);
                    break;
                default:
                    break;
            }
        }
        
    } else if (parsed == -1) {
        /* Parse error */
        state.transitionTo(ProcessingState::ERROR);
        if (contextData->errorHandler) {
            contextData->errorHandler(-1, "HTTP response parse error");
        }
    }
    /* parsed == -2 means incomplete, continue accumulating data */
}
```

### Phase 2: Chunked Processing

```cpp
static void processChunkedPhase(HttpClientContextData<SSL>* contextData, 
                               us_socket_t* s, char* data, int length) {
    auto& state = contextData->streamingState;
    
    /* Use picohttpparser's chunked decoder */
    size_t bufsz = length;
    
    ssize_t ret = phr_decode_chunked(&state.chunkedDecoder, data, &bufsz);
    
    if (ret >= 0) {
        /* Chunked response complete */
        if (contextData->dataReceivedHandler) {
            contextData->dataReceivedHandler(std::string_view(data, bufsz), true);
        }
        state.transitionTo(ProcessingState::COMPLETE);
        
        /* Call success handler */
        if (contextData->successHandler) {
            std::string responseHeaders;
            for (size_t i = 0; i < state.numHeaders; i++) {
                responseHeaders += std::string(state.headers[i].name) + ": " + 
                                 std::string(state.headers[i].value) + "\r\n";
            }
            contextData->successHandler(state.responseStatus, responseHeaders, "", "", true);
        }
        
    } else if (ret == -2) {
        /* Incomplete chunked data */
        if (contextData->dataReceivedHandler) {
            contextData->dataReceivedHandler(std::string_view(data, bufsz), false);
        }
    } else {
        /* Error in chunked decoding */
        state.transitionTo(ProcessingState::ERROR);
        if (contextData->errorHandler) {
            contextData->errorHandler(-1, "Invalid chunked encoding");
        }
    }
}
```

### Phase 3: Content Processing

```cpp
static void processContentPhase(HttpClientContextData<SSL>* contextData, 
                               us_socket_t* s, char* data, int length) {
    auto& state = contextData->streamingState;
    
    /* Add to received bytes */
    state.receivedBytes += length;
    
    /* Emit chunk data if streaming */
    if (state.isStreaming && contextData->dataReceivedHandler) {
        bool isLastChunk = (state.receivedBytes >= state.contentLength);
        contextData->dataReceivedHandler(std::string_view(data, length), isLastChunk);
    }
    
    /* Check if response is complete */
    if (state.receivedBytes >= state.contentLength) {
        state.transitionTo(ProcessingState::COMPLETE);
        
        /* Call success handler */
        if (contextData->successHandler) {
            std::string responseHeaders;
            for (size_t i = 0; i < state.numHeaders; i++) {
                responseHeaders += std::string(state.headers[i].name) + ": " + 
                                 std::string(state.headers[i].value) + "\r\n";
            }
            
            std::string body;
            if (!state.isStreaming) {
                /* For non-streaming responses, use accumulated buffer */
                body = state.buffer.substr(state.headerParseOffset);
            }
            
            contextData->successHandler(state.responseStatus, responseHeaders, "", body, true);
        }
    }
}
```

## Benefits of State Machine Approach

### 1. **Clear Separation of Concerns**
- Header parsing is isolated from content processing
- Chunked encoding has its own dedicated phase
- Content streaming is handled separately from header parsing

### 2. **Proper State Management**
- Each connection maintains its own state
- State transitions are explicit and controlled
- Error states are properly handled

### 3. **Improved Data Flow**
- Data is processed according to current state
- Remaining data after header parsing is handled correctly
- Buffer management is state-aware

### 4. **Better Error Handling**
- Parse errors transition to ERROR state
- Invalid chunked encoding is caught and handled
- State machine prevents processing in invalid states

### 5. **Enhanced Debugging**
- Clear state transitions make debugging easier
- State-specific processing logic is isolated
- Error conditions are clearly identified

## Usage Example

```cpp
// HTTP Client with state machine
uWS::ClientApp app;

app.get("http://example.com/api/data", {
    .success = [](int statusCode, std::string_view responseHeaders, 
                  std::string_view requestHeaders, std::string_view body, bool isComplete) {
        std::cout << "Status: " << statusCode << std::endl;
        std::cout << "Headers: " << responseHeaders << std::endl;
        if (isComplete) {
            std::cout << "Complete body: " << body << std::endl;
        }
    },
    .dataReceived = [](std::string_view chunk, bool isLastChunk) {
        std::cout << "Received chunk: " << chunk.length() << " bytes" << std::endl;
        if (isLastChunk) {
            std::cout << "Streaming complete" << std::endl;
        }
    },
    .headersReceived = [](std::string_view headers) {
        std::cout << "Headers received: " << headers << std::endl;
    },
    .error = [](int errorCode, std::string_view errorMessage) {
        std::cerr << "Error: " << errorCode << " - " << errorMessage << std::endl;
    }
});
```

## State Machine Flow

### Normal Flow (Content-Length)
1. **PARSING_HEADERS**: Accumulate data, parse headers
2. **PROCESSING_CONTENT**: Process content data, emit chunks if streaming
3. **COMPLETE**: Response complete, call success handler

### Chunked Flow
1. **PARSING_HEADERS**: Accumulate data, parse headers
2. **PROCESSING_CHUNKED**: Process chunked data using picohttpparser
3. **COMPLETE**: Chunked response complete, call success handler

### Error Flow
1. Any state → **ERROR**: On parsing or processing errors
2. **ERROR**: Ignore additional data, call error handler

## Conclusion

The state machine implementation provides:

- **Clear separation** between header parsing and content processing
- **Proper state isolation** for each HTTP client connection
- **Robust error handling** with explicit error states
- **Improved data flow** with state-aware processing
- **Better debugging** capabilities with clear state transitions

This approach makes the HTTP client implementation more maintainable, reliable, and suitable for production use with complex HTTP responses including chunked encoding and streaming content.
