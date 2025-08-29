# HTTP Client Callback API Refactoring

## Overview

Refactored the HTTP client callback API to be more consistent and efficient by removing the redundant `onSuccess` callback and enhancing `onHeaders` with status and context pointer, while using `onChunk` for all data and completion handling.

## Key Changes Made

### 1. **Removed onSuccess Callback**

**Before:**
```cpp
// HttpClientContextData.h
MoveOnlyFunction<void(int, const http_header*, size_t, std::string_view, std::string_view, bool)> onSuccess = nullptr;
MoveOnlyFunction<void(const http_header*, size_t)> onHeaders = nullptr;
MoveOnlyFunction<void(std::string_view, bool)> onChunk = nullptr;

// Usage required both onHeaders and onSuccess
.onHeaders = [](const http_header* headers, size_t numHeaders) {
    // Handle headers
},
.onSuccess = [](int status, const http_header* headers, size_t numHeaders,
               std::string_view requestHeaders, std::string_view body, bool complete) {
    // Handle completion
}
```

**After:**
```cpp
// HttpClientContextData.h
MoveOnlyFunction<void(int, const http_header*, size_t, HttpClientContextData<SSL>*)> onHeaders = nullptr;
MoveOnlyFunction<void(std::string_view, bool)> onChunk = nullptr;

// Usage simplified to onHeaders and onChunk
.onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
               HttpClientContextData<SSL>* context) {
    // Handle headers and status
},
.onChunk = [](std::string_view chunk, bool isLastChunk) {
    // Handle data chunks and completion
}
```

### 2. **Enhanced onHeaders Callback**

**Before:**
```cpp
MoveOnlyFunction<void(const http_header*, size_t)> onHeaders = nullptr;

// Callback call
contextData->onHeaders(contextData->headers, contextData->numHeaders);
```

**After:**
```cpp
MoveOnlyFunction<void(int, const http_header*, size_t, HttpClientContextData<SSL>*)> onHeaders = nullptr;

// Callback call
contextData->onHeaders(contextData->responseStatus, contextData->headers, contextData->numHeaders, contextData);
```

### 3. **Simplified Completion Handling**

**Before:**
```cpp
// Chunked response completion
if (contextData->onChunk) {
    contextData->onChunk(std::string_view(data, bufsz), true);
}
if (contextData->onSuccess) {
    contextData->onSuccess(contextData->responseStatus, contextData->headers, 
                         contextData->numHeaders, "", "", true);
}

// Content response completion
if (contextData->onSuccess) {
    contextData->onSuccess(contextData->responseStatus, contextData->headers, 
                         contextData->numHeaders, "", body, true);
}
```

**After:**
```cpp
// Chunked response completion
if (contextData->onChunk) {
    contextData->onChunk(std::string_view(data, bufsz), true);
}

// Content response completion handled in onChunk with isLastChunk flag
```

### 4. **Improved Content Processing**

**Before:**
```cpp
/* Emit chunk data if streaming */
if (contextData->isStreaming && contextData->onChunk) {
    bool isLastChunk = (contextData->receivedBytes >= contextData->contentLength);
    contextData->onChunk(std::string_view(data, length), isLastChunk);
}

/* Check if response is complete */
if (contextData->receivedBytes >= contextData->contentLength) {
    // Call onSuccess with accumulated body
}
```

**After:**
```cpp
if (contextData->isStreaming) {
    /* Emit chunk data if streaming */
    if (contextData->onChunk) {
        bool isLastChunk = (contextData->receivedBytes >= contextData->contentLength);
        contextData->onChunk(std::string_view(data, length), isLastChunk);
    }
} else {
    /* For non-streaming responses, accumulate in buffer */
    contextData->buffer.append(data, length);
    
    /* Check if we have complete content */
    if (contextData->receivedBytes >= contextData->contentLength) {
        if (contextData->onChunk) {
            /* Send accumulated content as single chunk */
            contextData->onChunk(contextData->buffer, true);
        }
        /* Clear buffer after sending */
        contextData->buffer.clear();
    }
}
```

