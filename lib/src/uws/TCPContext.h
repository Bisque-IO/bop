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
#include "TCPContextData.h"
#include "libusockets.h"
#include <mutex>
#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPContextData;

/* TCP behavior configuration for contexts */
template <bool SSL, typename USERDATA = void>
struct TCPBehavior {
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
};

/* Helper function to calculate connection data size */
template <bool SSL, typename USERDATA>
constexpr size_t getTCPConnectionDataSize() {
    if constexpr (std::is_same_v<USERDATA, void>) {
        return alignof(TCPConnectionData<SSL, USERDATA>) >= 16
            ? ((sizeof(TCPConnectionData<SSL, USERDATA>) + 15) & ~size_t(15))
            : sizeof(TCPConnectionData<SSL, USERDATA>);
    } else {
        constexpr size_t base = sizeof(TCPConnectionData<SSL, USERDATA>);
        constexpr size_t user = sizeof(USERDATA);
        constexpr size_t total = base + user;
        return alignof(TCPConnectionData<SSL, USERDATA>) >= 16
            ? ((total + 15) & ~size_t(15))
            : total;
    }
}

/* TCP context for managing TCP connections */
template <bool SSL, typename USERDATA = void>
struct TCPContext {
    template <bool, typename> friend struct TemplatedTCPServerApp;
    template <bool, typename> friend struct TemplatedTCPClientApp;
    
private:
    /* Get the socket context */
    struct us_socket_context_t* getContext() {
        return reinterpret_cast<us_socket_context_t *>(this);
    }

    static struct us_socket_context_t* getContextS(us_socket_t* s) {
        return us_socket_context(SSL, s);
    }
    
    /* Get socket context data */
    TCPContextData<SSL, USERDATA>* getSocketContextData() {
        return reinterpret_cast<TCPContextData<SSL, USERDATA> *>(us_socket_context_ext(SSL, getContext()));
    }

    static TCPContextData<SSL, USERDATA>* getSocketContextDataS(us_socket_t* s) {
        return reinterpret_cast<TCPContextData<SSL, USERDATA> *>(us_socket_context_ext(SSL, getContextS(s)));
    }

    static TCPConnection<SSL, USERDATA>* getConnection(us_socket_t* s) {
        return reinterpret_cast<TCPConnection<SSL, USERDATA> *>(s);
    }

    static TCPConnectionData<SSL, USERDATA>* getConnectionData(us_socket_t* s) {
        return reinterpret_cast<TCPConnectionData<SSL, USERDATA> *>(us_socket_ext(SSL, s));
    }
    
    /* Initialize the context with behavior */
    TCPContext* init(bool isServer = false) {
        /* Set up connection handlers */
        us_socket_context_on_open(SSL, getContext(), [](us_socket_t* s, int is_client, char *ip, int ip_length) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            
            if (!is_client) {
                /* Initialize connection data for server connections (client connections already initialized at creation) */
                new (connData) TCPConnectionData<SSL, USERDATA>();
            }

            USERDATA* userData = conn->getUserData();
            
            connData->isClient = is_client > 0;
            connData->isConnected = true;
            
            /* Call onConnection handler if available */
            if (contextData->onConnection) {
                contextData->onConnection(conn, connData, userData);
            }
            
            return s;
        });
        
