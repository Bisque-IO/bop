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
#include "libusockets.h"

#include <string>
#include <string_view>

namespace uWS {

/* Forward declarations */
template <bool SSL, typename USERDATA> struct TCPContext;
template <bool SSL, typename USERDATA> struct TCPConnection;
template <bool SSL, typename USERDATA> struct TemplatedTCPApp;

/* TCP connection data for both server and client sides */
template <bool SSL, typename USERDATA = void>
struct alignas(16) TCPConnectionData : AsyncSocketData<SSL> {
    /* Connection type */
    bool isClient = false;  // Distinguishes server vs client connections

    bool isConnected = false;
    
    /* Configuration */
    uint32_t idleTimeoutSeconds = 30;
    uint32_t longTimeoutMinutes = 60;
    
    /* Backpressure state */
    uintmax_t writeOffset = 0;
    uintmax_t maxBackpressure = 1024 * 1024;  // 1MB default
    bool resetIdleTimeoutOnSend = true;
    bool closeOnBackpressureLimit = false;
    
    /* Connection info */
    std::string remoteAddress{};
    int remotePort = 0;
    
    /* Statistics */
    uintmax_t bytesReceived = 0;
    uintmax_t bytesSent = 0;
    uintmax_t messagesReceived = 0;
    uintmax_t messagesSent = 0;

    /* Reset statistics */
    void resetStatistics() {
        bytesReceived = 0;
        bytesSent = 0;
        messagesReceived = 0;
        messagesSent = 0;
    }
    
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
    
    /* Destructor to clean up USERDATA */
    ~TCPConnectionData() {
        if constexpr (std::is_destructible_v<USERDATA> && !std::is_same_v<USERDATA, void>) {
            /* Clean up USERDATA if it is destructible and not void */
            // USERDATA* userData = reinterpret_cast<USERDATA*>(this + 1);
            // new (reinterpret_cast<USERDATA*>(this + 1)) USERDATA();
            // userData->~USERDATA();
        }
    }
};

/* Send status enum */
enum class SendStatus : int32_t {
    SUCCESS,        // Data sent successfully
    BACKPRESSURE,   // Data partially sent, rest buffered due to backpressure
    DROPPED,        // Data dropped due to backpressure limits
    FAILED          // Send failed (connection closed, etc.)
};

/* Unified TCP connection struct for both server and client sides */
template <bool SSL, typename USERDATA = void>
struct TCPConnection : public AsyncSocket<SSL> {
    template <bool, typename> friend struct TCPContext;
    template <bool, typename> friend struct TemplatedTCPApp;
    typedef AsyncSocket<SSL> Super;
    
public:
    /* Get the connection data */
    TCPConnectionData<SSL, USERDATA>* getConnectionData() {
        return reinterpret_cast<TCPConnectionData<SSL, USERDATA> *>(us_socket_ext(SSL, reinterpret_cast<struct us_socket_t *>(this)));
    }
    
    /* Get the connection data (const version) */
    const TCPConnectionData<SSL, USERDATA>* getConnectionData() const {
        return reinterpret_cast<const TCPConnectionData<SSL, USERDATA> *>(us_socket_ext(SSL, reinterpret_cast<struct us_socket_t *>(this)));
    }
    
    static TCPConnectionData<SSL, USERDATA>* getConnectionData(struct us_socket_t* s) {
        return reinterpret_cast<TCPConnectionData<SSL, USERDATA> *>(us_socket_ext(SSL, s));
    }
    
    static USERDATA* getUserData(us_socket_t* s) {
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData(s);
        return reinterpret_cast<USERDATA *>(connData + 1);
    }
    
public:
    /* Returns pointer to the per socket user data */
    USERDATA* getUserData() {
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData();
        /* We just have it overallocated by sizeof type */
        return reinterpret_cast<USERDATA *>(connData + 1);
    }

    /* Check if connection is closed */
    bool isClosed() {
        return us_socket_is_closed(SSL, (us_socket_t*)this);
    }
    
    /* Check if connection is shut down */
    bool isShutDown() {
        return us_socket_is_shut_down(SSL, (us_socket_t*)this);
    }

    /* See AsyncSocket */
    using Super::getRemoteAddress;
    using Super::getRemoteAddressAsText;
    using Super::getNativeHandle;
    using Super::getBufferedAmount;

    /* Get current backpressure amount */
    uintmax_t getBackpressure() {
        return this->getBufferedAmount();
    }
    
    /* Check if connection has backpressure */
    bool hasBackpressure() {
        return this->getBufferedAmount() > 0;
    }
    
    /* Socket long timeout (for minute precision) */
    void longTimeout(unsigned int minutes) {
        us_socket_long_timeout(SSL, (us_socket_t*)this, minutes);
    }

    /* Send data with backpressure handling */
    SendStatus send(std::string_view data) {
        TCPConnectionData<SSL, USERDATA>* connData = getConnectionData();
        
        if (this->isClosed() || this->isShutDown()) {
            return SendStatus::FAILED;
        }
        
        /* Check if this would exceed max backpressure */
        if (connData->maxBackpressure > 0) {
            uintmax_t currentBackpressure = this->getBufferedAmount();
            if (currentBackpressure + data.size() > connData->maxBackpressure) {
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
            auto [written, failed] = reinterpret_cast<AsyncSocket<SSL>*>(newCorkedSocket)->uncork();

            /* If we are no longer the same socket then early return the new "this" */
            if (this != newCorkedSocket) {
                return reinterpret_cast<TCPConnection*>(newCorkedSocket);
            }

            if (failed) {
                /* Set timeout on uncork failure */
                this->timeout(getConnectionData()->idleTimeoutSeconds);  // Use default timeout
            }
        } else {
            /* Cannot cork, but still execute the callback */
            callback();
        }
        
        return this;
    }

    /* Immediately terminate this Http response */
    using Super::close;
    using Super::shutdown;

    /* Throttle reads and writes */
    TCPConnection<SSL, USERDATA> *pause() {
        Super::pause();
        Super::timeout(0);
        return this;
    }

    TCPConnection<SSL, USERDATA> *resume() {
        Super::resume();
        Super::timeout(getConnectionData()->idleTimeoutSeconds);
        return this;
    }
};

} // namespace uWS

#endif // UWS_TCPCONNECTION_H
