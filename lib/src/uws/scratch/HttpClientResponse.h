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

#ifndef UWS_HTTPCLIENTRESPONSE_H
#define UWS_HTTPCLIENTRESPONSE_H

#include "AsyncSocket.h"
#include "MoveOnlyFunction.h"
#include <string>
#include <string_view>

namespace uWS {

template <bool SSL>
struct HttpClientResponse {
    template <bool> friend struct HttpClientContext;
    template <bool> friend struct TemplatedClientApp;

private:
    HttpClientResponse() = delete;

    /* The underlying socket */
    us_socket_t* socket;

    /* Response state */
    enum ResponseState {
        HTTP_RESPONSE_PENDING = 1,
        HTTP_RESPONSE_COMPLETE = 2,
        HTTP_CONNECTION_CLOSE = 4
    };

    unsigned int state = 0;

    /* Abort callback */
    MoveOnlyFunction<void()> onAborted = nullptr;

public:
    /* Get the underlying socket */
    us_socket_t* getSocket() {
        return socket;
    }

    /* Check if response is pending */
    bool isPending() const {
        return state & HTTP_RESPONSE_PENDING;
    }

    /* Check if response is complete */
    bool isComplete() const {
        return state & HTTP_RESPONSE_COMPLETE;
    }

    /* Check if connection should be closed */
    bool shouldClose() const {
        return state & HTTP_CONNECTION_CLOSE;
    }

    /* Set abort callback */
    void onAbort(MoveOnlyFunction<void()>&& handler) {
        onAborted = std::move(handler);
    }

    /* Close the response */
    void close() {
        if (socket) {
            us_socket_close(SSL, socket, 0, nullptr);
            socket = nullptr;
        }
    }

    /* Get the underlying AsyncSocket for advanced operations */
    AsyncSocket<SSL>* getAsyncSocket() {
        return (AsyncSocket<SSL>*)socket;
    }
};

} // namespace uWS

#endif // UWS_HTTPCLIENTRESPONSE_H
