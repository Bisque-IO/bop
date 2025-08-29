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

#ifndef UWS_TCPCONTEXT_H
#define UWS_TCPCONTEXT_H

#include "AsyncSocket.h"
#include "AsyncSocketData.h"
#include "Loop.h"
#include "MoveOnlyFunction.h"
#include "TCPConnection.h"
#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPConnectionData;

/* TCP behavior configuration for contexts */
template <bool SSL, typename USERDATA = void>
struct TCPBehavior {
    /* Connection callbacks */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onConnection = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)> onDisconnected = nullptr;
    
    /* Data flow callbacks */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)> onData = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onWritable = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onDrain = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)> onDropped = nullptr;
    
    /* Timeout callbacks */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onTimeout = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onLongTimeout = nullptr;
    
    /* Default configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
};

/* TCP context for managing TCP connections */
template <bool SSL, typename USERDATA = void>
struct TCPContext {
private:
    /* Get the socket context */
    us_socket_context_t* getSocketContext() {
        return (us_socket_context_t*)this;
    }
    
    static us_socket_context_t* getSocketContext(us_socket_t* s) {
        return us_socket_context(SSL, s);
    }
    
    /* Get socket context data */
    TCPConnectionData<SSL, USERDATA>* getSocketContextData() {
        return (TCPConnectionData<SSL, USERDATA>*)us_socket_context_ext(SSL, getSocketContext());
    }
    
    static TCPConnectionData<SSL, USERDATA>* getSocketContextDataS(us_socket_t* s) {
        return (TCPConnectionData<SSL, USERDATA>*)us_socket_context_ext(SSL, getSocketContext(s));
    }
    
    /* Initialize the context with behavior */
    TCPContext* init(const TCPBehavior<SSL>& behavior) {
        /* Set up connection handlers */
        us_socket_context_on_open(SSL, getSocketContext(), [](us_socket_t* s, int is_client) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            connData->isClient = is_client;
            
            /* Call onConnection handler if available */
            if (connData->onConnection) {
                connData->onConnection(static_cast<TCPConnection<SSL, USERDATA>*>(s));
            }
        });
        
        us_socket_context_on_close(SSL, getSocketContext(), [](us_socket_t* s, int code, void* data) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            
            /* Call onDisconnected handler if available */
            if (connData->onDisconnected) {
                connData->onDisconnected(connData, code, data);
            }
            
            /* Clean up USERDATA if not void */
            if constexpr (!std::is_same_v<USERDATA, void>) {
                ((USERDATA*)TCPConnection<SSL, USERDATA>::getUserData(s))->~USERDATA();
            }
        });
        
        us_socket_context_on_data(SSL, getSocketContext(), [](us_socket_t* s, char* data, int length) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            connData->bytesReceived += length;
            connData->messagesReceived++;
            
            /* Call onData handler if available */
            if (connData->onData) {
                connData->onData(connData, std::string_view(data, length));
            }
        });
        
        us_socket_context_on_timeout(SSL, getSocketContext(), [](us_socket_t* s) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            
            /* Call onTimeout handler if available */
            if (connData->onTimeout) {
                connData->onTimeout(connData);
            }
        });
        
        us_socket_context_on_long_timeout(SSL, getSocketContext(), [](us_socket_t* s) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            
            /* Call onLongTimeout handler if available */
            if (connData->onLongTimeout) {
                connData->onLongTimeout(connData);
            }
        });
        
        us_socket_context_on_connect_error(SSL, getSocketContext(), [](us_socket_t* s, int code) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            
            /* Call onConnectError handler if available */
            if (connData->onConnectError) {
                connData->onConnectError(connData, code);
            }
        });
        
        us_socket_context_on_writable(SSL, getSocketContext(), [](us_socket_t* s) {
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(s);
            auto* asyncSocket = static_cast<AsyncSocket<SSL>*>(s);
            
            /* Drain any remaining socket buffer */
            asyncSocket->write(nullptr, 0, true, 0);
            
            /* Call onWritable handler if available and backpressure is relieved */
            if (connData->onWritable) {
                uintmax_t currentBackpressure = asyncSocket->getBufferedAmount();
                /* Call onWritable when backpressure falls below 50% of max backpressure or when completely drained */
                if (currentBackpressure == 0 || 
                    (connData->maxBackpressure > 0 && currentBackpressure < connData->maxBackpressure / 2)) {
                    connData->onWritable(static_cast<TCPConnection<SSL, USERDATA>*>(s));
                }
            }
        });
        
        return this;
    }
    
