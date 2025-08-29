# HTTP Parser Standardization Plan

## Overview

Currently, the HTTP client uses `picohttpparser.hpp` for response parsing while the server uses `HttpParser.h` for request parsing. This creates inconsistency and maintenance overhead. We should standardize on `HttpParser.h` for both requests and responses.

## Current State Analysis

### **HttpParser.h Strengths:**
- ✅ **Unified parsing**: Can handle both requests and responses
- ✅ **Chunked encoding**: Built-in support via `ChunkedEncoding.h`
- ✅ **Performance optimized**: SIMD operations, bloom filters
- ✅ **Robust error handling**: Comprehensive error states
- ✅ **Partial data handling**: Fallback buffering for incomplete data
- ✅ **Memory efficient**: Uses `std::string_view` for zero-copy operations

### **picohttpparser.hpp Limitations:**
- ❌ **External dependency**: Not part of uWebSockets codebase
- ❌ **Limited error handling**: Basic error states
- ❌ **No fallback buffering**: Requires manual buffer management
- ❌ **Separate maintenance**: Different codebase, different update cycles

## Proposed Solution

### **1. Extend HttpParser.h for Response Parsing**

#### **Create HttpResponseHeaders Structure:**
```cpp
struct HttpResponseHeaders {
    // Reuse the same header structure as HttpRequest
    struct Header {
        std::string_view key, value;
    } headers[UWS_HTTP_MAX_HEADERS_COUNT];
    size_t headerCount = 0;
    
    // Response-specific fields
    int statusCode = 0;
    std::string_view statusMessage;
    int minorVersion = 0;
    
    // Reuse existing methods
    Header* getHeaders() { return &headers[1]; }
    size_t getHeaderCount() { /* same as HttpRequest */ }
    std::string_view getHeader(std::string_view lowerCasedHeader) { /* same as HttpRequest */ }
    
    // Response-specific methods
    int getStatusCode() const { return statusCode; }
    std::string_view getStatusMessage() const { return statusMessage; }
    bool isChunked() const { return getHeader("transfer-encoding") == "chunked"; }
    bool hasContentLength() const { return getHeader("content-length").length() > 0; }
    uint64_t getContentLength() const { /* parse content-length header */ }
};
```

#### **Add Response Parsing to HttpParser:**
```cpp
struct HttpParser {
    // Add response parsing method
    static char* consumeResponseLine(char* data, char* end, HttpResponseHeaders::Header& header);
    
    // Extend getHeaders to handle both request and response
    static unsigned int getHeaders(char* postPaddedBuffer, char* end, 
                                 struct HttpRequest::Header* headers, 
                                 void* reserved, unsigned int& err,
                                 size_t* headerCount = nullptr,
                                 bool isResponse = false);
    
    // Add response-specific consume method
    std::pair<unsigned int, void*> consumeResponsePostPadded(char* data, unsigned int length,
        void* user, void* reserved, HttpResponseHeaders* resp,
        MoveOnlyFunction<void*(void*, HttpResponseHeaders*)>& responseHandler,
        MoveOnlyFunction<void*(void*, std::string_view, bool)>& dataHandler);
};
```

### **2. Update HttpClientContext to Use HttpParser**

#### **Replace picohttpparser.hpp Usage:**
```cpp
// Current (picohttpparser.hpp)
#include "picohttpparser.hpp"
int parsed = phr_parse_response(data, length, &minor_version, &status, &msg, &msg_len,
                               headers, &num_headers, offset);

// Proposed (HttpParser.h)
#include "HttpParser.h"
HttpResponseHeaders resp;
HttpParser parser;
auto [consumed, user] = parser.consumeResponsePostPadded(data, length, user, reserved,
    &resp, responseHandler, dataHandler);
```

#### **Update HttpClientContextData:**
```cpp
template <bool SSL>
struct HttpClientContextData {
    // Replace picohttpparser state
    // Remove: phr_chunked_decoder chunkedDecoder;
    // Remove: http_header headers[MAX_HEADERS];
    
    // Add HttpParser state
    HttpParser parser;
    HttpResponseHeaders responseHeaders;
    
    // Keep existing state machine
    ProcessingState currentState = ProcessingState::PARSING_HEADERS;
    // ... other existing fields
};
```

### **3. Implementation Steps**

#### **Phase 1: Extend HttpParser.h**
1. **Add HttpResponseHeaders structure** to `HttpParser.h`
2. **Add response line parsing** (`consumeResponseLine`)
3. **Extend getHeaders** to handle response parsing
4. **Add consumeResponsePostPadded** method
5. **Add response-specific error handling**