        us_socket_context_on_close(SSL, getContext(), [](struct us_socket_t* s, int code, void* data) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Call onDisconnected handler if available */
            if (contextData->onDisconnected) {
                contextData->onDisconnected(conn, connData, userData, code, data);
            }

            /* Clean up connection data (destructor will handle USERDATA cleanup) */
            connData->~TCPConnectionData<SSL, USERDATA>();
            
            return s;
        });
        
        us_socket_context_on_data(SSL, getContext(), [](struct us_socket_t* s, char* data, int length) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();

            connData->bytesReceived += length;
            ++connData->messagesReceived;
            
            /* Call onData handler if available */
            if (contextData->onData) {
                contextData->onData(conn, connData, userData, std::string_view(data, length));
            }
            
            return s;
        });

        /* Handle FIN, default to not supporting half-closed sockets, so simply close */
        us_socket_context_on_end(SSL, getContext(), [](struct us_socket_t *s) {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Call onEnd handler if available - returns true to auto-close */
            if (contextData->onEnd) {
                if (contextData->onEnd(conn, connData, userData)) {
                    return conn->close();
                }
                return s; // Don't auto-close if callback returns false
            }
            
            /* Default behavior: we do not care for half closed sockets */
            return conn->close();
        });
        
        us_socket_context_on_timeout(SSL, getContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);

            if (contextData->onTimeout) {
                contextData->onTimeout(conn, connData, conn->getUserData());
            }

            return s;
        });
        
        us_socket_context_on_long_timeout(SSL, getContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Call onLongTimeout handler if available */
            if (contextData->onLongTimeout) {
                contextData->onLongTimeout(conn, connData, userData);
            }
            
            return s;
        });
        
        us_socket_context_on_connect_error(SSL, getContext(), [](struct us_socket_t* s, int code) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
             /* Call onConnectError handler if available */
             if (contextData->onConnectError) {
                std::string_view message = "Connection failed";
                contextData->onConnectError(conn, connData, userData, code, message);
            }
            
            /* Clean up connection data (destructor will handle USERDATA cleanup) */
            connData->~TCPConnectionData<SSL, USERDATA>();
            
            return s;
        });
        
        /* Handle SNI (Server Name Indication) - only for server contexts */
        if (isServer) {
            us_socket_context_on_server_name(SSL, getContext(), [](struct us_socket_context_t* context, const char* hostname) {
                TCPContextData<SSL, USERDATA>* contextData = reinterpret_cast<TCPContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, context));
                
                /* Call onServerName handler if available */
                if (contextData->onServerName) {
                    std::string_view hostnameView = hostname ? std::string_view(hostname) : std::string_view("");
                    contextData->onServerName(reinterpret_cast<TCPContext<SSL, USERDATA>*>(context), hostnameView);
                }
            });
        }
        
        us_socket_context_on_writable(SSL, getContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Drain any remaining socket buffer */
            conn->write(nullptr, 0, true, 0);
            
            /* Call onWritable handler if available and backpressure is relieved */
            if (contextData->onWritable) {
                uintmax_t currentBackpressure = conn->getBufferedAmount();
                uintmax_t availableBytes = contextData->maxBackpressure > currentBackpressure ? 
                    contextData->maxBackpressure - currentBackpressure : 0;
                /* Call onWritable when backpressure falls below 50% of max backpressure or when completely drained */
                if (currentBackpressure == 0 || 
                    (contextData->maxBackpressure > 0 && currentBackpressure < contextData->maxBackpressure / 2)) {
                    contextData->onWritable(conn, connData, userData, availableBytes);
                }
            }
            
            return s;
        });
        
        return this;
    }
    
