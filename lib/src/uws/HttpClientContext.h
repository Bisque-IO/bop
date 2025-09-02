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

#ifndef UWS_HTTPCLIENTCONTEXT_H
#define UWS_HTTPCLIENTCONTEXT_H

#include "AsyncSocket.h"
#include "AsyncSocketData.h"
#include "Loop.h"
#include "MoveOnlyFunction.h"
#include "HttpClientConnection.h"
#include "HttpClientContextData.h"
#include "HttpParser.h"
#include <string>
#include <string_view>

namespace uWS {



/* HTTP client behavior configuration for contexts */
template <bool SSL, typename USERDATA = void>
struct HttpClientBehavior {
    /* Connection event callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onConnected = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onDisconnected = nullptr;
    
    /* HTTP response callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view, HttpResponseHeaders&)> onHeaders = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view, bool)> onChunk = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onError = nullptr;
    
    /* Timeout callbacks - default handlers for all connections */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onTimeout = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onLongTimeout = nullptr;
    
    /* Data flow callbacks - default handlers for all connections */
    MoveOnlyFunction<bool(HttpClientConnection<SSL, USERDATA>*, uintmax_t)> onWritable = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onDropped = nullptr;
    
    /* Default configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
};

/* HTTP client context for managing HTTP client connections */
template <bool SSL, typename USERDATA = void>
struct HttpClientContext {
private:
    /* Get the socket context */
    us_socket_context_t* getSocketContext() {
        return (us_socket_context_t*)this;
    }
    
    static us_socket_context_t* getSocketContext(us_socket_t* s) {
        return us_socket_context(SSL, s);
    }
    
    /* Get socket context data */
    HttpClientContextData<SSL, USERDATA>* getSocketContextData() {
        return (HttpClientContextData<SSL, USERDATA>*)us_socket_context_ext(SSL, getSocketContext());
    }
    
    static HttpClientContextData<SSL, USERDATA>* getSocketContextDataS(us_socket_t* s) {
        return (HttpClientContextData<SSL, USERDATA>*)us_socket_context_ext(SSL, getSocketContext(s));
    }
    
    /* Initialize the context with behavior */
    HttpClientContext* init(const HttpClientBehavior<SSL, USERDATA>& behavior) {
        /* Set up connection handlers */
        us_socket_context_on_open(SSL, getSocketContext(), [](us_socket_t* s, int is_client) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Call onConnection handler if available */
            if (contextData->onConnection) {
                contextData->onConnection(static_cast<HttpClientConnection<SSL, USERDATA>*>(s));
            }
        });
        
        us_socket_context_on_close(SSL, getSocketContext(), [](us_socket_t* s, int code, void* data) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Call onDisconnected handler if available */
            if (contextData->onDisconnected) {
                std::string_view message = data ? std::string_view((char*)data, strlen((char*)data)) : std::string_view();
                contextData->onDisconnected(static_cast<HttpClientConnection<SSL, USERDATA>*>(s), code, message);
            }
            
            /* Clean up connection data (destructor will handle USERDATA cleanup) */
            connData->~HttpClientConnectionData<SSL, USERDATA>();
        });
        
        us_socket_context_on_data(SSL, getSocketContext(), [](us_socket_t* s, char* data, int length) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Parse HTTP response data */
            static_cast<HttpClientConnection<SSL, USERDATA>*>(s)->parseResponseData(data, length);
        });
        
