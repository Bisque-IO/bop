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
#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct HttpClientConnection;

/* HTTP client context data for managing HTTP client context-level configuration and callbacks */
template <bool SSL, typename USERDATA = void>
struct alignas(16) HttpClientContextData {
    template <bool, typename> friend struct HttpClientContext;
    template <bool, typename> friend struct HttpClientConnection;
    
private:
    /* Connection callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onConnection = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onDisconnected = nullptr;
    
    /* HTTP response callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view, std::string_view)> onResponse = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onResponseData = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onResponseEnd = nullptr;
    
    /* Data flow callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onData = nullptr;
    MoveOnlyFunction<bool(HttpClientConnection<SSL, USERDATA>*, uintmax_t)> onWritable = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onDrain = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onDropped = nullptr;
    
    /* Timeout callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onTimeout = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onLongTimeout = nullptr;
    
    /* Default configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONTEXTDATA_H
