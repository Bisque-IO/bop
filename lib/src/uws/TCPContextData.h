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

#ifndef UWS_TCPCONTEXTDATA_H
#define UWS_TCPCONTEXTDATA_H

#include "MoveOnlyFunction.h"
#include <string>
#include <string_view>

namespace uWS {

struct TCPAppBase {
    virtual ~TCPAppBase() = default;
};

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPConnection;
template <bool SSL, typename USERDATA> struct TCPConnectionData;
template <bool SSL, typename USERDATA> struct TCPContext;

/* TCP context data for managing TCP context-level configuration and callbacks */
template <bool SSL, typename USERDATA = void>
struct alignas(16) TCPContextData {
    template <bool, typename> friend struct TCPContext;
    template <bool, typename> friend struct TCPConnection;
    template <bool, typename> friend struct TemplatedTCPServerApp;
    template <bool, typename> friend struct TemplatedTCPClientApp;
    
private:
    /* Connection callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)> onConnection = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, int, void*)> onDisconnected = nullptr;
    MoveOnlyFunction<void(TCPContext<SSL, USERDATA>*, std::string_view)> onServerName = nullptr;
    
    /* Data flow callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, std::string_view)> onData = nullptr;
    MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, uintmax_t)> onWritable = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)> onDrain = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, std::string_view)> onDropped = nullptr;
    MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)> onEnd = nullptr;
    
    /* Timeout callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)> onTimeout = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)> onLongTimeout = nullptr;
    
    /* Default configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* Listener configuration */
    std::string listenerHost{};
    int listenerPort = 0;
    std::string listenerPath{};  // For Unix domain sockets
    
    /* SSL configuration */
    struct {
        std::string certFile{};
        std::string keyFile{};
        std::string caFile{};
        std::string passphrase{};
        std::string dhParamsFile{};
        std::string sslCiphers{};
        int sslPreferLowMemoryUsage = 0;
        // Additional SSL verification settings (not in SocketContextOptions)
        bool verifyPeer = false;
        bool verifyHost = false;
        int verifyDepth = 4;
    } sslConfig{};
};

} // namespace uWS

#endif // UWS_TCPCONTEXTDATA_H