        us_socket_context_on_timeout(SSL, getSocketContext(), [](us_socket_t* s) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Call onTimeout handler if available */
            if (contextData->onTimeout) {
                contextData->onTimeout(static_cast<HttpClientConnection<SSL, USERDATA>*>(s));
            } else if (contextData->onResponseEnd) {
                /* Fallback to onResponseEnd handler if onTimeout not set */
                contextData->onResponseEnd(static_cast<HttpClientConnection<SSL, USERDATA>*>(s), "Connection timeout");
            }
        });
        
        us_socket_context_on_long_timeout(SSL, getSocketContext(), [](us_socket_t* s) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Call onLongTimeout handler if available */
            if (contextData->onLongTimeout) {
                contextData->onLongTimeout(static_cast<HttpClientConnection<SSL, USERDATA>*>(s));
            } else if (contextData->onResponseEnd) {
                /* Fallback to onResponseEnd handler if onLongTimeout not set */
                contextData->onResponseEnd(static_cast<HttpClientConnection<SSL, USERDATA>*>(s), "Connection long timeout");
            }
        });
        
        us_socket_context_on_connect_error(SSL, getSocketContext(), [](us_socket_t* s, int code) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            
            /* Call onConnectError handler if available */
            if (contextData->onConnectError) {
                std::string_view message = "Connection failed";
                contextData->onConnectError(static_cast<HttpClientConnection<SSL, USERDATA>*>(s), code, message);
            }
        });
        
        us_socket_context_on_writable(SSL, getSocketContext(), [](us_socket_t* s) {
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(s);
            HttpClientContextData<SSL, USERDATA>* contextData = static_cast<HttpClientContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, us_socket_context(SSL, s)));
            auto* asyncSocket = static_cast<AsyncSocket<SSL>*>(s);
            
            /* Drain any remaining socket buffer */
            asyncSocket->write(nullptr, 0, true, 0);
            
            /* Call onWritable handler if available and backpressure is relieved */
            if (contextData->onWritable) {
                uintmax_t currentBackpressure = asyncSocket->getBufferedAmount();
                uintmax_t availableBytes = contextData->maxBackpressure > currentBackpressure ? 
                    contextData->maxBackpressure - currentBackpressure : 0;
                /* Call onWritable when backpressure falls below 50% of max backpressure or when completely drained */
                if (currentBackpressure == 0 || 
                    (contextData->maxBackpressure > 0 && currentBackpressure < contextData->maxBackpressure / 2)) {
                    contextData->onWritable(static_cast<HttpClientConnection<SSL, USERDATA>*>(s), availableBytes);
                }
            }
        });
        
        return this;
    }
    
