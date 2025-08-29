/*
 * HttpClientContext.h
 * 
 * HTTP client context for managing HTTP client connections and requests.
 */

#ifndef UWS_HTTPCLIENTCONTEXT_H
#define UWS_HTTPCLIENTCONTEXT_H

#include "HttpClientConnection.h"
#include "HttpClientRequest.h"
#include "MoveOnlyFunction.h"
#include "Loop.h"
#include <unordered_map>

namespace uWS {

/* HTTP client context behavior */
template <bool SSL>
struct HttpClientBehavior {
    /* Connection callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL>*)> onConnection = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL>*, int)> onError = nullptr;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
};

/* HTTP client context data */
template <bool SSL>
struct HttpClientContextData {
    /* Connection management */
    std::unordered_map<uint64_t, HttpClientConnection<SSL>*> connections;
    uint64_t nextConnectionId = 1;
    
    /* Statistics */
    uintmax_t connectionsCreated = 0;
    uintmax_t connectionsClosed = 0;
    uintmax_t requestsSent = 0;
    uintmax_t responsesReceived = 0;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* Callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL>*)> onConnection = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL>*, int)> onError = nullptr;
    
    /* Reset context data */
    void reset() {
        connections.clear();
        nextConnectionId = 1;
        connectionsCreated = 0;
        connectionsClosed = 0;
        requestsSent = 0;
        responsesReceived = 0;
    }
    
    /* Add connection */
    uint64_t addConnection(HttpClientConnection<SSL>* connection) {
        uint64_t id = nextConnectionId++;
        connections[id] = connection;
        connectionsCreated++;
        return id;
    }
    
    /* Remove connection */
    void removeConnection(uint64_t id) {
        auto it = connections.find(id);
        if (it != connections.end()) {
            connections.erase(it);
            connectionsClosed++;
        }
    }
    
    /* Get connection */
    HttpClientConnection<SSL>* getConnection(uint64_t id) {
        auto it = connections.find(id);
        return it != connections.end() ? it->second : nullptr;
    }
};

/* HTTP client context */
template <bool SSL>
struct HttpClientContext {
    HttpClientContextData<SSL>* contextData;
    
    /* Create context */
    static HttpClientContext<SSL>* create(Loop* loop, const HttpClientBehavior<SSL>& behavior = {}) {
        auto* context = new HttpClientContext<SSL>();
        context->contextData = new HttpClientContextData<SSL>();
        
        /* Copy behavior to context data */
        context->contextData->onConnection = std::move(behavior.onConnection);
        context->contextData->onError = std::move(behavior.onError);
        context->contextData->idleTimeoutSeconds = behavior.idleTimeoutSeconds;
        context->contextData->longTimeoutMinutes = behavior.longTimeoutMinutes;
        context->contextData->maxBackpressure = behavior.maxBackpressure;
        context->contextData->resetIdleTimeoutOnSend = behavior.resetIdleTimeoutOnSend;
        context->contextData->closeOnBackpressureLimit = behavior.closeOnBackpressureLimit;
        
        return context;
    }
    
    /* Destroy context */
    void destroy() {
        delete contextData;
        delete this;
    }
    
    /* Get context data */
    HttpClientContextData<SSL>* getContextData() const {
        return contextData;
    }
    
    /* Create connection */
    HttpClientConnection<SSL>* createConnection() {
        auto* connection = HttpClientConnection<SSL>::create(Loop::get());
        if (connection) {
            uint64_t id = contextData->addConnection(connection);
            connection->setConnectionId(id);
            connection->setContext(this);
        }
        return connection;
    }
    
    /* Remove connection */
    void removeConnection(uint64_t id) {
        contextData->removeConnection(id);
    }
    
    /* Get connection */
    HttpClientConnection<SSL>* getConnection(uint64_t id) {
        return contextData->getConnection(id);
    }
    
    /* Reset context */
    void reset() {
        contextData->reset();
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONTEXT_H
