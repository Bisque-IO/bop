/*
 * HttpClientConnection.h
 * 
 * HTTP client connection for managing HTTP client requests.
 */

#ifndef UWS_HTTPCLIENTCONNECTION_H
#define UWS_HTTPCLIENTCONNECTION_H

#include "AsyncSocket.h"
#include "HttpClientRequest.h"
#include "HttpClientConnectionData.h"
#include "MoveOnlyFunction.h"
#include "Loop.h"
#include <string>

namespace uWS {

/* Forward declarations */
template <bool SSL> struct HttpClientContext;

/* HTTP client connection */
template <bool SSL>
struct HttpClientConnection : public AsyncSocket<SSL> {
private:
    /* Get the connection data */
    HttpClientConnectionData<SSL>* getConnectionData() {
        return (HttpClientConnectionData<SSL>*)us_socket_ext(SSL, (us_socket_t*)this);
    }
    
    static HttpClientConnectionData<SSL>* getConnectionData(us_socket_t* s) {
        return (HttpClientConnectionData<SSL>*)us_socket_ext(SSL, s);
    }
    
public:
    /* Create connection */
    static HttpClientConnection<SSL>* create(Loop* loop) {
        HttpClientConnection* connection = (HttpClientConnection*)us_create_socket(
            SSL, (us_loop_t*)loop, sizeof(HttpClientConnectionData<SSL>)
        );
        
        if (!connection) {
            return nullptr;
        }
        
        /* Initialize connection data */
        new (getConnectionData((us_socket_t*)connection)) HttpClientConnectionData<SSL>();
        
        return connection;
    }
    
    /* Destroy connection */
    void destroy() {
        /* Destruct connection data */
        getConnectionData()->~HttpClientConnectionData<SSL>();
        
        /* Close socket */
        us_socket_close(SSL, (us_socket_t*)this, 0, nullptr);
    }
    
    /* Set connection ID */
    void setConnectionId(uint64_t id) {
        getConnectionData()->connectionId = id;
    }
    
    /* Get connection ID */
    uint64_t getConnectionId() const {
        return getConnectionData()->connectionId;
    }
    
    /* Set context */
    void setContext(HttpClientContext<SSL>* context) {
        getConnectionData()->context = context;
    }
    
    /* Get context */
    HttpClientContext<SSL>* getContext() const {
        return getConnectionData()->context;
    }
    
    /* Create request */
    HttpClientRequest<SSL>* createRequest() {
        auto* request = HttpClientRequest<SSL>::create(Loop::get());
        if (request) {
            request->setConnection(this);
        }
        return request;
    }
    
    /* Connect to host */
    bool connect(const char* host, int port) {
        return us_socket_connect(SSL, (us_socket_t*)this, host, port, nullptr, 0, nullptr) != nullptr;
    }
    
    /* Connect to Unix domain socket */
    bool connectUnix(const char* server_path) {
        return us_socket_connect_unix(SSL, (us_socket_t*)this, server_path, nullptr, 0, nullptr) != nullptr;
    }
    
    /* Close connection */
    void close() {
        us_socket_close(SSL, (us_socket_t*)this, 0, nullptr);
    }
    
    /* Check if connection is closed */
    bool isClosed() const {
        return us_socket_is_closed(SSL, (us_socket_t*)this);
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONNECTION_H