public:
    /* Construct a new TCPContext using specified loop */
    static TCPContext* create(Loop* loop, struct us_socket_context_options_t options = {}, bool isServer = false) {
        TCPContext<SSL, USERDATA> *tcpContext = reinterpret_cast<TCPContext<SSL, USERDATA> *>(us_create_socket_context(
            SSL, reinterpret_cast<struct us_loop_t *>(loop), sizeof(TCPContextData<SSL, USERDATA>), options
        ));
        
        if (!tcpContext) {
            return nullptr;
        }
        
        /* Init socket context data */
        new (reinterpret_cast<TCPContextData<SSL, USERDATA> *>(us_socket_context_ext(
            SSL, reinterpret_cast<us_socket_context_t *>(tcpContext)
        ))) TCPContextData<SSL, USERDATA>();
        
        return tcpContext->init(isServer);
    }
    
    /* Destruct the TCPContext */
    void free() {
        /* Destruct socket context data */
        TCPContextData<SSL, USERDATA>* contextData = getSocketContextData();
        contextData->~TCPContextData<SSL, USERDATA>();
        
        /* Free the socket context */
        us_socket_context_close(SSL, getContext());
        us_socket_context_free(SSL, getContext());
    }
    
    /* Connect to a server (client-side) */
    TCPConnection<SSL, USERDATA>* connect(const char* host, int port, const char* source_host = nullptr, int options = 0) {
        auto* socket = reinterpret_cast<TCPConnection<SSL, USERDATA>*>(us_socket_context_connect(
            SSL,
            getContext(),
            host,
            port,
            source_host,
            options,
            getTCPConnectionDataSize<SSL, USERDATA>()
        ));
        
        if (socket) {
            /* Initialize connection data immediately for proper RAII */
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(reinterpret_cast<us_socket_t*>(socket));
            new (connData) TCPConnectionData<SSL, USERDATA>();
            connData->isClient = true;  // Client connections are always true
        }
        
        return socket;
    }
    
    /* Connect to a Unix domain socket (client-side) */
    TCPConnection<SSL, USERDATA>* connectUnix(const char* server_path, int options = 0) {
        auto* socket = static_cast<TCPConnection<SSL, USERDATA>*>(us_socket_context_connect_unix(
            SSL, getContext(), server_path, options, getTCPConnectionDataSize<SSL, USERDATA>()
        ));
        
        if (socket) {
            /* Initialize connection data immediately for proper RAII */
            TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(reinterpret_cast<us_socket_t*>(socket));
            new (connData) TCPConnectionData<SSL, USERDATA>();
            connData->isClient = true;  // Client connections are always true
        }
        
        return socket;
    }
    
    /* Set context-level handlers */
    void onConnection(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)>&& handler) {
        getSocketContextData()->onConnection = std::move(handler);
    }

    void onConnectError(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, int, std::string_view)>&& handler) {
        getSocketContextData()->onConnectError = std::move(handler);
    }

    void onDisconnected(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, int, void*)>&& handler) {
        getSocketContextData()->onDisconnected = std::move(handler);
    }

    void onServerName(MoveOnlyFunction<void(TCPContext<SSL, USERDATA>*, std::string_view)>&& handler) {
        getSocketContextData()->onServerName = std::move(handler);
    }

    void onData(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, std::string_view)>&& handler) {
        getSocketContextData()->onData = std::move(handler);
    }

    void onWritable(MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, uintmax_t)>&& handler) {
        getSocketContextData()->onWritable = std::move(handler);
    }

    void onDrain(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)>&& handler) {
        getSocketContextData()->onDrain = std::move(handler);
    }

    void onDropped(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*, std::string_view)>&& handler) {
        getSocketContextData()->onDropped = std::move(handler);
    }

    void onEnd(MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)>&& handler) {
        getSocketContextData()->onEnd = std::move(handler);
    }

    void onTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)>&& handler) {
        getSocketContextData()->onTimeout = std::move(handler);
    }

    void onLongTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, TCPConnectionData<SSL, USERDATA>*, USERDATA*)>&& handler) {
        getSocketContextData()->onLongTimeout = std::move(handler);
    }
    
    /* Set timeout configuration */
    void setTimeout(uint32_t idleTimeoutSeconds, uint32_t longTimeoutMinutes) {
        TCPContextData<SSL, USERDATA>* data = getSocketContextData();
        data->idleTimeoutSeconds = idleTimeoutSeconds;
        data->longTimeoutMinutes = longTimeoutMinutes;
    }
    
    /* Get timeout configuration */
    std::pair<uint32_t, uint32_t> getTimeout() {
        const TCPContextData<SSL, USERDATA>* data = getSocketContextData();
        return {data->idleTimeoutSeconds, data->longTimeoutMinutes};
    }
    
    /* Set max backpressure */
    void setMaxBackpressure(uintmax_t maxBackpressure) {
        getSocketContextData()->maxBackpressure = maxBackpressure;
    }
    
    /* Get max backpressure */
    uintmax_t getMaxBackpressure() {
        const TCPContextData<SSL, USERDATA>* data = getSocketContextData();
        return data->maxBackpressure;
    }
    
    /* Set reset idle timeout on send */
    void setResetIdleTimeoutOnSend(bool reset) {
        getSocketContextData()->resetIdleTimeoutOnSend = reset;
    }
    
    /* Get reset idle timeout on send */
    bool getResetIdleTimeoutOnSend() {
        const TCPContextData<SSL, USERDATA>* data = getSocketContextData();
        return data->resetIdleTimeoutOnSend;
    }
    
    /* Set close on backpressure limit */
    void setCloseOnBackpressureLimit(bool close) {
        getSocketContextData()->closeOnBackpressureLimit = close;
    }
    
    /* Get close on backpressure limit */
    bool getCloseOnBackpressureLimit() {
        const TCPContextData<SSL, USERDATA>* data = getSocketContextData();
        return data->closeOnBackpressureLimit;
    }
};

} // namespace uWS

#endif // UWS_TCPCONTEXT_H
