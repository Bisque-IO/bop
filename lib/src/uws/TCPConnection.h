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

#ifndef UWS_TCPCONNECTION_H
#define UWS_TCPCONNECTION_H

#include "AsyncSocket.h"
#include "AsyncSocketData.h"
#include "MoveOnlyFunction.h"
#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPContext;
template <bool SSL, typename USERDATA> struct TCPConnection;
template <bool SSL> struct TemplatedTCPApp;

/* TCP connection data for both server and client sides */
template <bool SSL, typename USERDATA = void>
struct TCPConnectionData {
    /* Connection type */
    bool isClient = false;  // Distinguishes server vs client connections
    
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
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    
    /* Backpressure state */
    uintmax_t writeOffset = 0;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* Connection info */
    std::string remoteAddress;
    int remotePort = 0;
    
    /* Statistics */
    uintmax_t bytesReceived = 0;
    uintmax_t bytesSent = 0;
    uintmax_t messagesReceived = 0;
    uintmax_t messagesSent = 0;
    
    /* Reset connection data */
    void reset() {
        writeOffset = 0;
        remoteAddress.clear();
        remotePort = 0;
        bytesReceived = 0;
        bytesSent = 0;
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

/* Unified TCP connection struct for both server and client sides */
template <bool SSL, typename USERDATA = void>
struct TCPConnection : public AsyncSocket<SSL> {
    template <bool, typename> friend struct TCPContext;
    template <bool> friend struct TemplatedTCPApp;
    
private:
    /* Get the connection data */
    TCPConnectionData<SSL, USERDATA>* getConnectionData() {
        return (TCPConnectionData<SSL, USERDATA>*)us_socket_ext(SSL, (us_socket_t*)this);
    }
    
    static TCPConnectionData<SSL, USERDATA>* getConnectionData(us_socket_t* s) {
        return (TCPConnectionData<SSL, USERDATA>*)us_socket_ext(SSL, s);
    }
    
    static USERDATA* getUserData(us_socket_t* s) {
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
        return (USERDATA*)(connData + 1);
    }
    
public:
    /* Returns pointer to the per socket user data */
    USERDATA* getUserData() {
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData();
        /* We just have it overallocated by sizeof type */
        return (USERDATA*)(connData + 1);
    }
    
    /* Connection State and Information */
    
    /* Check if this is a client connection */
    bool isClient() const {
        return getConnectionData()->isClient;
    }
    
    /* Check if this is a server connection */
    bool isServer() const {
        return !getConnectionData()->isClient;
    }
    
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
        TCPConnectionData<SSL, USERDATA>* data = getConnectionData();
        data->idleTimeoutSeconds = idleTimeoutSeconds;
        data->longTimeoutMinutes = longTimeoutMinutes;
        us_socket_timeout(SSL, (us_socket_t*)this, idleTimeoutSeconds);
    }
    
    /* Get timeout configuration */
    std::pair<uint32_t, uint32_t> getTimeout() const {
        const TCPConnectionData<SSL, USERDATA>* data = getConnectionData();
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
    void onConnection(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getConnectionData()->onConnection = std::move(handler);
    }
    
    void onDisconnected(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getConnectionData()->onDisconnected = std::move(handler);
    }
    
    void onTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getConnectionData()->onTimeout = std::move(handler);
    }
    
    void onLongTimeout(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getConnectionData()->onLongTimeout = std::move(handler);
    }
    
    void onConnectError(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, int, std::string_view)>&& handler) {
        getConnectionData()->onConnectError = std::move(handler);
    }
    
    void onData(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getConnectionData()->onData = std::move(handler);
    }
    
    void onWritable(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
        getConnectionData()->onWritable = std::move(handler);
    }
    
    void onDrain(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*)>&& handler) {
        getConnectionData()->onDrain = std::move(handler);
    }
    
    void onDropped(MoveOnlyFunction<void(TCPConnection<SSL, USERDATA>*, std::string_view)>&& handler) {
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
        TCPConnectionData<SSL, USERDATA>* data = getConnectionData();
        data->bytesReceived = 0;
        data->bytesSent = 0;
        data->messagesReceived = 0;
        data->messagesSent = 0;
    }
    
    /* Send data with backpressure handling */
    SendStatus send(std::string_view data) {
        if (this->isClosed() || this->isShutDown()) {
            return SendStatus::FAILED;
        }
        
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData();
        
        /* Check if this would exceed max backpressure */
        if (connData->maxBackpressure > 0) {
            uintmax_t currentBackpressure = this->getBufferedAmount();
            if (currentBackpressure + data.size() > connData->maxBackpressure) {
                /* Call dropped handler if available */
                if (connData->onDropped) {
                    connData->onDropped(connData, data);
                }
                
                /* Close connection if configured to do so */
                if (connData->closeOnBackpressureLimit) {
                    this->close();
                }
                return SendStatus::DROPPED;
            }
        }
        
        /* Use AsyncSocket for backpressure handling */
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
    
    /* Get current backpressure amount */
    uintmax_t getBackpressure() const {
        return this->getBufferedAmount();
    }
    
    /* Check if connection has backpressure */
    bool hasBackpressure() const {
        return this->getBufferedAmount() > 0;
    }
    
    /* Send data using corking for multiple sends */
    TCPConnection* cork(MoveOnlyFunction<void()>&& callback) {
        if (!this->isCorked() && this->canCork()) {
            /* We can cork, so do it properly */
            LoopData* loopData = this->getLoopData();
            this->cork();
            callback();

            /* The only way we could possibly have changed the corked socket during handler call, would be if 
             * the socket was upgraded and caused a realloc. Because of this we cannot use "this"
             * from here downwards. */
            auto* newCorkedSocket = loopData->corkedSocket;

            /* If nobody is corked, it means most probably that large amounts of data has
             * been written and the cork buffer has already been sent off and uncorked.
             * We are done here, if that is the case. */
            if (!newCorkedSocket) {
                return this;
            }

            /* Timeout on uncork failure, since most writes will succeed while corked */
            auto [written, failed] = static_cast<AsyncSocket<SSL>*>(newCorkedSocket)->uncork();

            /* If we are no longer the same socket then early return the new "this" */
            if (this != newCorkedSocket) {
                return static_cast<TCPConnection*>(newCorkedSocket);
            }

            if (failed) {
                /* Set timeout on uncork failure */
                this->timeout(30);  // Use default timeout
            }
        } else {
            /* Cannot cork, but still execute the callback */
            callback();
        }
        
        return this;
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
    
    /* Check if connection has timed out */
    bool hasTimedOut() const {
        return us_socket_is_shut_down(SSL, (us_socket_t*)this);
    }
};

} // namespace uWS

#endif // UWS_TCPCONNECTION_H