#### **Phase 2: Update HttpClientContext**
1. **Replace picohttpparser.hpp includes** with `HttpParser.h`
2. **Update HttpClientContextData** to use `HttpParser` and `HttpResponseHeaders`
3. **Refactor processHeadersPhase** to use new parser
4. **Update chunked encoding handling** to use `ChunkedEncoding.h`
5. **Remove picohttpparser.hpp dependency**

#### **Phase 3: Testing and Validation**
1. **Unit tests** for response parsing
2. **Integration tests** with real HTTP servers
3. **Performance benchmarks** vs picohttpparser.hpp
4. **Memory usage analysis**
5. **Error handling validation**

### **4. Benefits of Standardization**

#### **Code Consistency:**
- **Single parser**: Both client and server use same parsing logic
- **Unified error handling**: Consistent error codes and states
- **Shared optimizations**: Performance improvements benefit both sides

#### **Maintenance Benefits:**
- **Reduced dependencies**: No external HTTP parser dependency
- **Easier debugging**: Single codebase to understand and debug
- **Faster updates**: No waiting for external library updates

#### **Performance Benefits:**
- **Optimized for uWebSockets**: Parser designed specifically for this use case
- **Better memory usage**: Integrated fallback buffering
- **SIMD optimizations**: Already optimized for performance

#### **Feature Parity:**
- **Chunked encoding**: Native support via `ChunkedEncoding.h`
- **Compression detection**: Can add compression flags easily
- **Header validation**: Bloom filter for efficient header lookup

### **5. Migration Strategy**

#### **Backward Compatibility:**
- **Gradual migration**: Keep picohttpparser.hpp as fallback initially
- **Feature flags**: Allow switching between parsers
- **A/B testing**: Compare performance and reliability

#### **Risk Mitigation:**
- **Extensive testing**: Ensure all edge cases are covered
- **Performance validation**: Ensure new parser is at least as fast
- **Rollback plan**: Easy to revert if issues arise

### **6. Implementation Details**

#### **Response Line Parsing:**
```cpp
static inline char* consumeResponseLine(char* data, char* end, HttpResponseHeaders::Header& header) {
    // Parse: "HTTP/1.1 200 OK\r\n"
    // Extract: version, status code, status message
    // Similar to consumeRequestLine but for responses
}
```

#### **Header Parsing Extension:**
```cpp
static unsigned int getHeaders(char* postPaddedBuffer, char* end, 
                             struct HttpRequest::Header* headers, 
                             void* reserved, unsigned int& err,
                             size_t* headerCount = nullptr,
                             bool isResponse = false) {
    if (isResponse) {
        // Parse response line instead of request line
        if ((char*)2 > (postPaddedBuffer = consumeResponseLine(postPaddedBuffer, end, headers[0]))) {
            err = postPaddedBuffer ? HTTP_ERROR_505_HTTP_VERSION_NOT_SUPPORTED : 0;
            return 0;
        }
    } else {
        // Existing request line parsing
        if ((char*)2 > (postPaddedBuffer = consumeRequestLine(postPaddedBuffer, end, headers[0]))) {
            err = postPaddedBuffer ? HTTP_ERROR_505_HTTP_VERSION_NOT_SUPPORTED : 0;
            return 0;
        }
    }
    
    // Rest of header parsing is the same for both requests and responses
    // ... existing header parsing logic
}
```

#### **Chunked Encoding Integration:**
```cpp
// Replace picohttpparser chunked decoder with ChunkedEncoding.h
if (resp.isChunked()) {
    std::string_view dataToConsume(data, length);
    for (auto chunk : uWS::ChunkIterator(&dataToConsume, &remainingStreamingBytes)) {
        dataHandler(user, chunk, chunk.length() == 0);
    }
    if (isParsingInvalidChunkedEncoding(remainingStreamingBytes)) {
        return {HTTP_ERROR_400_BAD_REQUEST, FULLPTR};
    }
}
```

## Conclusion

Standardizing on `HttpParser.h` for both HTTP requests and responses will provide:

1. **Unified codebase**: Single parser for all HTTP parsing needs
2. **Better performance**: Optimized specifically for uWebSockets
3. **Reduced maintenance**: No external dependencies
4. **Enhanced features**: Better error handling, chunked encoding, etc.
5. **Future-proof**: Easier to add new HTTP features

The migration can be done gradually with proper testing to ensure reliability and performance.
