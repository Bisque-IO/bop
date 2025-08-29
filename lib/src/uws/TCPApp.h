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

#ifndef UWS_TCPAPP_H
#define UWS_TCPAPP_H

#include "AsyncSocket.h"
#include "AsyncSocketData.h"
#include "Loop.h"
#include "MoveOnlyFunction.h"
#include "TCPConnection.h"
#include "TCPContext.h"
#include "App.h"  // For SocketContextOptions
#include <string>
#include <string_view>

namespace uWS {

/* Main TCP application class for both server and client functionality */
template <bool SSL, typename USERDATA = void>
struct TemplatedTCPApp {
private:
    /* SSL options for context creation */
    SocketContextOptions sslOptions;
    
    /* Create a new TCP context with behavior configuration */
    TCPContext<SSL, USERDATA>* createTCPContext(const TCPBehavior<SSL, USERDATA>& behavior = {}) {
        return TCPContext<SSL, USERDATA>::create(Loop::get(), behavior, sslOptions);
    }
    
public:
    /* Constructor with SSL options */
    TemplatedTCPApp(SocketContextOptions options = {}) : sslOptions(options) {}
    
    /* Move constructor */
    TemplatedTCPApp(TemplatedTCPApp&& other) noexcept {
        sslOptions = other.sslOptions;
    }
    
    /* Move assignment */
    TemplatedTCPApp& operator=(TemplatedTCPApp&& other) noexcept {
        if (this != &other) {
            sslOptions = other.sslOptions;
        }
        return *this;
    }
    
    /* Destructor */
    ~TemplatedTCPApp() = default;
    
    /* Server-side: Listen methods following App.h semantics */
    TemplatedTCPApp&& listen(std::string host, int port, MoveOnlyFunction<void(us_listen_socket_t*)>&& handler) {
        if (host == "localhost") {
            return listen(port, std::move(handler));
        }
        auto* context = createTCPContext();
        handler(context ? context->listen(host.c_str(), port, 0) : nullptr);
        return std::move(*this);
    }
    
    TemplatedTCPApp&& listen(std::string host, int port, int options, MoveOnlyFunction<void(us_listen_socket_t*)>&& handler) {
        if (host == "localhost") {
            return listen(port, options, std::move(handler));
        }
        auto* context = createTCPContext();
        handler(context ? context->listen(host.c_str(), port, options) : nullptr);
        return std::move(*this);
    }
    
    TemplatedTCPApp&& listen(int port, MoveOnlyFunction<void(us_listen_socket_t*)>&& handler) {
        auto* context = createTCPContext();
        handler(context ? context->listen(nullptr, port, 0) : nullptr);
        return std::move(*this);
    }
    
    TemplatedTCPApp&& listen(int port, int options, MoveOnlyFunction<void(us_listen_socket_t*)>&& handler) {
        auto* context = createTCPContext();
        handler(context ? context->listen(nullptr, port, options) : nullptr);
        return std::move(*this);
    }
    
    TemplatedTCPApp&& listen(int options, MoveOnlyFunction<void(us_listen_socket_t*)>&& handler, std::string path) {
        auto* context = createTCPContext();
        handler(context ? context->listenUnix(path.c_str(), options) : nullptr);
        return std::move(*this);
    }
    
    TemplatedTCPApp&& listen(MoveOnlyFunction<void(us_listen_socket_t*)>&& handler, std::string path) {
        auto* context = createTCPContext();
        handler(context ? context->listenUnix(path.c_str(), 0) : nullptr);
        return std::move(*this);
    }
    
    /* Client-side: Connect methods */
    us_socket_t* connect(const char* host, int port, 
                        const char* source_host = nullptr, int options = 0) {
        auto* context = createTCPContext();
        if (!context) return nullptr;
        
        return context->connect(host, port, source_host, options);
    }
    
    us_socket_t* connectUnix(const char* server_path, int options = 0) {
        auto* context = createTCPContext();
        if (!context) return nullptr;
        
        return context->connectUnix(server_path, options);
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
using TCPApp = TemplatedTCPApp<false, void>;
using SSLTCPApp = TemplatedTCPApp<true, void>;

} // namespace uWS

#endif // UWS_TCPAPP_H