## Technical Details

### Callback Flow

**New Callback Flow:**
1. **`onHeaders`**: Called when HTTP response headers are parsed
   - Provides status code, parsed headers, and context pointer
   - Single point for all response metadata
2. **`onChunk`**: Called for all data chunks and completion
   - Streaming: Called for each chunk with `isLastChunk` flag
   - Non-streaming: Called once with complete accumulated data
   - `isLastChunk = true` indicates response completion

### Callback Parameters

**onHeaders:**
```cpp
void(int status, const http_header* headers, size_t numHeaders, HttpClientContextData<SSL>* context)
```
- `status`: HTTP response status code
- `headers`: Array of parsed HTTP headers
- `numHeaders`: Number of headers in the array
- `context`: Pointer to context data for additional access

**onChunk:**
```cpp
void(std::string_view chunk, bool isLastChunk)
```
- `chunk`: Data chunk (or complete accumulated data for non-streaming)
- `isLastChunk`: Flag indicating if this is the final chunk

**onError:**
```cpp
void(int errorCode, std::string_view errorMessage)
```
- `errorCode`: Error code
- `errorMessage`: Error description

### Benefits of Changes

**Simplified API:**
- **Fewer callbacks**: Removed redundant `onSuccess` callback
- **Clearer flow**: Headers → Data chunks → Completion (via `isLastChunk`)
- **Consistent pattern**: All completion handled through `onChunk`

**Better Performance:**
- **Reduced callback overhead**: One less callback to manage
- **Efficient data handling**: Direct chunk processing without intermediate success callback
- **Memory optimization**: Better buffer management for non-streaming responses

**Improved Usability:**
- **Single completion point**: `isLastChunk` flag in `onChunk` for completion detection
- **Context access**: Direct access to context data in `onHeaders`
- **Status availability**: HTTP status code available in `onHeaders`

## Usage Examples

### 1. **Basic HTTP Request**

**Before:**
```cpp
app.get("http://example.com/api", {
    .onHeaders = [](const http_header* headers, size_t numHeaders) {
        // Process headers
    },
    .onSuccess = [](int status, const http_header* headers, size_t numHeaders,
                   std::string_view requestHeaders, std::string_view body, bool complete) {
        // Handle completion
        std::cout << "Response: " << body << std::endl;
    }
});
```

**After:**
```cpp
app.get("http://example.com/api", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        std::cout << "Status: " << status << std::endl;
        // Process headers if needed
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process data chunk
        if (isLastChunk) {
            std::cout << "Response complete" << std::endl;
        }
    }
});
```

### 2. **Streaming Response Handling**

```cpp
app.get("http://example.com/stream", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        if (status == 200) {
            std::cout << "Stream started" << std::endl;
        }
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process each chunk
        std::cout << "Received chunk: " << chunk.size() << " bytes" << std::endl;
        
        if (isLastChunk) {
            std::cout << "Stream complete" << std::endl;
        }
    }
});
```

### 3. **Error Handling**

```cpp
app.get("http://example.com/api", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        if (status >= 400) {
            std::cout << "HTTP Error: " << status << std::endl;
        }
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process data
    },
    .onError = [](int errorCode, std::string_view errorMessage) {
        std::cout << "Error " << errorCode << ": " << errorMessage << std::endl;
    }
});
```

### 4. **Content-Type Detection**

```cpp
app.get("http://example.com/api", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        for (size_t i = 0; i < numHeaders; i++) {
            if (headers[i].name == "content-type") {
                std::string_view contentType = headers[i].value;
                if (contentType.starts_with("application/json")) {
                    std::cout << "JSON response" << std::endl;
                } else if (contentType.starts_with("text/html")) {
                    std::cout << "HTML response" << std::endl;
                }
                break;
            }
        }
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process data based on content type
    }
});
```

### 5. **Compression Detection**

