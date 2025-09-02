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

#include "Loop.h"
#include "TCPContext.h"
#include "TCPConnection.h"
#include "App.h"  // For SocketContextOptions
#include "libusockets.h"  // For us_socket_context_listen
#include <string>
#include <string_view>

namespace uWS {

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
    
    /* Helper method to set up behavior handlers and configuration */
    void setupBehavior(TCPBehavior<SSL, USERDATA>&& behavior) {
        if (!context) return;

        auto contextData = context->getSocketContextData();
        
        // Set up behavior handlers - now we can move directly from the rvalue reference
        if (behavior.onConnection) {
            contextData->onConnection = std::move(behavior.onConnection);
        }
        if (behavior.onData) {
            contextData->onData = std::move(behavior.onData);
        }
        if (behavior.onDisconnected) {
            contextData->onDisconnected = std::move(behavior.onDisconnected);
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
        context->setTimeout(behavior.idleTimeoutSeconds, behavior.longTimeoutMinutes);
        context->setMaxBackpressure(behavior.maxBackpressure);
        context->setResetIdleTimeoutOnSend(behavior.resetIdleTimeoutOnSend);
        context->setCloseOnBackpressureLimit(behavior.closeOnBackpressureLimit);
    }
    
    /* Helper method to set listener configuration */
    void setListenerConfig(const std::string& host, int port) {
        if (!context) return;
        
        auto* contextData = context->getSocketContextData();
        contextData->listenerHost = host;
        contextData->listenerPort = port;
    }
    
    /* Helper method to set Unix domain socket listener configuration */
    void setUnixListenerConfig(const std::string& path) {
        if (!context) return;
        
        auto* contextData = context->getSocketContextData();
        contextData->listenerPath = path;
    }
    
    /* Helper method to set SSL configuration */
    void setSSLConfig(const SocketContextOptions& sslOptions) {
        if (!context) return;
        
        auto* contextData = context->getSocketContextData();
        // Store the SSL options in the context data for later reference
        contextData->sslConfig.certFile = sslOptions.cert_file_name ? sslOptions.cert_file_name : "";
        contextData->sslConfig.keyFile = sslOptions.key_file_name ? sslOptions.key_file_name : "";
        contextData->sslConfig.caFile = sslOptions.ca_file_name ? sslOptions.ca_file_name : "";
        contextData->sslConfig.passphrase = sslOptions.passphrase ? sslOptions.passphrase : "";
        contextData->sslConfig.dhParamsFile = sslOptions.dh_params_file_name ? sslOptions.dh_params_file_name : "";
        contextData->sslConfig.sslCiphers = sslOptions.ssl_ciphers ? sslOptions.ssl_ciphers : "";
        contextData->sslConfig.sslPreferLowMemoryUsage = sslOptions.ssl_prefer_low_memory_usage;
        
        // Update the app's SSL options
        this->sslOptions = sslOptions;
    }
    
    /* Helper method to set additional SSL verification settings */
    void setSSLVerification(bool verifyPeer, bool verifyHost, int verifyDepth = 4) {
        if (!context) return;
        
        auto* contextData = context->getSocketContextData();
        contextData->sslConfig.verifyPeer = verifyPeer;
        contextData->sslConfig.verifyHost = verifyHost;
        contextData->sslConfig.verifyDepth = verifyDepth;
    }
    
    /* Helper method to create app, context, and set up behavior */
    static TemplatedTCPServerApp* create(Loop* loop, TCPBehavior<SSL, USERDATA>&& behavior, SocketContextOptions sslOptions) {
        auto* app = new TemplatedTCPServerApp(loop, sslOptions);
        app->context = TCPContext<SSL, USERDATA>::create(loop, sslOptions, true);
        if (!app->context) {
            delete app;
            return nullptr;
        }
        
        app->setupBehavior(std::move(behavior));
        return app;
    }
    
public:
    /* Move constructor */
    TemplatedTCPServerApp(TemplatedTCPServerApp&& other) noexcept = delete;
    
    /* Move assignment */
    TemplatedTCPServerApp& operator=(TemplatedTCPServerApp&& other) noexcept = delete;
    
    /* Destructor */
    ~TemplatedTCPServerApp() {
        // if (listenSocket) {
        //     us_socket_close(listenSocket);
        //     listenSocket = nullptr;
        // }
        if (context) {
            context->free();
            context = nullptr;
            listenSocket = nullptr;
        }
    }
    
    /* Static factory methods for creating server apps */
    static TemplatedTCPServerApp* listen(Loop* loop, int port, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set listener configuration
        app->setListenerConfig("", port);
        
        // Start listening
        app->listenSocket = us_socket_context_listen(SSL, reinterpret_cast<us_socket_context_t*>(app->context), nullptr, port, 0, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
    }
    
    static TemplatedTCPServerApp* listen(Loop* loop, int port, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set listener configuration
        app->setListenerConfig("", port);
        
        // Start listening
        app->listenSocket = us_socket_context_listen(SSL, reinterpret_cast<us_socket_context_t*>(app->context), nullptr, port, options, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
    }
    
    static TemplatedTCPServerApp* listen(Loop* loop, std::string host, int port, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        if (host == "localhost") {
            return listen(loop, port, std::move(behavior), sslOptions);
        }
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set listener configuration
        app->setListenerConfig(host, port);
        
        // Start listening
        app->listenSocket = us_socket_context_listen(SSL, reinterpret_cast<us_socket_context_t*>(app->context), host.c_str(), port, 0, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
    }
    
    static TemplatedTCPServerApp* listen(Loop* loop, std::string host, int port, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        if (host == "localhost") {
            return listen(loop, port, options, std::move(behavior), sslOptions);
        }
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set listener configuration
        app->setListenerConfig(host, port);
        
        // Start listening
        app->listenSocket = us_socket_context_listen(SSL, reinterpret_cast<us_socket_context_t*>(app->context), host.c_str(), port, options, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
    }
    
    static TemplatedTCPServerApp* listenUnix(Loop* loop, std::string path, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set Unix domain socket listener configuration
        app->setUnixListenerConfig(path);
        
        // Start listening
        app->listenSocket = us_socket_context_listen_unix(SSL, reinterpret_cast<us_socket_context_t*>(app->context), path.c_str(), 0, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
    }
    
    static TemplatedTCPServerApp* listenUnix(Loop* loop, std::string path, int options, TCPBehavior<SSL, USERDATA>&& behavior = {}, SocketContextOptions sslOptions = {}) {
        auto* app = create(loop, std::move(behavior), sslOptions);
        if (!app) return nullptr;
        
        // Set Unix domain socket listener configuration
        app->setUnixListenerConfig(path);
        
        // Start listening
        app->listenSocket = us_socket_context_listen_unix(SSL, reinterpret_cast<us_socket_context_t*>(app->context), path.c_str(), options, getTCPConnectionDataSize<SSL, USERDATA>());
        return app;
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
    
    /* Get the listening socket */
    struct us_listen_socket_t* getListenSocket() const {
        return listenSocket;
    }
    
    /* Check if keep-alive poll is active */
    bool isKeepAliveActive() const {
        return loop && loop->keepAliveRefCount() > 0;
    }
    
    /* Check if the server is valid */
    bool isValid() const {
        return listenSocket != nullptr;
    }
    
    /* Get listener configuration */
    std::string getListenerHost() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->listenerHost;
    }
    
    int getListenerPort() const {
        if (!context) return 0;
        const auto* contextData = context->getSocketContextData();
        return contextData->listenerPort;
    }
    
    std::string getListenerPath() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->listenerPath;
    }
    
    /* Get SSL configuration */
    std::string getSSLCertFile() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.certFile;
    }
    
    std::string getSSLKeyFile() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.keyFile;
    }
    
    std::string getSSLCAFile() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.caFile;
    }
    
    bool getSSLVerifyPeer() const {
        if (!context) return false;
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.verifyPeer;
    }
    
    bool getSSLVerifyHost() const {
        if (!context) return false;
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.verifyHost;
    }
    
    std::string getSSLDHParamsFile() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.dhParamsFile;
    }
    
    std::string getSSLCiphers() const {
        if (!context) return "";
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.sslCiphers;
    }
    
    int getSSLPreferLowMemoryUsage() const {
        if (!context) return 0;
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.sslPreferLowMemoryUsage;
    }
    
    int getSSLVerifyDepth() const {
        if (!context) return 4;
        const auto* contextData = context->getSocketContextData();
        return contextData->sslConfig.verifyDepth;
    }

    int getLocalPort() {
        if (!listenSocket) return 0;
        // Cast listen socket to regular socket to get the port
        return us_socket_local_port(SSL, reinterpret_cast<us_socket_t*>(listenSocket));
    }
};

/* Type aliases for convenience */
template <typename USERDATA = void>
using TCPServerApp = TemplatedTCPServerApp<false, USERDATA>;

template <typename USERDATA = void>
using SSLTCPServerApp = TemplatedTCPServerApp<true, USERDATA>;

} // namespace uWS

#endif // UWS_TCPSERVERAPP_H
