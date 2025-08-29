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

#ifndef UWS_HTTPCLIENTCONNECTION_H
#define UWS_HTTPCLIENTCONNECTION_H

#include "AsyncSocket.h"
#include "AsyncSocketData.h"
#include "MoveOnlyFunction.h"
#include "HttpParser.h"
#include "ChunkedEncoding.h"
#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL> struct HttpClientContext;
template <bool SSL, typename USERDATA> struct HttpClientConnectionData;
template <bool SSL, typename USERDATA> struct HttpClientConnection;

/* HTTP client connection data */
template <bool SSL, typename USERDATA = void>
struct HttpClientConnectionData {
    /* Connection type */
    bool isClient = true;  // Always true for HTTP client connections
    
    /* Connection callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onConnected = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onConnectError = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onDisconnected = nullptr;
    
    /* HTTP response callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view, HttpResponseHeaders&)> onHeaders = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view, bool)> onChunk = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)> onError = nullptr;
    
    /* Timeout callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onTimeout = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onLongTimeout = nullptr;
    
    /* Data flow callbacks */
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)> onWritable = nullptr;
    MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)> onDropped = nullptr;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    
    /* Backpressure state */
    uintmax_t writeOffset = 0;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* HTTP parsing state */
    HttpResponseHeaders currentResponse;  // Current response being parsed
    HttpParser parser;  // HTTP parser instance with its own state
     
    /* Request state */
    int64_t requestBodyLength = -1;  // -1: no body, 0: chunked, >0: content-length
    uint32_t pendingRequests = 0;  // Counter for pipelined requests
    std::string headers;  // Reusable buffer for building HTTP request headers

    /* Connection info */
    std::string remoteAddress;
    int remotePort = 0;
    
    /* Statistics */
    uintmax_t bytesReceived = 0;
    uintmax_t bytesSent = 0;
    uintmax_t requestsSent = 0;
    uintmax_t responsesReceived = 0;
    uintmax_t messagesReceived = 0;
    uintmax_t messagesSent = 0;
    
    /* Reset connection data */
    void reset() {
        currentResponse = HttpResponseHeaders();
        parser.resetParserState();
        requestBodyLength = -1;  // Reset to no body
        pendingRequests = 0;  // Reset pending requests counter
        headers.clear();  // Clear reusable headers buffer
        writeOffset = 0;
        remoteAddress.clear();
        remotePort = 0;
        bytesReceived = 0;
        bytesSent = 0;
        requestsSent = 0;
        responsesReceived = 0;
        messagesReceived = 0;
        messagesSent = 0;
    }
};

/* Send status enum */
enum class SendStatus {
    SUCCESS,        // Data sent successfully
    BACKPRESSURE,   // Data partially sent, rest buffered due to backpressure
    DROPPED,        // Data dropped due to backpressure limits
    FAILED          // Send failed (connection closed, etc.)
};

/* HTTP client connection struct */
template <bool SSL, typename USERDATA = void>
struct HttpClientConnection : public AsyncSocket<SSL> {
    template <bool> friend struct HttpClientContext;
    
private:
    /* Get the connection data */
    HttpClientConnectionData<SSL, USERDATA>* getConnectionData() {
        return (HttpClientConnectionData<SSL, USERDATA>*)us_socket_ext(SSL, (us_socket_t*)this);
    }
    
    static HttpClientConnectionData<SSL, USERDATA>* getConnectionData(us_socket_t* s) {
        return (HttpClientConnectionData<SSL, USERDATA>*)us_socket_ext(SSL, s);
    }
    
    static USERDATA* getUserData(us_socket_t* s) {
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
        return (USERDATA*)(connData + 1);
    }
    
