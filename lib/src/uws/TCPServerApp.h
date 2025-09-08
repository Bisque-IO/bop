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

#ifndef UWS_TCPSERVERAPP_H
#define UWS_TCPSERVERAPP_H

#include "libusockets.h"  // For us_socket_context_listen

#include "App.h"  // For SocketContextOptions
#include "Loop.h"
#include "TCPContext.h"
#include "TCPConnection.h"
#include <cstdint>
#include <string>
#include <string_view>

namespace uWS {

/* Server name */
struct ServerName {
    std::string name;
    SocketContextOptions options;
};

/* TCP Server application for listening functionality only */
template <bool SSL, typename USERDATA = void>
struct TemplatedTCPServerApp {
private:
    /* SSL options for context creation */
    SocketContextOptions sslOptions;
    
    /* Reference to the loop */
    Loop* loop;
    
    /* The single TCP context for this server app */
    TCPContext<SSL, USERDATA>* context = nullptr;
    
    /* The listening socket */
    struct us_listen_socket_t* listenSocket = nullptr;
    
    /* Private constructor - use static factory methods */
    TemplatedTCPServerApp(Loop* loop, SocketContextOptions options = {}) : sslOptions(options), loop(loop) {
    }

    /* Helper method to create app, context, and set up behavior */
    static TemplatedTCPServerApp<SSL, USERDATA>* create(
        Loop* loop,
        TCPBehavior<SSL, USERDATA>&& behavior,
        SocketContextOptions sslOptions,
        bool isUnix,
        std::string host,
        uint32_t hostIP4,
        int port,
        int options,
        std::function<us_listen_socket_t*(us_socket_context_t*, int connExtSize)> listenCB,
        std::vector<ServerName>& serverNames
    ) {
        auto* app = new TemplatedTCPServerApp<SSL, USERDATA>(loop, sslOptions);
        app->context = TCPContext<SSL, USERDATA>::create(loop, sslOptions, true);
        if (!app->context) {
            delete app;
            return nullptr;
        }
    
        auto* contextData = app->context->getSocketContextData();
        assert(contextData);

        contextData->connUserDataSize = (std::max(behavior.connUserDataSize, 16) + 15) / 16 * 16;
        contextData->connExtSize = sizeof(TCPConnectionData<SSL, USERDATA>) + std::max(contextData->connUserDataSize, (int)((sizeof(USERDATA) + 15) / 16 * 16));

        // Set up behavior handlers - now we can move directly from the rvalue reference
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
        
        // Set configuration
        contextData->idleTimeoutSeconds = behavior.idleTimeoutSeconds;
        contextData->longTimeoutMinutes = behavior.longTimeoutMinutes;
        contextData->maxBackpressure = behavior.maxBackpressure;
        contextData->resetIdleTimeoutOnSend = behavior.resetIdleTimeoutOnSend;
        contextData->closeOnBackpressureLimit = behavior.closeOnBackpressureLimit;

        if (!isUnix) {
            contextData->listenerHost = std::move(host);
            contextData->listenerOptions = options;
            contextData->listenerPort = port;
        } else {
            contextData->listenerPath = std::move(host);
            contextData->listenerOptions = options;
        }

        // Store the SSL options in the context data for later reference. Create owned values.
        contextData->sslConfig.certFile = sslOptions.cert_file_name ? sslOptions.cert_file_name : "";
        contextData->sslConfig.keyFile = sslOptions.key_file_name ? sslOptions.key_file_name : "";
        contextData->sslConfig.caFile = sslOptions.ca_file_name ? sslOptions.ca_file_name : "";
        contextData->sslConfig.passphrase = sslOptions.passphrase ? sslOptions.passphrase : "";
        contextData->sslConfig.dhParamsFile = sslOptions.dh_params_file_name ? sslOptions.dh_params_file_name : "";
        contextData->sslConfig.sslCiphers = sslOptions.ssl_ciphers ? sslOptions.ssl_ciphers : "";
        contextData->sslConfig.sslPreferLowMemoryUsage = sslOptions.ssl_prefer_low_memory_usage;

        if constexpr (SSL) {
            for (const auto& serverName : serverNames) {
                app->addServerName(serverName.name, serverName.options);
            }
        }

        app->listenSocket = listenCB(reinterpret_cast<us_socket_context_t*>(app->context), contextData->connExtSize);
        if (!app->listenSocket) {
            delete app;
            return nullptr;
        }

        if (!isUnix) {
            contextData->listenerPort = us_socket_local_port(SSL, reinterpret_cast<us_socket_t*>(app->listenSocket));
        }

        return app;
    }
    
public:
    /* Move constructor */
    TemplatedTCPServerApp(TemplatedTCPServerApp&& other) noexcept = delete;
    
    /* Move assignment */
    TemplatedTCPServerApp& operator=(TemplatedTCPServerApp&& other) noexcept = delete;
    
