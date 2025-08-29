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

#ifndef UWS_CLIENTAPP_H
#define UWS_CLIENTAPP_H

#include "Loop.h"
#include "WebSocket.h"
#include "HttpClientConnection.h"
#include "HttpClientContext.h"
#include "App.h"  // For SocketContextOptions
#include "MoveOnlyFunction.h"
#include <string>
#include <string_view>

namespace uWS {

/* Client application for WebSocket and HTTP client connections */
template <bool SSL, typename USERDATA = void>
struct ClientApp {
private:
    /* SSL options for context creation */
    SocketContextOptions sslOptions;
    
    /* Create a new HTTP client context with behavior configuration */
    HttpClientContext<SSL>* createHttpClientContext(const HttpClientBehavior<SSL, USERDATA>& behavior = {}) {
        return HttpClientContext<SSL>::create(Loop::get(), behavior, sslOptions);
    }
    
public:
    /* Constructor with SSL options */
    ClientApp(SocketContextOptions options = {}) : sslOptions(options) {}
    
    /* Move constructor */
    ClientApp(ClientApp&& other) noexcept {
        sslOptions = other.sslOptions;
    }
    
    /* Move assignment */
    ClientApp& operator=(ClientApp&& other) noexcept {
        if (this != &other) {
            sslOptions = other.sslOptions;
        }
        return *this;
    }
    
    /* Destructor */
    ~ClientApp() = default;
    
    /* WebSocket connection methods */
    
    /* Connect to WebSocket */
    WebSocket<SSL, false, void>* connectWS(const char* host, int port, const char* path = "/") {
        return WebSocket<SSL, false, void>::connect(Loop::get(), host, port, path);
    }
    
    /* Connect to WebSocket with IP address */
    WebSocket<SSL, false, void>* connectWS(const char* ip, int port, bool isIPv6, const char* path = "/") {
        return WebSocket<SSL, false, void>::connect(Loop::get(), ip, port, isIPv6, path);
    }
    
    /* Connect to Unix domain WebSocket */
    WebSocket<SSL, false, void>* connectWSUnix(const char* server_path, const char* path = "/") {
        return WebSocket<SSL, false, void>::connectUnix(Loop::get(), server_path, path);
    }
    
    /* HTTP client connection methods */
    
    /* Connect to HTTP server */
    HttpClientConnection<SSL, USERDATA>* connect(const char* host, int port) {
        auto* context = createHttpClientContext();
        if (!context) { return nullptr; }
        
        auto* socket = context->connect(host, port);
        if (!socket) { return nullptr; }
        
        return static_cast<HttpClientConnection<SSL, USERDATA>*>(socket);
    }
    
    /* Connect to HTTP server with IP address */
    HttpClientConnection<SSL, USERDATA>* connect(const char* ip, int port, bool isIPv6) {
        auto* context = createHttpClientContext();
        if (!context) { return nullptr; }
        
        auto* socket = context->connect(ip, port);
        if (!socket) { return nullptr; }
        
        return static_cast<HttpClientConnection<SSL, USERDATA>*>(socket);
    }
    
    /* Connect to Unix domain HTTP server */
    HttpClientConnection<SSL, USERDATA>* connectUnix(const char* server_path) {
        auto* context = createHttpClientContext();
        if (!context) { return nullptr; }
        
        auto* socket = context->connectUnix(server_path);
        if (!socket) { return nullptr; }
        
        return static_cast<HttpClientConnection<SSL, USERDATA>*>(socket);
    }
    
    /* Get SSL options */
    const SocketContextOptions& getSSLOptions() const {
        return sslOptions;
    }
    
    /* Set SSL options */
    void setSSLOptions(const SocketContextOptions& options) {
        sslOptions = options;
    }
};

/* Type aliases for convenience */
using ClientAppType = ClientApp<false, void>;
using SSLClientAppType = ClientApp<true, void>;

} // namespace uWS

#endif // UWS_CLIENTAPP_H