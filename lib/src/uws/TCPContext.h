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

#undef _HAS_CXX17
#define _HAS_CXX17 1
// #include <__msvc_string_view.hpp>
#include <string>
#include <string_view>

#include "Loop.h"
#include "MoveOnlyFunction.h"
#include "TCPConnection.h"
#include "TCPContextData.h"
#include "libusockets.h"



namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPContextData;

/* TCP behavior configuration for contexts */
template <bool SSL, typename USERDATA = void>
struct TCPBehavior {
    /* Connection callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onOpen = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, void*)> onClose = nullptr;
    MoveOnlyFunction<void(TCPContext<SSL, USERDATA>*, std::string_view)> onServerName = nullptr;
    
    /* Data flow callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)> onData = nullptr;
    MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*, uintmax_t)> onWritable = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onDrain = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)> onDropped = nullptr;
    MoveOnlyFunction<bool(TCPConnection<SSL, USERDATA>*)> onEnd = nullptr;
    
    /* Timeout callbacks - default handlers for all connections */
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onTimeout = nullptr;
    MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)> onLongTimeout = nullptr;
    
    /* Default configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    int connUserDataSize = 16;
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
};

/* TCP context for managing TCP connections */
template <bool SSL, typename USERDATA = void>
struct TCPContext {
    template <bool, typename> friend struct TemplatedTCPServerApp;
    template <bool, typename> friend struct TemplatedTCPClientApp;
    
private:
    /* Get the socket context */
    struct us_socket_context_t* getSocketContext() {
        return reinterpret_cast<us_socket_context_t *>(this);
    }

    static struct us_socket_context_t* getSocketContextS(us_socket_t* s) {
        return us_socket_context(SSL, s);
    }

    static TCPContextData<SSL, USERDATA>* getSocketContextDataS(us_socket_t* s) {
        return reinterpret_cast<TCPContextData<SSL, USERDATA> *>(us_socket_context_ext(SSL, getSocketContextS(s)));
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
        us_socket_context_on_open(SSL, getSocketContext(), [](us_socket_t* s, int is_client, char *ip, int ip_length) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            
            if (!is_client) {
                /* Initialize connection data for server connections (client connections already initialized at creation) */
                new (connData) TCPConnectionData<SSL, USERDATA>();

                memset(connData + 1, 0, contextData->connUserDataSize);
            }
            
            connData->isClient = is_client != 0;
            connData->isConnected = true;
            connData->idleTimeoutSeconds = contextData->idleTimeoutSeconds;
            connData->longTimeoutMinutes = contextData->longTimeoutMinutes;
            connData->maxBackpressure = contextData->maxBackpressure;
            connData->resetIdleTimeoutOnSend = contextData->resetIdleTimeoutOnSend;
            connData->closeOnBackpressureLimit = contextData->closeOnBackpressureLimit;
            connData->remoteAddress = std::string(ip, (size_t)ip_length);
            connData->remotePort = us_socket_remote_port(SSL, s);
            
            /* Call onConnection handler if available */
            if (contextData->onConnection) {
                contextData->onConnection(conn);
            }
            
            return s;
        });
        