```cpp
app.get("http://example.com/api", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        // Check compression flags from context
        if (context->hasGzip) {
            std::cout << "Response is gzip compressed" << std::endl;
        } else if (context->hasDeflate) {
            std::cout << "Response is deflate compressed" << std::endl;
        }
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Handle compressed data
    }
});
```

## Migration Impact

### ✅ **Breaking Changes**
- **Removed `onSuccess` callback**: All completion handling now done via `onChunk` with `isLastChunk` flag
- **Enhanced `onHeaders` signature**: Now includes status code and context pointer
- **User code updates required**: All callback implementations need updating

### ✅ **Performance Benefits**
- **Reduced callback overhead**: One less callback to manage and invoke
- **Simplified completion logic**: Single completion point via `isLastChunk` flag
- **Better memory management**: Improved buffer handling for non-streaming responses

### ✅ **Code Quality**
- **Cleaner API**: More consistent and intuitive callback design
- **Better separation of concerns**: Headers vs data handling clearly separated
- **Improved maintainability**: Simpler callback flow and logic

## Migration Guide

### 1. **Remove onSuccess Callbacks**

**Before:**
```cpp
.onSuccess = [](int status, const http_header* headers, size_t numHeaders,
               std::string_view requestHeaders, std::string_view body, bool complete) {
    // Handle completion
}
```

**After:**
```cpp
// Remove onSuccess entirely - completion handled in onChunk
```

### 2. **Update onHeaders Callbacks**

**Before:**
```cpp
.onHeaders = [](const http_header* headers, size_t numHeaders) {
    // Process headers
}
```

**After:**
```cpp
.onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
               HttpClientContextData<SSL>* context) {
    // Process headers and status
    std::cout << "Status: " << status << std::endl;
    
    // Access context if needed
    if (context->hasGzip) {
        // Handle compression
    }
}
```

### 3. **Update onChunk for Completion**

**Before:**
```cpp
.onChunk = [](std::string_view chunk, bool isLastChunk) {
    // Process chunks
}
// Completion handled in onSuccess
```

**After:**
```cpp
.onChunk = [](std::string_view chunk, bool isLastChunk) {
    // Process chunks
    std::cout << "Received chunk: " << chunk.size() << " bytes" << std::endl;
    
    if (isLastChunk) {
        // Handle completion
        std::cout << "Response complete" << std::endl;
    }
}
```

### 4. **Complete Migration Example**

**Before:**
```cpp
app.get("http://example.com/api", {
    .onHeaders = [](const http_header* headers, size_t numHeaders) {
        // Process headers
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process chunks
    },
    .onSuccess = [](int status, const http_header* headers, size_t numHeaders,
                   std::string_view requestHeaders, std::string_view body, bool complete) {
        // Handle completion
        std::cout << "Complete response: " << body << std::endl;
    }
});
```

**After:**
```cpp
app.get("http://example.com/api", {
    .onHeaders = [](int status, const http_header* headers, size_t numHeaders, 
                   HttpClientContextData<SSL>* context) {
        std::cout << "Status: " << status << std::endl;
        // Process headers if needed
    },
    .onChunk = [](std::string_view chunk, bool isLastChunk) {
        // Process chunks
        std::cout << "Received chunk: " << chunk.size() << " bytes" << std::endl;
        
        if (isLastChunk) {
            // Handle completion
            std::cout << "Response complete" << std::endl;
        }
    }
});
```

## Conclusion

The callback API refactoring successfully:

1. **Simplified the API** - Removed redundant `onSuccess` callback
2. **Improved consistency** - All completion handled through `onChunk` with `isLastChunk` flag
3. **Enhanced functionality** - `onHeaders` now provides status and context access
4. **Better performance** - Reduced callback overhead and improved memory management
5. **Cleaner design** - More intuitive and maintainable callback flow

The HTTP client implementation now provides a more streamlined and efficient callback API that clearly separates header processing from data handling while maintaining all necessary functionality through a simpler, more consistent interface.