public:
    /* Construct a new TCPContext using specified loop */
    static TCPContext* create(Loop* loop, const TCPBehavior<SSL, USERDATA>& behavior = {}, us_socket_context_options_t options = {}) {
        TCPContext* tcpContext;
        
        tcpContext = (TCPContext*)us_create_socket_context(
            SSL, (us_loop_t*)loop, sizeof(TCPConnectionData<SSL, USERDATA>) + sizeof(USERDATA), options
        );
        
        if (!tcpContext) {
            return nullptr;
        }
        
        /* Init socket context data */
        new ((TCPConnectionData<SSL, USERDATA>*)us_socket_context_ext(
            SSL, (us_socket_context_t*)tcpContext
        )) TCPConnectionData<SSL, USERDATA>();
        
        /* Apply behavior configuration */
        TCPConnectionData<SSL, USERDATA>* contextData = (TCPConnectionData<SSL, USERDATA>*)us_socket_context_ext(
            SSL, (us_socket_context_t*)tcpContext
        );
        
        /* Apply configuration */
        contextData->idleTimeoutSeconds = behavior.idleTimeoutSeconds;
        contextData->longTimeoutMinutes = behavior.longTimeoutMinutes;
        contextData->maxBackpressure = behavior.maxBackpressure;
        contextData->resetIdleTimeoutOnSend = behavior.resetIdleTimeoutOnSend;
        contextData->closeOnBackpressureLimit = behavior.closeOnBackpressureLimit;
        
        /* Apply callbacks */
        contextData->onConnection = std::move(behavior.onConnection);
        contextData->onConnectError = std::move(behavior.onConnectError);
        contextData->onDisconnected = std::move(behavior.onDisconnected);
        contextData->onData = std::move(behavior.onData);
        contextData->onWritable = std::move(behavior.onWritable);
        contextData->onDrain = std::move(behavior.onDrain);
        contextData->onDropped = std::move(behavior.onDropped);
        contextData->onTimeout = std::move(behavior.onTimeout);
        contextData->onLongTimeout = std::move(behavior.onLongTimeout);
        
        return tcpContext->init(behavior);
    }
    
    /* Destruct the TCPContext */
    void free() {
        /* Destruct socket context data */
        TCPConnectionData<SSL, USERDATA>* connData = getSocketContextData();
        connData->~TCPConnectionData<SSL, USERDATA>();
        
        /* Free the socket context */
        us_socket_context_free(SSL, getSocketContext());
    }
    
    /* Listen for incoming connections (server-side) */
    us_listen_socket_t* listen(const char* host, int port, int options = 0) {
        return us_socket_context_listen(SSL, getSocketContext(), host, port, options);
    }
    
    /* Listen on Unix domain socket (server-side) */
    us_listen_socket_t* listenUnix(const char* server_path, int options = 0) {
        return us_socket_context_listen_unix(SSL, getSocketContext(), server_path, options);
    }
    
    /* Connect to a server (client-side) */
    us_socket_t* connect(const char* host, int port, const char* source_host = nullptr, int options = 0) {
        return us_socket_context_connect(
            SSL,
            getSocketContext(),
            host,
            port,
            source_host,
            options,
            sizeof(TCPConnectionData<SSL, USERDATA>) + sizeof(USERDATA)
        );
    }
    
    /* Connect to a Unix domain socket (client-side) */
    us_socket_t* connectUnix(const char* server_path, int options = 0) {
        return us_socket_context_connect_unix(
            SSL, getSocketContext(), server_path, options, sizeof(TCPConnectionData<SSL, USERDATA>) + sizeof(USERDATA)
        );
    }
    
    /* Set context-level handlers */
    void onConnection(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onConnection = std::move(handler);
    }
    
    void onConnectError(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getSocketContextData()->onConnectError = std::move(handler);
    }
    
    void onDisconnected(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getSocketContextData()->onDisconnected = std::move(handler);
    }
    
    void onData(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onData = std::move(handler);
    }
    
    void onWritable(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onWritable = std::move(handler);
    }
    
    void onDrain(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onDrain = std::move(handler);
    }
    
    void onDropped(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onDropped = std::move(handler);
    }
    
    void onTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onTimeout = std::move(handler);
    }
    
    void onLongTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getSocketContextData()->onLongTimeout = std::move(handler);
    }
    
    /* Set timeout configuration */
    void setTimeout(uint32_t idleTimeoutSeconds, uint32_t longTimeoutMinutes) {
        TCPConnectionData<SSL, USERDATA>* data = getSocketContextData();
        data->idleTimeoutSeconds = idleTimeoutSeconds;
        data->longTimeoutMinutes = longTimeoutMinutes;
    }
    
    /* Get timeout configuration */
    std::pair<uint32_t, uint32_t> getTimeout() const {
        const TCPConnectionData<SSL, USERDATA>* data = getSocketContextData();
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

#endif // UWS_TCPCONTEXT_H