    /* Destructor */
    ~TemplatedTCPServerApp() {
        if (context) {
            context->free();
            context = nullptr;
            listenSocket = nullptr;
        }
    }
    
    /* Static factory methods for creating server apps */
    static TemplatedTCPServerApp<SSL, USERDATA>* listen(Loop* loop, int port, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return listen(loop, "", port, 0, std::move(behavior), sslOptions, serverNames);
    }
    
    static TemplatedTCPServerApp<SSL, USERDATA>* listen(Loop* loop, int port, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return listen(loop, "", port, 0, std::move(behavior), sslOptions, serverNames);
    }
    
    static TemplatedTCPServerApp<SSL, USERDATA>* listen(Loop* loop, std::string host, int port, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return listen(loop, host, port, 0, std::move(behavior), sslOptions, serverNames);
    }
    
    static TemplatedTCPServerApp<SSL, USERDATA>* listen(Loop* loop, std::string host, int port, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return create(loop, std::move(behavior), sslOptions, false, host, 0, port, options, [host, port, options](auto* context, int connExtSize) {
            return us_socket_context_listen(SSL, reinterpret_cast<us_socket_context_t*>(context), host.empty() ? nullptr : host.c_str(), port, options, connExtSize);
        }, serverNames);
    }

    static TemplatedTCPServerApp<SSL, USERDATA>* listenIP4(Loop* loop, uint32_t host, int port, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return create(loop, std::move(behavior), sslOptions, false, "", host,port, options, [host, port, options](auto* context, int connExtSize) {
            return us_socket_context_listen_ip4(SSL, reinterpret_cast<us_socket_context_t*>(context), host, port, options, connExtSize);
        }, serverNames);
    }
    
    static TemplatedTCPServerApp<SSL, USERDATA>* listenUnix(Loop* loop, std::string path, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        return listenUnix(loop, path, 0, std::move(behavior), sslOptions);
    }
    
    static TemplatedTCPServerApp<SSL, USERDATA>* listenUnix(Loop* loop, std::string path, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}, std::vector<ServerName> serverNames = {}) {
        return create(loop, std::move(behavior), sslOptions, false, path, 0, options, [path, options](auto* context, int connExtSize) {
            us_socket_context_listen_unix(SSL, reinterpret_cast<us_socket_context_t*>(context), path.c_str(), options, connExtSize);
        }, serverNames);
    }

    /* Server name */
    TemplatedTCPServerApp<SSL, USERDATA>& addServerName(void* userData, std::string hostnamePattern, SocketContextOptions options = {}) {
        /* Do nothing if not even on SSL */
        if constexpr (SSL) {
            us_socket_context_add_server_name(SSL, (struct us_socket_context_t *) context, hostnamePattern.c_str(), options, userData);
        }
        return *this;
    }

    void* removeServerName(std::string hostnamePattern) {
        /* This will do for now, would be better if us_socket_context_remove_server_name returned the user data */
        auto *userData = us_socket_context_find_server_name_userdata(SSL, (struct us_socket_context_t *) context, hostnamePattern.c_str());
        us_socket_context_remove_server_name(SSL, (struct us_socket_context_t *) context, hostnamePattern.c_str());
        return userData;
    }

    /* Returns the SSL_CTX of this app, or nullptr. */
    void* getNativeHandle() {
        return us_socket_context_get_native_handle(SSL, (struct us_socket_context_t *) context);
    }
    
    /* Get SSL options */
    SocketContextOptions& getSSLOptions() {
        return sslOptions;
    }

    /* Get the loop reference */
    Loop* getLoop() {
        return loop;
    }
    
    /* Get the TCP context */
    TCPContext<SSL, USERDATA>* getContext() {
        return context;
    }

    /* Get the TCP context data */
    TCPContextData<SSL, USERDATA>* getContextData() {
        if (!context) return nullptr;
        return context->getSocketContextData();
    }
    
    /* Get the listening socket */
    struct us_listen_socket_t* getListenSocket() {
        return listenSocket;
    }
    
    /* Check if the server is valid */
    bool isValid() {
        return listenSocket != nullptr;
    }

    int getListenerPort() {
        if (!listenSocket) return 0;
        return context->getSocketContextData()->listenerPort;
    }

    /* adopt an externally accepted socket */
    us_socket_t* adoptSocket(LIBUS_SOCKET_DESCRIPTOR accepted_fd) {
        return context ? context->adoptAcceptedSocket(accepted_fd) : nullptr;
    }
};

/* Type aliases for convenience */
template <typename USERDATA = void>
using TCPServerApp = TemplatedTCPServerApp<false, USERDATA>;

template <typename USERDATA = void>
using SSLTCPServerApp = TemplatedTCPServerApp<true, USERDATA>;

} // namespace uWS

#endif // UWS_TCPSERVERAPP_H