              /* Parse incoming HTTP response data */
     void parseResponseData(char* data, int length) {
         HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
         connData->bytesReceived += length;
         connData->messagesReceived++;
         
         /* Parse HTTP response using OS-supplied buffer directly */
         /* The uSockets library provides buffers with proper padding */
         auto [consumed, user] = connData->parser.consumeResponsePostPadded(
             data, 
             (unsigned int)length, 
             connData, 
             nullptr, 
             &connData->currentResponse,
            [](void* user, HttpResponseHeaders* resp) -> void* {
                HttpClientConnectionData<SSL, USERDATA>* connData = static_cast<HttpClientConnectionData<SSL, USERDATA>*>(user);
                
                /* Call onHeaders callback */
                if (connData->onHeaders) {
                    connData->onHeaders(static_cast<HttpClientConnection<SSL, USERDATA>*>(this), resp->getStatusCode(), resp->getStatusMessage(), *resp);
                }
                
                connData->responsesReceived++;
                return user;
            },
            [](void* user, std::string_view chunk, bool isLast) -> void* {
                  HttpClientConnectionData<SSL, USERDATA>* connData = static_cast<HttpClientConnectionData<SSL, USERDATA>*>(user);
                  
                  /* Call onChunk callback */
                  if (connData->onChunk) {
                      connData->onChunk(static_cast<HttpClientConnection<SSL, USERDATA>*>(this), chunk, isLast);
                  }
                  
                  /* Decrement pending requests counter when response is complete */
                  if (isLast && connData->pendingRequests > 0) {
                      connData->pendingRequests--;
                  }
                  
                  return user;
              }
        );
         
        /* Handle parsing errors */
        if (user == FULLPTR) {
            int errorCode = consumed;
            if (connData->onError) {
                connData->onError(static_cast<HttpClientConnection<SSL, USERDATA>*>(this), errorCode, "HTTP parsing error");
            }
        }
     }
    
public:
    /* Returns pointer to the per socket user data */
    USERDATA* getUserData() {
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
        return (USERDATA*)(connData + 1);
    }
    
    /* Connection State and Information */
    
    /* Get remote address */
    std::string_view getRemoteAddress() const {
        return getConnectionData()->remoteAddress;
    }
    
    /* Get remote port */
    int getRemotePort() const {
        return getConnectionData()->remotePort;
    }
    
    /* Connection Configuration */
    
    /* Set timeout configuration */
    void setTimeout(uint32_t idleTimeoutSeconds, uint32_t longTimeoutMinutes) {
        HttpClientConnectionData<SSL, USERDATA>* data = getConnectionData();
        data->idleTimeoutSeconds = idleTimeoutSeconds;
        data->longTimeoutMinutes = longTimeoutMinutes;
        us_socket_timeout(SSL, (us_socket_t*)this, idleTimeoutSeconds);
    }
    
    /* Get timeout configuration */
    std::pair<uint32_t, uint32_t> getTimeout() const {
        const HttpClientConnectionData<SSL, USERDATA>* data = getConnectionData();
        return {data->idleTimeoutSeconds, data->longTimeoutMinutes};
    }
    
    /* Set max backpressure */
    void setMaxBackpressure(uintmax_t maxBackpressure) {
        getConnectionData()->maxBackpressure = maxBackpressure;
    }
    
    /* Get max backpressure */
    uintmax_t getMaxBackpressure() const {
        return getConnectionData()->maxBackpressure;
    }
    
    /* Set reset idle timeout on send */
    void setResetIdleTimeoutOnSend(bool reset) {
        getConnectionData()->resetIdleTimeoutOnSend = reset;
    }
    
    /* Get reset idle timeout on send */
    bool getResetIdleTimeoutOnSend() const {
        return getConnectionData()->resetIdleTimeoutOnSend;
    }
    
    /* Set close on backpressure limit */
    void setCloseOnBackpressureLimit(bool close) {
        getConnectionData()->closeOnBackpressureLimit = close;
    }
    
    /* Get close on backpressure limit */
    bool getCloseOnBackpressureLimit() const {
        return getConnectionData()->closeOnBackpressureLimit;
    }
    
    /* Connection Event Handlers */
    
    /* Set connection event handlers */
    void onConnected(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getConnectionData()->onConnected = std::move(handler);
    }
    
    void onConnectError(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getConnectionData()->onConnectError = std::move(handler);
    }
    
    void onDisconnected(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getConnectionData()->onDisconnected = std::move(handler);
    }
    
    /* HTTP Response Event Handlers */
    
