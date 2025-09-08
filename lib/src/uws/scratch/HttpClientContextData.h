/*
 * Authored by Clay Molocznik, 2025.
 * Intellectual property of third-party.

 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at

 *     http://www.apache.org/licenses/LICENSE-2.0

 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#ifndef UWS_HTTPCLIENTCONTEXTDATA_H
#define UWS_HTTPCLIENTCONTEXTDATA_H

#include "MoveOnlyFunction.h"
#include "HttpParser.h"
#include <string>
#include " ../Utilities.h\

namespace uWS {
template<bool> struct HttpClientResponse;
struct HttpRequest;

template <bool SSL>
struct alignas(16) HttpClientContextData {
    template <bool> friend struct HttpClientContext;
    template <bool> friend struct HttpClientResponse;
    template <bool> friend struct TemplatedClientApp;
private:
    /* HTTP Client-specific callbacks */
    MoveOnlyFunction<void(HttpClientResponse<SSL> *, int, std::string_view)> onConnected = nullptr;
    MoveOnlyFunction<void(HttpClientResponse<SSL> *, int, void *)> onDisconnected = nullptr;
    MoveOnlyFunction<void(HttpClientResponse<SSL> *)> onTimeout = nullptr;
    MoveOnlyFunction<void(HttpClientResponse<SSL> *, int)> onConnectError = nullptr;
    
    /* HTTP Client behavior callbacks */
    MoveOnlyFunction<void(int, std::string_view)> onError = nullptr;
    MoveOnlyFunction<void(int, const HttpResponseHeaders::Header*, size_t, HttpClientContextData<SSL>*)> onHeaders = nullptr;
    MoveOnlyFunction<void(std::string_view, bool)> onChunk = nullptr;
    
    /* Request streaming callbacks */
    MoveOnlyFunction<void()> requestReady = nullptr;  // Called when ready to send request data
    MoveOnlyFunction<void()> requestComplete = nullptr;  // Called when request data is complete
    
    /* Request backpressure handling */
    MoveOnlyFunction<bool(uintmax_t)> onWritable = nullptr;  // Called when socket is writable
    MoveOnlyFunction<void()> onDrain = nullptr;  // Called when backpressure buffer is drained

    /* Client-specific state tracking */
    uint32_t totalRequests = 0;
    uint32_t failedRequests = 0;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;  // Maximum delay until connection termination
    
    /* Request streaming state */
    bool requestHeadersSent = false;
    
    /* Request backpressure state */
    uintmax_t requestOffset = 0;  // Current write offset for backpressure tracking
    
    /* Buffer for incomplete data (for pipelining) */
    std::string buffer;
    
    /* HttpParser state */
    HttpParser parser;
    HttpResponseHeaders responseHeaders;
    

    
    /* Caller of onWritable. It is possible onWritable calls markDone so we need to borrow it. */
    bool callOnWritable(uintmax_t offset) {
        /* Borrow real onWritable */
        MoveOnlyFunction<bool(uintmax_t)> borrowedOnWritable = std::move(onWritable);

        /* Set onWritable to placeholder */
        onWritable = [](uintmax_t) {return true;};

        /* Run borrowed onWritable */
        bool ret = borrowedOnWritable(offset);

        /* If we still have onWritable (the placeholder) then move back the real one */
        if (onWritable) {
            /* We haven't reset onWritable, so give it back */
            onWritable = std::move(borrowedOnWritable);
        }

        return ret;
    }
    
    void reset() {
        buffer.clear();
        
        /* Reset request streaming state */
        requestHeadersSent = false;
        requestOffset = 0;
        
        /* Reset HttpParser state - it manages its own internal state */
        parser = HttpParser();
        responseHeaders = HttpResponseHeaders();
    }
};

}

#endif // UWS_HTTPCLIENTCONTEXTDATA_H