public:
    /* Construct a new HttpClientContext using specified loop */
    static HttpClientContext* create(Loop* loop, const HttpClientBehavior<SSL, USERDATA>& behavior = {}, us_socket_context_options_t options = {}) {
        HttpClientContext<SSL, USERDATA>* httpContext;
        
        httpContext = (HttpClientContext*)us_create_socket_context(
            SSL, (us_loop_t*)loop, sizeof(HttpClientContextData<SSL, USERDATA>), options
        );
        
        if (!httpContext) {
            return nullptr;
        }
        
        /* Init socket context data */
        new ((HttpClientContextData<SSL, USERDATA>*)us_socket_context_ext(
            SSL, (us_socket_context_t*)httpContext
        )) HttpClientContextData<SSL, USERDATA>();
        
        /* Apply behavior configuration */
        HttpClientContextData<SSL, USERDATA>* contextData = (HttpClientContextData<SSL, USERDATA>*)us_socket_context_ext(
            SSL, (us_socket_context_t*)httpContext
        );
        contextData->idleTimeoutSeconds = behavior.idleTimeoutSeconds;
        contextData->longTimeoutMinutes = behavior.longTimeoutMinutes;
        contextData->maxBackpressure = behavior.maxBackpressure;
        contextData->resetIdleTimeoutOnSend = behavior.resetIdleTimeoutOnSend;
        contextData->closeOnBackpressureLimit = behavior.closeOnBackpressureLimit;
        
        /* Apply callbacks */
        contextData->onConnection = std::move(behavior.onConnected);
        contextData->onConnectError = std::move(behavior.onConnectError);
        contextData->onDisconnected = std::move(behavior.onDisconnected);
        contextData->onResponse = std::move(behavior.onHeaders);
        contextData->onResponseData = std::move(behavior.onChunk);
        contextData->onResponseEnd = std::move(behavior.onError);
        contextData->onWritable = std::move(behavior.onWritable);
        contextData->onDropped = std::move(behavior.onDropped);
        contextData->onTimeout = std::move(behavior.onTimeout);
        contextData->onLongTimeout = std::move(behavior.onLongTimeout);
        
        return httpContext->init(behavior);
    }
    
    /* Destruct the HttpClientContext */
    void free() {
        /* Destruct socket context data */
        HttpClientContextData<SSL, USERDATA>* contextData = getSocketContextData();
        contextData->~HttpClientContextData<SSL, USERDATA>();
        
        /* Free the socket context */
        us_socket_context_free(SSL, getSocketContext());
    }
    
    /* Connect to a server (client-side) */
    HttpClientConnection<SSL, USERDATA>* connect(const char* host, int port, const char* source_host = nullptr, int options = 0) {
        auto* socket = static_cast<HttpClientConnection<SSL, USERDATA>*>(us_socket_context_connect(
            SSL,
            getSocketContext(),
            host,
            port,
            source_host,
            options,
            sizeof(HttpClientConnectionData<SSL, USERDATA>) + sizeof(USERDATA)
        ));
        
        if (socket) {
            /* Initialize connection data immediately for proper RAII */
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(socket);
            new (connData) HttpClientConnectionData<SSL, USERDATA>();
        }
        
        return socket;
    }
    
    /* Connect to a Unix domain socket (client-side) */
    HttpClientConnection<SSL, USERDATA>* connectUnix(const char* server_path, int options = 0) {
        auto* socket = static_cast<HttpClientConnection<SSL, USERDATA>*>(us_socket_context_connect_unix(
            SSL, getSocketContext(), server_path, options, sizeof(HttpClientConnectionData<SSL, USERDATA>) + sizeof(USERDATA)
        ));
        
        if (socket) {
            /* Initialize connection data immediately for proper RAII */
            HttpClientConnectionData<SSL, USERDATA>* connData = HttpClientConnection<SSL, USERDATA>::getConnectionData(socket);
            new (connData) HttpClientConnectionData<SSL, USERDATA>();
        }
        
        return socket;
    }
    
    /* Set context-level handlers */
    void onConnection(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onConnection = std::move(handler);
    }
    
    void onConnectError(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getSocketContextData()->onConnectError = std::move(handler);
    }
    
    void onDisconnected(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getSocketContextData()->onDisconnected = std::move(handler);
    }
    
    void onResponse(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view, std::string_view)>&& handler) {
        getSocketContextData()->onResponse = std::move(handler);
    }
    
    void onResponseData(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onResponseData = std::move(handler);
    }
    
    void onResponseEnd(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onResponseEnd = std::move(handler);
    }
    
    void onData(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onData = std::move(handler);
    }
    
    void onWritable(MoveOnlyFunction<bool(HttpClientConnection<SSL, USERDATA>*, uintmax_t)>&& handler) {
        getSocketContextData()->onWritable = std::move(handler);
    }
    
    void onDrain(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onDrain = std::move(handler);
    }
    
    void onDropped(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onDropped = std::move(handler);
    }
    
    void onTimeout(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onTimeout = std::move(handler);
    }
    
    void onLongTimeout(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onLongTimeout = std::move(handler);
    }
    
    /* Set timeout configuration */
    void setTimeout(uint32_t idleTimeoutSeconds, uint32_t longTimeoutMinutes) {
        HttpClientContextData<SSL, USERDATA>* data = getSocketContextData();
        data->idleTimeoutSeconds = idleTimeoutSeconds;
        data->longTimeoutMinutes = longTimeoutMinutes;
    }
    
    /* Get timeout configuration */
    std::pair<uint32_t, uint32_t> getTimeout() const {
        const HttpClientContextData<SSL, USERDATA>* data = getSocketContextData();
        return {data->idleTimeoutSeconds, data->longTimeoutMinutes};
    }
    
    /* Set max backpressure */
    void setMaxBackpressure(uintmax_t maxBackpressure) {
        getSocketContextData()->maxBackpressure = maxBackpressure;
    }
    
    /* Get max backpressure */
    uintmax_t getMaxBackpressure() const {
        return getSocketContextData()->maxBackpressure;
    }
    
    /* Set reset idle timeout on send */
    void setResetIdleTimeoutOnSend(bool reset) {
        getSocketContextData()->resetIdleTimeoutOnSend = reset;
    }
    
    /* Get reset idle timeout on send */
    bool getResetIdleTimeoutOnSend() const {
        return getSocketContextData()->resetIdleTimeoutOnSend;
    }
    
    /* Set close on backpressure limit */
    void setCloseOnBackpressureLimit(bool close) {
        getSocketContextData()->closeOnBackpressureLimit = close;
    }
    
    /* Get close on backpressure limit */
    bool getCloseOnBackpressureLimit() const {
        return getSocketContextData()->closeOnBackpressureLimit;
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONTEXT_H
