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
        if (loop) {
            loop->incrementKeepAliveRef();
        }
        
        // Create the TCP context once on construction
        context = TCPContext<SSL, USERDATA>::create(loop, sslOptions, false);
        if (context) {
            // Set up behavior handlers - now we can move directly from the rvalue reference
            if (behavior.onConnection) {
                context->onConnection(std::move(behavior.onConnection));
            }
            if (behavior.onData) {
                context->onData(std::move(behavior.onData));
            }
            if (behavior.onDisconnected) {
                context->onDisconnected(std::move(behavior.onDisconnected));
            }
            if (behavior.onConnectError) {
                context->onConnectError(std::move(behavior.onConnectError));
            }
            if (behavior.onWritable) {
                context->onWritable(std::move(behavior.onWritable));
            }
            if (behavior.onDrain) {
                context->onDrain(std::move(behavior.onDrain));
            }
            if (behavior.onDropped) {
                context->onDropped(std::move(behavior.onDropped));
            }
            if (behavior.onEnd) {
                context->onEnd(std::move(behavior.onEnd));
            }
            if (behavior.onTimeout) {
                context->onTimeout(std::move(behavior.onTimeout));
            }
            if (behavior.onLongTimeout) {
                context->onLongTimeout(std::move(behavior.onLongTimeout));
            }
            
            // Set configuration
            context->setTimeout(behavior.idleTimeoutSeconds, behavior.longTimeoutMinutes);
            context->setMaxBackpressure(behavior.maxBackpressure);
            context->setResetIdleTimeoutOnSend(behavior.resetIdleTimeoutOnSend);
            context->setCloseOnBackpressureLimit(behavior.closeOnBackpressureLimit);
        }
    }
    
    /* Move constructor */
    TemplatedTCPClientApp(TemplatedTCPClientApp&& other) noexcept {
        sslOptions = other.sslOptions;
        loop = other.loop;
        context = other.context;
        other.loop = nullptr;
        other.context = nullptr;
    }
    
    /* Move assignment */
    TemplatedTCPClientApp& operator=(TemplatedTCPClientApp&& other) noexcept {
        if (this != &other) {
            if (loop) {
                loop->decrementKeepAliveRef();
            }
            
            // Free current context
            if (context) {
                context->free();
                context = nullptr;
            }
            
            sslOptions = other.sslOptions;
            loop = other.loop;
            context = other.context;
            other.loop = nullptr;
            other.context = nullptr;
        }
        return *this;
    }
    
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
        auto* socket = context->connect(host, port, source_host, options);
        return reinterpret_cast<TCPConnection<SSL, USERDATA>*>(socket);
    }
    
    TCPConnection<SSL, USERDATA>* connectUnix(const char* server_path, int options = 0) {
        if (!context) return nullptr;
        
        auto* socket = context->connectUnix(server_path, options);
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
    Loop* getLoop() const {
        return loop;
    }
    
    /* Get the TCP context */
    TCPContext<SSL, USERDATA>* getContext() const {
        return context;
    }
    
    /* Check if keep-alive poll is active */
    bool isKeepAliveActive() const {
        return loop && loop->keepAliveRefCount() > 0;
    }
    
    /* Check if the client app is valid */
    bool isValid() const {
        return context != nullptr;
    }
};

/* Type aliases for convenience */
template <typename USERDATA = void>
using TCPClientApp = TemplatedTCPClientApp<false, USERDATA>;

template <typename USERDATA = void>
using SSLTCPClientApp = TemplatedTCPClientApp<true, USERDATA>;

} // namespace uWS

#endif // UWS_TCPCLIENTAPP_H
