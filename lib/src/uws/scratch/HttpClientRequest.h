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

#ifndef UWS_HTTPCLIENTREQUEST_H
#define UWS_HTTPCLIENTREQUEST_H

#include "AsyncSocket.h"
#include "HttpClientRequestData.h"
#include "MoveOnlyFunction.h"
#include "Loop.h"
#include <string>

namespace uWS {

/* Forward declarations */
template <bool SSL> struct HttpClientConnection;

/* HTTP client request */
template <bool SSL>
struct HttpClientRequest : public AsyncSocket<SSL> {
    template <bool> friend struct HttpClientConnection;
    
private:
    /* Get the request data */
    HttpClientRequestData<SSL>* getRequestData() {
        return (HttpClientRequestData<SSL>*)us_socket_ext(SSL, (us_socket_t*)this);
    }
    
    static HttpClientRequestData<SSL>* getRequestData(us_socket_t* s) {
        return (HttpClientRequestData<SSL>*)us_socket_ext(SSL, s);
    }
    
public:
    /* Create request */
    static HttpClientRequest<SSL>* create(Loop* loop) {
        HttpClientRequest* request = (HttpClientRequest*)us_create_socket(
            SSL, (us_loop_t*)loop, sizeof(HttpClientRequestData<SSL>)
        );
        
        if (!request) {
            return nullptr;
        }
        
        /* Initialize request data */
        new (getRequestData((us_socket_t*)request)) HttpClientRequestData<SSL>();
        
        return request;
    }
    
    /* Destroy request */
    void destroy() {
        /* Destruct request data */
        getRequestData()->~HttpClientRequestData<SSL>();
        
        /* Close socket */
        us_socket_close(SSL, (us_socket_t*)this, 0, nullptr);
    }
    
    /* Set connection */
    void setConnection(HttpClientConnection<SSL>* connection) {
        getRequestData()->connection = connection;
    }
    
    /* Get connection */
    HttpClientConnection<SSL>* getConnection() const {
        return getRequestData()->connection;
    }
    
    /* Request Configuration */
    
    /* Set request method and path */
    void setRequest(const std::string& method, const std::string& path) {
        HttpClientRequestData<SSL>* data = getRequestData();
        data->method = method;
        data->path = path;
    }
    
    /* Set request host and port */
    void setHost(const std::string& host, int port = 80) {
        HttpClientRequestData<SSL>* data = getRequestData();
        data->host = host;
        data->port = port;
    }
    
    /* Add header */
    void addHeader(const std::string& name, const std::string& value) {
        getRequestData()->headers[name] = value;
    }
    
    /* Set content length for body */
    void setContentLength(uintmax_t length) {
        HttpClientRequestData<SSL>* data = getRequestData();
        data->hasContentLength = true;
        data->bodySize = length;
        data->useChunkedEncoding = false;
        addHeader("Content-Length", std::to_string(length));
    }
    
    /* Enable chunked encoding */
    void enableChunkedEncoding() {
        HttpClientRequestData<SSL>* data = getRequestData();
        data->useChunkedEncoding = true;
        data->hasContentLength = false;
        addHeader("Transfer-Encoding", "chunked");
    }
    
    /* Set request body */
    void setBody(const std::string& body) {
        HttpClientRequestData<SSL>* data = getRequestData();
        data->body = body;
        data->bodySize = body.size();
        data->bodyComplete = true;
        
        if (!data->hasContentLength && !data->useChunkedEncoding) {
            setContentLength(body.size());
        }
    }
    
    /* Request Event Handlers */
    
    /* Set response handler */
    void onResponse(MoveOnlyFunction<void(HttpClientRequestData<SSL>*, int, const std::string&)>&& handler) {
        getRequestData()->onResponse = std::move(handler);
    }
    
    /* Set data handler */
    void onData(MoveOnlyFunction<void(HttpClientRequestData<SSL>*, const std::string&)>&& handler) {
        getRequestData()->onData = std::move(handler);
    }
    
    /* Set end handler */
    void onEnd(MoveOnlyFunction<void(HttpClientRequestData<SSL>*)>&& handler) {
        getRequestData()->onEnd = std::move(handler);
    }
    
    /* Set error handler */
    void onError(MoveOnlyFunction<void(HttpClientRequestData<SSL>*, int)>&& handler) {
        getRequestData()->onError = std::move(handler);
    }
    
    /* Request Information */
    
    /* Get request method */
    const std::string& getMethod() const {
        return getRequestData()->method;
    }
    
    /* Get request path */
    const std::string& getPath() const {
        return getRequestData()->path;
    }
    
    /* Get request host */
    const std::string& getHost() const {
        return getRequestData()->host;
    }
    
    /* Get request port */
    int getPort() const {
        return getRequestData()->port;
    }
    
    /* Get header value */
    std::string getHeader(const std::string& name) const {
        const auto& headers = getRequestData()->headers;
        auto it = headers.find(name);
        return it != headers.end() ? it->second : "";
    }
    
    /* Check if response received */
    bool hasResponse() const {
        return getRequestData()->responseReceived;
    }
    
    /* Check if response body is complete */
    bool isResponseComplete() const {
        return getRequestData()->responseBodyComplete;
    }
    
    /* Close the request */
    void close() {
        us_socket_close(SSL, (us_socket_t*)this, 0, nullptr);
    }
    
    /* Check if request is closed */
    bool isClosed() const {
        return us_socket_is_closed(SSL, (us_socket_t*)this);
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTREQUEST_H
