/*
 * HttpClientRequestData.h
 * 
 * HTTP client request data structures.
 */

#ifndef UWS_HTTPCLIENTREQUESTDATA_H
#define UWS_HTTPCLIENTREQUESTDATA_H

#include "MoveOnlyFunction.h"
#include <string>
#include <unordered_map>

namespace uWS {

/* Forward declarations */
template <bool SSL> struct HttpClientConnection;

/* HTTP client request data */
template <bool SSL>
struct HttpClientRequestData {
    /* Request identification */
    uint64_t requestId = 0;
    HttpClientConnection<SSL>* connection = nullptr;
    
    /* Request info */
    std::string method;
    std::string path;
    std::string host;
    int port = 80;
    
    /* Headers */
    std::unordered_map<std::string, std::string> headers;
    
    /* Request body state */
    std::string body;
    uintmax_t bodySize = 0;
    bool hasContentLength = false;
    bool useChunkedEncoding = false;
    bool bodyComplete = false;
    
    /* Response state */
    bool responseReceived = false;
    bool responseBodyComplete = false;
    
    /* Statistics */
    uintmax_t bytesReceived = 0;
    uintmax_t bytesSent = 0;
    
    /* Callbacks */
    MoveOnlyFunction<void(HttpClientRequestData<SSL>*, int, const std::string&)> onResponse = nullptr;
    MoveOnlyFunction<void(HttpClientRequestData<SSL>*, const std::string&)> onData = nullptr;
    MoveOnlyFunction<void(HttpClientRequestData<SSL>*)> onEnd = nullptr;
    MoveOnlyFunction<void(HttpClientRequestData<SSL>*, int)> onError = nullptr;
    
    /* Reset request data */
    void reset() {
        requestId = 0;
        connection = nullptr;
        method.clear();
        path.clear();
        host.clear();
        port = 80;
        headers.clear();
        body.clear();
        bodySize = 0;
        hasContentLength = false;
        useChunkedEncoding = false;
        bodyComplete = false;
        responseReceived = false;
        responseBodyComplete = false;
        bytesReceived = 0;
        bytesSent = 0;
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTREQUESTDATA_H