        us_socket_context_on_close(SSL, getSocketContext(), [](struct us_socket_t* s, int code, void* data) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Call onDisconnected handler if available */
            if (contextData->onDisconnected) {
                contextData->onDisconnected(conn, code, data);
            }

            /* Clean up connection data (destructor will handle USERDATA cleanup) */
            connData->~TCPConnectionData<SSL, USERDATA>();
            
            return s;
        });
        
        us_socket_context_on_data(SSL, getSocketContext(), [](struct us_socket_t* s, char* data, int length) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();

            connData->bytesReceived += length;
            ++connData->messagesReceived;
            
            /* Call onData handler if available */
            if (contextData->onData) {
                contextData->onData(conn, std::string_view(data, length));
            }
            
            return s;
        });

        /* Handle FIN, default to not supporting half-closed sockets, so simply close */
        us_socket_context_on_end(SSL, getSocketContext(), [](struct us_socket_t *s) {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            USERDATA* userData = conn->getUserData();
            
            /* Call onEnd handler if available - returns true to auto-close */
            if (contextData->onEnd) {
                if (contextData->onEnd(conn)) {
                    return conn->close();
                }
                return s; // Don't auto-close if callback returns false
            }
            
            /* Default behavior: we do not care for half closed sockets */
            return conn->close();
        });
        
        us_socket_context_on_timeout(SSL, getSocketContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);

            if (contextData->onTimeout) {
                contextData->onTimeout(conn);
            }

            return s;
        });
        
        us_socket_context_on_long_timeout(SSL, getSocketContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            
            /* Call onLongTimeout handler if available */
            if (contextData->onLongTimeout) {
                contextData->onLongTimeout(conn);
            }
            
            return s;
        });
        
        us_socket_context_on_connect_error(SSL, getSocketContext(), [](struct us_socket_t* s, int code) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            
             /* Call onConnectError handler if available */
             if (contextData->onConnectError) {
                std::string_view message = "Connection failed";
                contextData->onConnectError(conn, code, message);
            }
            
            /* Clean up connection data (destructor will handle USERDATA cleanup) */
            connData->~TCPConnectionData<SSL, USERDATA>();
            
            return s;
        });
        
        /* Handle SNI (Server Name Indication) - only for server contexts */
        if (isServer) {
            us_socket_context_on_server_name(SSL, getSocketContext(), [](struct us_socket_context_t* context, const char* hostname) {
                TCPContextData<SSL, USERDATA>* contextData = reinterpret_cast<TCPContextData<SSL, USERDATA>*>(us_socket_context_ext(SSL, context));
                
                /* Call onServerName handler if available */
                if (contextData->onServerName) {
                    std::string_view hostnameView = hostname ? std::string_view(hostname) : std::string_view("");
                    contextData->onServerName(reinterpret_cast<TCPContext<SSL, USERDATA>*>(context), hostnameView);
                }
            });
        }
        
        us_socket_context_on_writable(SSL, getSocketContext(), [](struct us_socket_t* s) -> struct us_socket_t* {
            TCPConnection<SSL, USERDATA>* conn = getConnection(s);
            TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
            TCPContextData<SSL, USERDATA>* contextData = getSocketContextDataS(s);
            
            /* Drain any remaining socket buffer */
            conn->write(nullptr, 0, true, 0);
            
            /* Call onWritable handler if available and backpressure is relieved */
            if (contextData->onWritable) {
                auto maxBackpressure = connData->maxBackpressure ? connData->maxBackpressure : contextData->maxBackpressure;
                uintmax_t currentBackpressure = conn->getBufferedAmount();
                uintmax_t availableBytes = maxBackpressure > currentBackpressure ? 
                maxBackpressure - currentBackpressure : 0;
                /* Call onWritable when backpressure falls below 50% of max backpressure or when completely drained */
                if (currentBackpressure == 0 || 
                    (maxBackpressure > 0 && currentBackpressure < maxBackpressure / 2)) {
                    contextData->onWritable(conn, availableBytes);
                }
            }
            
            return s;
        });
        
        return this;
    }

    TCPConnectionData<SSL, USERDATA>* initData(TCPConnection<SSL, USERDATA>* conn) {
        /* Initialize connection data immediately for proper RAII */
        TCPConnectionData<SSL, USERDATA>* connData = TCPConnection<SSL, USERDATA>::getConnectionData(reinterpret_cast<us_socket_t*>(conn));
        new (connData) TCPConnectionData<SSL, USERDATA>();
        connData->isClient = true;
        connData->maxBackpressure = getSocketContextData()->maxBackpressure;
        connData->resetIdleTimeoutOnSend = getSocketContextData()->resetIdleTimeoutOnSend;
        connData->closeOnBackpressureLimit = getSocketContextData()->closeOnBackpressureLimit;
        connData->idleTimeoutSeconds = getSocketContextData()->idleTimeoutSeconds;
        connData->longTimeoutMinutes = getSocketContextData()->longTimeoutMinutes;
        conn->timeout(connData->idleTimeoutSeconds);
        conn->longTimeout(connData->longTimeoutMinutes);
        /* Zero out the user data */
        memset(connData + 1, 0, getSocketContextData()->connUserDataSize);
        return connData;
    }
    
public:
    /* Get socket context data */
    TCPContextData<SSL, USERDATA>* getSocketContextData() {
        return reinterpret_cast<TCPContextData<SSL, USERDATA> *>(us_socket_context_ext(SSL, getSocketContext()));
    }

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
        us_socket_context_close(SSL, getSocketContext());
        us_socket_context_free(SSL, getSocketContext());
    }
    
    /* Connect to a server (client-side) */
    TCPConnection<SSL, USERDATA>* connect(const char* host, int port, const char* source_host = nullptr, int options = 0) {
        TCPConnection<SSL, USERDATA>* socket = reinterpret_cast<TCPConnection<SSL, USERDATA>*>(us_socket_context_connect(
            SSL,
            getSocketContext(),
            host,
            port,
            source_host,
            options,
            getSocketContextData()->connExtSize
        ));
        
        if (socket) {
            initData(socket);
        }
        
        return socket;
    }

    /* Connect to a server (client-side) */
    TCPConnection<SSL, USERDATA>* connectIP4(uint32_t host_ip, int port, uint32_t source_host_ip = 0, int options = 0) {
        TCPConnection<SSL, USERDATA>* socket = reinterpret_cast<TCPConnection<SSL, USERDATA>*>(us_socket_context_connect_ip4(
            SSL,
            getSocketContext(),
            host_ip,
            port,
            source_host_ip,
            options,
            getSocketContextData()->connExtSize
        ));
        
        if (socket) {
            initData(socket);
        }
        
        return socket;
    }
    
    /* Connect to a Unix domain socket (client-side) */
    TCPConnection<SSL, USERDATA>* connectUnix(const char* server_path, int options = 0) {
        TCPConnection<SSL, USERDATA>* socket = static_cast<TCPConnection<SSL, USERDATA>*>(us_socket_context_connect_unix(
            SSL, getSocketContext(), server_path, options, getSocketContextData()->connExtSize
        ));
        
        if (socket) {
            initData(socket);
        }
        
        return socket;
    }

    /* Adopt an externally accepted socket into this HttpContext */
    us_socket_t *adoptAcceptedSocket(LIBUS_SOCKET_DESCRIPTOR accepted_fd) {
        us_socket_t* socket = us_adopt_accepted_socket(SSL, getSocketContext(), accepted_fd, getSocketContextData()->connExtSize, 0, 0);
        if (socket) {
            initData(socket);
        }
        return socket;
    }
};

} // namespace uWS

#endif // UWS_TCPCONTEXT_H
