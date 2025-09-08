/*
 * Authored by Clay Molocznik, 2025.
 * Intellectual property of third-party.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#ifndef UWS_TCPCLIENTAPP_H
#define UWS_TCPCLIENTAPP_H

#include "Loop.h"
#include "TCPContext.h"
#include "TCPConnection.h"
#include "App.h"  // For SocketContextOptions

#include <string_view>

namespace uWS {

/* TCP Client application for connecting functionality only */
template <bool SSL, typename USERDATA = void>
struct TemplatedTCPClientApp {
private:
    /* SSL options for context creation */
    SocketContextOptions sslOptions;
    
    /* Reference to the loop */
    Loop* loop;
    
    /* The single TCP context for this client app */
    TCPContext<SSL, USERDATA>* context = nullptr;
    
public:
    /* Constructor with SSL options and behavior */
    TemplatedTCPClientApp(Loop* loop, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions options = {}) 
        : sslOptions(options), loop(loop) {
        assert(loop);

        /*
        Increment keep-alive ref since we are only client connections and have no
        listeners to keep the loop alive.
        */
        loop->incrementKeepAliveRef();
        
        /* Create the TCP context once on construction */
        context = TCPContext<SSL, USERDATA>::create(loop, sslOptions, false);
        if (context) {
            auto contextData = context->getSocketContextData();
            assert(contextData);
            contextData->connUserDataSize = (std::max(behavior.connUserDataSize, 16) + 15) / 16 * 16;
            contextData->connExtSize = sizeof(TCPConnectionData<SSL, USERDATA>) + std::max(contextData->connUserDataSize, (int)((sizeof(USERDATA) + 15) / 16 * 16));

            /* Set up behavior handlers - now we can move directly from the rvalue reference */
            if (behavior.onOpen) {
                contextData->onConnection = std::move(behavior.onOpen);
            }
            if (behavior.onData) {
                contextData->onData = std::move(behavior.onData);
            }
            if (behavior.onClose) {
                contextData->onDisconnected = std::move(behavior.onClose);
            }
            if (behavior.onConnectError) {
                contextData->onConnectError = std::move(behavior.onConnectError);
            }
            if (behavior.onServerName) {
                contextData->onServerName = std::move(behavior.onServerName);
            }
            if (behavior.onWritable) {
                contextData->onWritable = std::move(behavior.onWritable);
            }
            if (behavior.onDrain) {
                contextData->onDrain = std::move(behavior.onDrain);
            }
            if (behavior.onDropped) {
                contextData->onDropped = std::move(behavior.onDropped);
            }
            if (behavior.onEnd) {
                contextData->onEnd = std::move(behavior.onEnd);
            }
            if (behavior.onTimeout) {
                contextData->onTimeout = std::move(behavior.onTimeout);
            }
            if (behavior.onLongTimeout) {
                contextData->onLongTimeout = std::move(behavior.onLongTimeout);
            }
            
            /* Set configuration */
            contextData->idleTimeoutSeconds = behavior.idleTimeoutSeconds;
            contextData->longTimeoutMinutes = behavior.longTimeoutMinutes;
            contextData->maxBackpressure = behavior.maxBackpressure;
            contextData->resetIdleTimeoutOnSend = behavior.resetIdleTimeoutOnSend;
            contextData->closeOnBackpressureLimit = behavior.closeOnBackpressureLimit;
        }
    }
    
    /* Move constructor */
    TemplatedTCPClientApp(TemplatedTCPClientApp&& other) noexcept = default;
    
    /* Move assignment */
    TemplatedTCPClientApp& operator=(TemplatedTCPClientApp&& other) noexcept = default;
    
    /* Destructor */
    ~TemplatedTCPClientApp() {
        if (context) {
            context->free();
            context = nullptr;
        }
        if (loop) {
            loop->decrementKeepAliveRef();
            loop = nullptr;
        }
    }
    
    /* Client-side: Connect methods using the existing context */
    TCPConnection<SSL, USERDATA>* connect(const char* host, int port, 
                                        const char* source_host = nullptr, int options = 0) {
        if (!context) return nullptr;
        auto* socket = context->connect(host, port, source_host != nullptr && strlen(source_host) > 0 ? source_host : nullptr, options);
        return reinterpret_cast<TCPConnection<SSL, USERDATA>*>(socket);
    }

    /* Client-side: Connect methods using the existing context */
    TCPConnection<SSL, USERDATA>* connectIP4(uint32_t host_ip, int port, uint32_t source_host_ip = 0, int options = 0) {
        if (!context) return nullptr;
        auto* socket = context->connectIP4(host_ip, port, source_host_ip, options);
        return reinterpret_cast<TCPConnection<SSL, USERDATA>*>(socket);
    }
    
    TCPConnection<SSL, USERDATA>* connectUnix(const char* server_path, int options = 0) {
        if (!context) return nullptr;
        
        auto* socket = context->connectUnix(server_path != nullptr && strlen(server_path) > 0 ? server_path : nullptr, options);
        return reinterpret_cast<TCPConnection<SSL, USERDATA>*>(socket);
    }
    
    /* Get SSL options */
    const SocketContextOptions& getSSLOptions() const {
        return sslOptions;
    }
    
    /* Set SSL options */
    void setSSLOptions(const SocketContextOptions& options) {
        sslOptions = options;
    }
    
    /* Get the loop reference */
    Loop *getLoop() const {
        return loop;
    }
    
    /* Get the TCP context */
    TCPContext<SSL, USERDATA>* getContext() const {
        return context;
    }

    /* Check if the client app is valid */
    bool isValid() const {
        return context != nullptr;
    }

    /* adopt an externally accepted socket */
    TemplatedTCPClientApp<SSL, USERDATA>& adoptSocket(LIBUS_SOCKET_DESCRIPTOR accepted_fd) {
        if (context) {
            context->adoptAcceptedSocket(accepted_fd);
        }
        return *this;
    }
};

/* Type aliases for convenience */
template <typename USERDATA = void>
using TCPClientApp = TemplatedTCPClientApp<false, USERDATA>;

template <typename USERDATA = void>
using SSLTCPClientApp = TemplatedTCPClientApp<true, USERDATA>;

} // namespace uWS

#endif // UWS_TCPCLIENTAPP_H
