/*
 * HttpClientConnectionData.h
 * 
 * HTTP client connection data structures.
 */

#ifndef UWS_HTTPCLIENTCONNECTIONDATA_H
#define UWS_HTTPCLIENTCONNECTIONDATA_H

#include "MoveOnlyFunction.h"
#include <string>

namespace uWS {

/* Forward declarations */
template <bool SSL> struct HttpClientContext;
template <bool SSL> struct HttpClientRequest;

/* HTTP client connection data */
template <bool SSL>
struct HttpClientConnectionData {
    /* Connection identification */
    uint64_t connectionId = 0;
    HttpClientContext<SSL>* context = nullptr;
    
    /* Connection state */
    bool connected = false;
    bool connecting = false;
    
    /* Statistics */
    uintmax_t bytesReceived = 0;
    uintmax_t bytesSent = 0;
    uintmax_t requestsSent = 0;
    uintmax_t responsesReceived = 0;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    uintmax_t maxBackpressure = 1024 * 1024;
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* Callbacks */
    MoveOnlyFunction<void(HttpClientConnectionData<SSL>*)> onConnect = nullptr;
    MoveOnlyFunction<void(HttpClientConnectionData<SSL>*, int)> onError = nullptr;
    MoveOnlyFunction<void(HttpClientConnectionData<SSL>*)> onDisconnect = nullptr;
    
    /* Reset connection data */
    void reset() {
        connectionId = 0;
        context = nullptr;
        connected = false;
        connecting = false;
        bytesReceived = 0;
        bytesSent = 0;
        requestsSent = 0;
        responsesReceived = 0;
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONNECTIONDATA_H