    void onHeaders(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view, HttpResponseHeaders)>&& handler) {
        getConnectionData()->onHeaders = std::move(handler);
    }
    
    void onChunk(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view, bool)>&& handler) {
        getConnectionData()->onChunk = std::move(handler);
    }
    
         void onError(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
         getConnectionData()->onError = std::move(handler);
     }
     
     /* Timeout Event Handlers */
     
     void onTimeout(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
         getConnectionData()->onTimeout = std::move(handler);
     }
     
     void onLongTimeout(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
         getConnectionData()->onLongTimeout = std::move(handler);
     }
     
     /* Data Flow Event Handlers */
     
         void onWritable(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*)>&& handler) {
        getConnectionData()->onWritable = std::move(handler);
    }
     
     void onDropped(MoveOnlyFunction<void(HttpClientConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
         getConnectionData()->onDropped = std::move(handler);
     }
    
    /* Connection Statistics */
    
    /* Get bytes received */
    uintmax_t getBytesReceived() const {
        return getConnectionData()->bytesReceived;
    }
    
    /* Get bytes sent */
    uintmax_t getBytesSent() const {
        return getConnectionData()->bytesSent;
    }
    
    /* Get requests sent */
    uintmax_t getRequestsSent() const {
        return getConnectionData()->requestsSent;
    }
    
    /* Get responses received */
    uintmax_t getResponsesReceived() const {
        return getConnectionData()->responsesReceived;
    }
    
    /* Get messages received */
    uintmax_t getMessagesReceived() const {
        return getConnectionData()->messagesReceived;
    }
    
    /* Get messages sent */
    uintmax_t getMessagesSent() const {
        return getConnectionData()->messagesSent;
    }
    
    /* Reset statistics */
    void resetStatistics() {
        HttpClientConnectionData<SSL, USERDATA>* data = getConnectionData();
        data->bytesReceived = 0;
        data->bytesSent = 0;
        data->requestsSent = 0;
        data->responsesReceived = 0;
        data->messagesReceived = 0;
        data->messagesSent = 0;
    }

    /* Send HTTP headers with body length specification */
    SendStatus sendHeaders(std::string_view headers, int64_t bodyLength = -1) {
        if (this->isClosed() || this->isShutDown()) {
            return SendStatus::FAILED;
        }
        
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
        
        /* Check if this would exceed max backpressure */
        if (connData->maxBackpressure > 0) {
            uintmax_t currentBackpressure = this->getBufferedAmount();
            if (currentBackpressure + headers.size() > connData->maxBackpressure) {
                /* Call dropped handler if available */
                if (connData->onDropped) {
                    connData->onDropped(static_cast<HttpClientConnection<SSL, USERDATA>*>(this), headers);
                }
                
                /* Close connection if configured to do so */
                if (connData->closeOnBackpressureLimit) {
                    this->close();
                }
                return SendStatus::DROPPED;
            }
        }
        
        /* Store the body length for future sends */
        connData->requestBodyLength = bodyLength;
        
        /* Send pre-formatted headers */
        auto [written, needsWritable] = this->write(headers.data(), headers.length(), false, 0);
        
        /* Update statistics */
        connData->bytesSent += written;
        connData->requestsSent++;
        connData->messagesSent++;
        connData->writeOffset += written;
        
        /* Reset idle timeout if configured */
        if (connData->resetIdleTimeoutOnSend) {
            us_socket_timeout(SSL, (us_socket_t*)this, connData->idleTimeoutSeconds);
        }
        
        if (written < (int)headers.length()) {
            return SendStatus::BACKPRESSURE;
        }
        
        /* Increment pending requests counter for pipelining */
        connData->pendingRequests++;
        
        return SendStatus::SUCCESS;
    }

    /* Send data with backpressure handling and optional chunked encoding */
    SendStatus send(std::string_view data, bool useChunkedEncoding = false) {
        if (this->isClosed() || this->isShutDown()) {
            return SendStatus::FAILED;
        }
        
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
        
        /* Check if this would exceed max backpressure */
        if (connData->maxBackpressure > 0) {
            uintmax_t currentBackpressure = this->getBufferedAmount();
            uintmax_t dataSize = data.size();
            
            /* For chunked encoding, account for the framing overhead */
            if (useChunkedEncoding && !data.empty()) {
                /* Estimate chunked encoding overhead: hex size + \r\n + \r\n */
                dataSize += 16 + 4;  // Max hex size + \r\n delimiters
            }
            
            if (currentBackpressure + dataSize > connData->maxBackpressure) {
                /* Call dropped handler if available */
                if (connData->onDropped) {
                    connData->onDropped(static_cast<HttpClientConnection<SSL, USERDATA>*>(this), data);
                }
                
                /* Close connection if configured to do so */
                if (connData->closeOnBackpressureLimit) {
                    this->close();
                }
                return SendStatus::DROPPED;
            }
        }
        
        if (useChunkedEncoding) {
            /* Use corking for chunked encoding to avoid intermediate allocations */
            return this->cork([&]() -> SendStatus {
                if (data.empty()) {
                    /* Send final chunk (empty chunk) */
                    const char* finalChunk = "0\r\n\r\n";
                    auto [written, needsWritable] = this->write(finalChunk, 5, false, 0);
                    
                                         /* Update statistics */
                     connData->bytesSent += written;
                     connData->messagesSent++;
                     connData->writeOffset += written;
                    
                    if (written == 5) {
                        return SendStatus::SUCCESS;
                    } else {
                        return SendStatus::BACKPRESSURE;
                    }
                } else {
                    /* Send chunked data */
                    char sizeHex[16];
                    snprintf(sizeHex, sizeof(sizeHex), "%zx\r\n", data.size());
                    
                    /* Write chunk size */
                    auto [sizeWritten, sizeNeedsWritable] = this->write(sizeHex, strlen(sizeHex), false, 0);
                    
                    /* Write chunk data */
                    auto [dataWritten, dataNeedsWritable] = this->write(data.data(), data.size(), false, 0);
                    
                    /* Write chunk terminator */
                    const char* terminator = "\r\n";
                    auto [termWritten, termNeedsWritable] = this->write(terminator, 2, false, 0);
                    
                                         /* Update statistics */
                     int totalWritten = sizeWritten + dataWritten + termWritten;
                     connData->bytesSent += totalWritten;
                     connData->messagesSent++;
                     connData->writeOffset += totalWritten;
                    
                    if (totalWritten == (int)(strlen(sizeHex) + data.size() + 2)) {
                        return SendStatus::SUCCESS;
                    } else {
                        return SendStatus::BACKPRESSURE;
                    }
                }
            });
        } else {
            /* Use AsyncSocket for backpressure handling (content-length mode) */
            auto [written, needsWritable] = this->write(data.data(), data.size(), false, 0);
            
                         /* Update statistics for what was actually written */
             connData->bytesSent += written;
             connData->messagesSent++;
             connData->writeOffset += written;
            
            /* Reset idle timeout if configured */
            if (connData->resetIdleTimeoutOnSend) {
                us_socket_timeout(SSL, (us_socket_t*)this, connData->idleTimeoutSeconds);
            }
            
            if (written == (int)data.size()) {
                return SendStatus::SUCCESS;
            } else {
                return SendStatus::BACKPRESSURE;
            }
        }
    }

    /* Get current request body length */
    int64_t getRequestBodyLength() const {
        return getConnectionData()->requestBodyLength;
    }
    
    /* Get pending requests count for pipelining */
    uint32_t getPendingRequests() const {
        return getConnectionData()->pendingRequests;
    }
    
    /* Convenience methods that use the stored body length */
    SendStatus sendChunk(std::string_view data) {
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
        bool useChunked = (connData->requestBodyLength == 0);
        return send(data, useChunked);
    }
    
    SendStatus sendContentLengthChunk(std::string_view data) {
        return send(data, false);  // Always use content-length mode
    }
     
    SendStatus sendFinalChunk() {
        HttpClientConnectionData<SSL, USERDATA>* connData = getConnectionData();
        if (connData->requestBodyLength == 0) {
            return send(std::string_view(), true);  // Send empty chunk for chunked encoding
        }
        return SendStatus::SUCCESS;  // No final chunk needed for content-length
    }
    
    /* Get current backpressure amount */
    uintmax_t getBackpressure() const {
        return this->getBufferedAmount();
    }
    
    /* Check if connection has backpressure */
    bool hasBackpressure() const {
        return this->getBufferedAmount() > 0;
    }
    
    /* Close the connection */
    void close() {
        us_socket_close(SSL, (us_socket_t*)this, 0, nullptr);
    }
    
    /* Shutdown the connection */
    void shutdown() {
        us_socket_shutdown(SSL, (us_socket_t*)this);
    }
    
    /* Check if connection is closed */
    bool isClosed() const {
        return us_socket_is_closed(SSL, (us_socket_t*)this);
    }
    
    /* Check if connection is shut down */
    bool isShutDown() const {
        return us_socket_is_shut_down(SSL, (us_socket_t*)this);
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTCONNECTION_H
