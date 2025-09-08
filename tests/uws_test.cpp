#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include <doctest/doctest.h>
// #define CHECK(x)
// #define CHECK_NE(x, y)
// #define CHECK_EQ(x, y)
#include <assert.h>

// Debug output control
#ifndef TCP_TEST_DEBUG
#define TCP_TEST_DEBUG 0
#endif

// static std::mutex coutMutex;

#if TCP_TEST_DEBUG
#define DEBUG_OUT(x)                                                                               \
    do {                                                                                           \
        MESSAGE("[DEBUG] :: " << x);                                                               \
    } while (0)
#else
#define DEBUG_OUT(x)                                                                               \
    do {                                                                                           \
    } while (0)
#endif

#include "Loop.h"
#include "TCPClientApp.h"
#include "TCPConnection.h"
#include "TCPContext.h"
#include "TCPServerApp.h"
#include "libusockets.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <future>
#include <iostream>
#include <list>
#include <memory>
#include <mutex>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

#ifndef _WIN32
#include <arpa/inet.h>
#else
// #include <winsock2.h>
#include <ws2tcpip.h>
#endif

using namespace uWS;

/* Utility function to convert dotted decimal IP to uint32_t in host byte order */
inline uint32_t ip_to_uint32(const char* ip_str) {
    struct in_addr addr;
    if (inet_pton(AF_INET, ip_str, &addr) == 1) {
        return ntohl(addr.s_addr);
    }
    return 0; // Invalid IP
}

inline uint32_t ip_to_uint32(std::string ip_str) {
    return ip_to_uint32(ip_str.c_str());
}

inline int64_t unixNanos() {
    return std::chrono::system_clock::now().time_since_epoch().count();
}

enum class EventType {
    TCPOnConnecting,
    TCPOnConnection,
    TCPOnData,
    TCPOnDisconnected,
    TCPOnConnectError,
    TCPOnWritable,
    TCPOnDrain,
    TCPOnDropped,
    TCPOnEnd,
    TCPOnTimeout,
    TCPOnLongTimeout,
    TCPOnServerName,
    TCPOnTestTCPConnectionDestroyed,

    HTTPServerOnConnection,
    HTTPServerOnData,
    HTTPServerOnDisconnected,
    HTTPServerOnConnectError,
    HTTPServerOnWritable,
    HTTPServerOnDrain,
    HTTPServerOnDropped,
    HTTPServerOnEnd,
    HTTPServerOnTimeout,
    HTTPServerOnLongTimeout,
    HTTPServerOnServerName,
    HTTPServerOnTestHTTPServerConnectionDestroyed,

    HTTPClientOnConnection,
    HTTPClientOnData,
    HTTPClientOnDisconnected,
    HTTPClientOnConnectError,
    HTTPClientOnWritable,
    HTTPClientOnDrain,
    HTTPClientOnDropped,
    HTTPClientOnEnd,
    HTTPClientOnTimeout,
    HTTPClientOnLongTimeout,
    HTTPClientOnServerName,
    HTTPClientOnTestHTTPClientConnectionDestroyed,
};

// Pretty string conversion for NetworkEventType
inline std::string_view as_string(EventType type) {
    switch (type) {
    case EventType::TCPOnConnecting: return "TCP::Connecting";
    case EventType::TCPOnConnection: return "TCP::Connection";
    case EventType::TCPOnData: return "TCP::Data";
    case EventType::TCPOnDisconnected: return "TCP::Disconnected";
    case EventType::TCPOnConnectError: return "TCP::ConnectError";
    case EventType::TCPOnWritable: return "TCP::Writable";
    case EventType::TCPOnDrain: return "TCP::Drain";
    case EventType::TCPOnDropped: return "TCP::Dropped";
    case EventType::TCPOnEnd: return "TCP::End";
    case EventType::TCPOnTimeout: return "TCP::Timeout";
    case EventType::TCPOnLongTimeout: return "TCP::LongTimeout";
    case EventType::TCPOnServerName: return "TCP::ServerName";
    case EventType::TCPOnTestTCPConnectionDestroyed: return "TCP::TestTCPConnectionDestroyed";

    case EventType::HTTPServerOnConnection: return "HTTPServer::Connection";
    case EventType::HTTPServerOnData: return "HTTPServer::Data";
    case EventType::HTTPServerOnDisconnected: return "HTTPServer::Disconnected";
    case EventType::HTTPServerOnConnectError: return "HTTPServer::ConnectError";
    case EventType::HTTPServerOnWritable: return "HTTPServer::Writable";
    case EventType::HTTPServerOnDrain: return "HTTPServer::Drain";
    case EventType::HTTPServerOnDropped: return "HTTPServer::Dropped";
    case EventType::HTTPServerOnEnd: return "HTTPServer::End";
    case EventType::HTTPServerOnTimeout: return "HTTPServer::Timeout";
    case EventType::HTTPServerOnLongTimeout: return "HTTPServer::LongTimeout";
    case EventType::HTTPServerOnServerName: return "HTTPServer::ServerName";
    case EventType::HTTPServerOnTestHTTPServerConnectionDestroyed:
        return "HTTPServer::TestHTTPServerConnectionDestroyed";

    case EventType::HTTPClientOnConnection: return "HTTPClient::Connection";
    case EventType::HTTPClientOnData: return "HTTPClient::Data";
    case EventType::HTTPClientOnDisconnected: return "HTTPClient::Disconnected";
    case EventType::HTTPClientOnConnectError: return "HTTPClient::ConnectError";
    case EventType::HTTPClientOnWritable: return "HTTPClient::Writable";
    case EventType::HTTPClientOnDrain: return "HTTPClient::Drain";
    case EventType::HTTPClientOnDropped: return "HTTPClient::Dropped";
    case EventType::HTTPClientOnEnd: return "HTTPClient::End";
    case EventType::HTTPClientOnTimeout: return "HTTPClient::Timeout";
    case EventType::HTTPClientOnLongTimeout: return "HTTPClient::LongTimeout";
    case EventType::HTTPClientOnServerName: return "HTTPClient::ServerName";
    case EventType::HTTPClientOnTestHTTPClientConnectionDestroyed:
        return "HTTPClient::TestHTTPClientConnectionDestroyed";
    }
    return "Unknown";
}

// NetworkEvent represents a network event.
struct Event {
    uint64_t sequence = 0;
    int64_t timestamp{};
    uint64_t conn_id{};
    void* connection{nullptr};
    EventType type{};
    int code{};
    std::string data{};
    void* data_ptr{};
    uintmax_t bytes;
    bool isClient = false;

    Event(
        uint64_t conn_id, void* connection, EventType type, int code, std::string data,
        void* data_ptr, uintmax_t bytes
    )
        : timestamp(unixNanos()),
          conn_id(conn_id),
          connection(connection),
          type(type),
          code(code),
          data(data),
          data_ptr(data_ptr),
          bytes(bytes) {}

    Event() = default;
    Event(const Event&) = default;
    Event(Event&&) = default;
    Event& operator=(const Event&) = default;
    Event& operator=(Event&&) = default;

    std::string to_string() const {
        std::string out;
        out.reserve(128);
        out.append("Event{seq=");
        out.append(std::to_string(sequence));
        out.append(", type=");
        out.append(as_string(type));
        out.append(", timestamp=");
        out.append(std::to_string(timestamp));
        out.append(", id=");
        out.append(std::to_string(conn_id));
        out.append(", connection=");
        out.append(std::to_string(reinterpret_cast<uintptr_t>(connection)));
        // out.append(", isClient=");
        // out.append(std::to_string(isClient));
        out.append(", code=");
        out.append(std::to_string(code));
        out.append(", data=");
        // if (data.size() > 16) {
        //     out.append("[").append(std::to_string(data.size())).append(" bytes]");
        // } else {
        // }
        out.append(data);
        out.append(", data_ptr=");
        out.append(std::to_string(reinterpret_cast<uintptr_t>(data_ptr)));
        out.append(", bytes=");
        out.append(std::to_string(bytes));
        out.append("}");
        return out;
    }
};

struct EventBuffer {
  public:
    using OnPush = std::function<bool(Event&)>;

    explicit EventBuffer(size_t capacity) {}

    // Move-only support: delete copy constructor and copy assignment
    EventBuffer(const EventBuffer&) = delete;
    EventBuffer& operator=(const EventBuffer&) = delete;

    // Push an item to the buffer. Automatically grows if full.
    bool push(Event& item) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        if (closed_) {
            return 0;
        }
        uint64_t sequence = buffer_.size();
        buffer_.emplace_back(std::move(item));
        auto& event = buffer_[sequence];
        DEBUG_OUT("EventBuffer::push => " << event.to_string());

        std::vector<uint64_t> handlers;
        for (auto& handler : handlers_) {
            if (!handler.second(event)) {
                handlers.push_back(handler.first);
                // it = handlers_.erase(it);
                continue;
            }
        }
        for (auto id : handlers) {
            handlers_.erase(id);
        }

        DEBUG_OUT("[2] EventBuffer::push => " << event.to_string());
        return sequence;
    }

    // Push an item to the buffer. Automatically grows if full.
    uint64_t push(Event&& item) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        if (closed_) {
            return 0;
        }
        uint64_t sequence = buffer_.size();
        buffer_.emplace_back(std::move(item));
        auto& event = buffer_[sequence];
        DEBUG_OUT("EventBuffer::push => " << event.to_string());

        std::vector<uint64_t> handlers;
        for (auto& handler : handlers_) {
            if (!handler.second(event)) {
                handlers.push_back(handler.first);
                // it = handlers_.erase(it);
                continue;
            }
        }
        for (auto id : handlers) {
            handlers_.erase(id);
        }

        DEBUG_OUT("[2] EventBuffer::push => " << event.to_string());
        return sequence;
    }

    // Check if buffer is empty
    bool empty() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return buffer_.empty();
    }

    // Check if buffer is full
    bool full() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return buffer_.size() == buffer_.capacity();
    }

    size_t size() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return buffer_.size();
    }

    // Register a callback to be called on each push.
    // Returns a HandlerRegistration object that can be used to unregister the handler in O(1).
    uint64_t on(OnPush cb) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        auto id = handler_id_.fetch_add(1);
        handlers_.insert({id, std::move(cb)});
        return id;
    }

    // Unregister a handler by iterator (O(1)), used by HandlerRegistration.
    void unregister(uint64_t id) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        handlers_.erase(id);
    }

    void close() {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        handlers_.clear();
        buffer_.clear();
        closed_ = true;
    }

    // Clear the buffer
    void clear() {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        buffer_.clear();
    }

    void clearHandlers() {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        handlers_.clear();
    }

    // Iterate through all valid values in the buffer, protected by mutex.
    // The lambda should take a const Event&.
    template <typename Func>
    void iterate(Func&& func) const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        for (auto& item : buffer_) {
            func(item);
        }
    }

  protected:
    bool closed_;
    std::vector<Event> buffer_;
    mutable std::recursive_mutex mutex_;
    bool isDestructing = false;
    std::atomic<uint64_t> handler_id_{0};
    std::unordered_map<uint64_t, OnPush> handlers_;
};

struct TestLoop;

template <bool SSL>
struct TestTCPConnection;

template <bool SSL>
struct TestTCPServerApp;

template <bool SSL>
struct TestTCPClientApp;

struct TestLoop {
    Loop* loop;
    EventBuffer events;
    std::thread loopThread;
    std::atomic<bool> running{false};
    std::mutex loopMutex{};
    std::condition_variable loopReady{};
    std::atomic<bool> loopInitialized{false};
    std::atomic<uint64_t> connectionId{1};
    std::thread::id loopThreadId;
    us_timer_t* timer = nullptr;

    TestLoop() : events(8192) {
        loopThread = std::thread([this]() {
            loopThreadId = std::this_thread::get_id();
            loop = Loop::get();
            auto l = loop;

            timer = us_create_timer(reinterpret_cast<us_loop_t*>(loop), 0, 0);
            us_timer_set(timer, [](struct us_timer_t*) {}, 1, 50);

            // Signal that loop is ready
            {
                std::lock_guard<std::mutex> lock(loopMutex);
                loopInitialized = true;
            }
            loopReady.notify_all();

            // Run the event loop
            l->run();

            DEBUG_OUT("Loop::thread exit");
        });
        // loopThread.detach();

        std::unique_lock<std::mutex> lock(loopMutex);
        loopReady.wait(lock, [this]() { return loopInitialized.load(); });
    }

    ~TestLoop() {
        DEBUG_OUT("[1] TestLoop::~TestLoop " << reinterpret_cast<uintptr_t>((void*)this));
        events.close();

        if (timer) {
            call(
                [this]() {
                    us_timer_close(timer);
                    timer = nullptr;
                },
                10000
            );
        }

        DEBUG_OUT("[2] TestLoop::~TestLoop");

        // Wait for the thread to finish
        if (loopThread.joinable()) {
            loopThread.join();
        }

        DEBUG_OUT("[3] TestLoop::~TestLoop");
    }

    static TestLoop* create() {
        auto loop = new TestLoop();
        return loop;
    }

    uint64_t getNextConnectionId() {
        return connectionId.fetch_add(1);
    }

    Loop* getLoop() {
        // Wait for loop to be initialized
        std::unique_lock<std::mutex> lock(loopMutex);
        loopReady.wait(lock, [this]() { return loopInitialized.load(); });
        return loop;
    }

    // Waits for the loopThread to exit (join), if it exists and is joinable.
    void waitForExit() {
        if (loopThread.joinable()) {
            loopThread.join();
        }
    }

    template <typename F>
    auto call(F&& fn, int timeoutMs = 5000000) -> std::invoke_result_t<F&&> {
        using ReturnT = std::invoke_result_t<F&&>;
        if (std::this_thread::get_id() == loopThreadId) {
            if constexpr (std::is_void_v<ReturnT>) {
                fn();
                return;
            } else {
                return fn();
            }
        }
        auto future = callAsync(std::forward<F>(fn));
        if constexpr (std::is_void_v<ReturnT>) {
            if (future.wait_for(std::chrono::milliseconds(timeoutMs)) !=
                std::future_status::ready) {
                DEBUG_OUT("[WARNING] TestLoop::call timed out after " << timeoutMs << "ms");
                throw std::runtime_error("runOnLoop timed out");
            }
            future.get();
        } else {
            if (future.wait_for(std::chrono::milliseconds(timeoutMs)) !=
                std::future_status::ready) {
                DEBUG_OUT("[WARNING] TestLoop::call timed out after " << timeoutMs << "ms");
                throw std::runtime_error("runOnLoop timed out");
            }
            return future.get();
        }
    }

    // Overload for simple async execution, returns a future with the result type of F
    template <typename F>
    auto callAsync(F&& fn) -> std::future<std::invoke_result_t<F>> {
        using ReturnT = std::invoke_result_t<F>;
        auto promise = std::promise<ReturnT>();
        auto future = promise.get_future();
        auto loop = this->loop;
        assert(loop != nullptr);

        loop->defer([fn = std::forward<F>(fn), promise = std::move(promise)]() mutable {
            try {
                if constexpr (std::is_void_v<ReturnT>) {
                    fn();
                    promise.set_value();
                } else {
                    promise.set_value(fn());
                }
            } catch (...) { promise.set_exception(std::current_exception()); }
        });

        return future;
    }

    uint64_t tap(EventBuffer::OnPush cb) {
        return events.on(std::move(cb));
    }
};

template <bool SSL>
struct alignas(16) TestTCPConnection {
    TestLoop* loop = nullptr;
    EventBuffer* loopEvents = nullptr;
    uint64_t id = 0;
    int64_t established = 0;
    std::string host = "";
    int port = 0;
    bool isClient = false;
    std::string source_host = "";
    int options = 0;
    TCPConnection<SSL, TestTCPConnection<SSL>*>* connection = nullptr;
    bool closed = false;
    EventBuffer localEvents{128};
    std::string readBuffer = "";
    size_t readPackets = 0;
    std::mutex mutex{};

    TestTCPConnection(TestLoop* loop, uint64_t id) {
        CHECK(loop);
        CHECK(id != 0);
        this->id = id;
        this->loop = loop;

        localEvents.on([this](Event& event) {
            if (event.type == EventType::TCPOnData) {
                std::lock_guard<std::mutex> lock(mutex);
                readBuffer.append(event.data);
                readPackets++;
            }
            return true;
        });
    }

    ~TestTCPConnection() {
        DEBUG_OUT("TestTCPConnection::~TestTCPConnection => " << toString());
        closed = true;
        loop = nullptr;
        connection = nullptr;
        // DEBUG_OUT("[2] TestTCPConnection::~TestTCPConnection");
    }

    SendStatus send(std::string_view data, int timeoutMs = 5000) {
        auto loop = this->loop;
        CHECK(loop);
        return loop->call([this, data]() { return connection->send(data); }, timeoutMs);
    }

    std::string read(int timeoutMs = 5000) {
        auto loop = this->loop;
        CHECK(loop);
        return loop->call([this]() { return connection->read(); }, timeoutMs);
    }

    std::future<std::string> readAsync(int timeoutMs = 5000) {
        auto loop = this->loop;
        CHECK(loop);
        return loop->callAsync([this]() { return connection->read(); }, timeoutMs);
    }

    std::string toString() const {
        std::ostringstream oss;
        oss << "TestTCPConnection[" << (SSL ? "SSL" : "PLAIN") << "]{"
            << "id=" << id << ", established=" << established << ", isClient=" << isClient
            << ", host=" << host << ", port=" << port << ", source_host=" << source_host
            << ", options=" << options << ", closed=" << (closed ? "true" : "false")
            << ", connection=" << static_cast<const void*>(connection) << "}";
        // << ", localEvents.size=" << localEvents.size()
        // << ", localEvents.capacity=" << localEvents.capacity() << "}";
        return oss.str();
    }
};

template <bool SSL>
void wireTCPBehavior(
    bool isServer, TestLoop* loop, TCPBehavior<SSL, TestTCPConnection<SSL>*>& behavior
) {
    behavior.onData =
        [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn, std::string_view data) {
            DEBUG_OUT("onData");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK(*userData);
            loop->events.push(Event(
                *userData ? (*userData)->id : 0,
                conn,
                EventType::TCPOnData,
                0,
                std::string(data),
                nullptr,
                data.size()
            ));
        };

    if (isServer) {
        behavior.onOpen = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
            DEBUG_OUT("onConnection - server");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            // *userData = nullptr;
            CHECK_EQ(*userData, nullptr);

            // For server, create a new TestTCPConnection when connection is accepted
            auto connection = new TestTCPConnection<SSL>(loop, loop->getNextConnectionId());
            connection->isClient = false;
            connection->loopEvents = &loop->events;
            connection->established = 0;
            connection->connection = conn;

            // Store the pointer in user data
            *userData = connection;

            DEBUG_OUT("onConnection server before on push");
            loop->events.push(
                Event(connection->id, conn, EventType::TCPOnConnection, 0, "", nullptr, 0)
            );
            DEBUG_OUT("onConnection server after on push");
        };
    } else {
        behavior.onOpen = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
            DEBUG_OUT("onConnection - client");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK(*userData);

            // (*userData)->connection = conn;
            // if ((*userData)->id == 0) {
            //     (*userData)->id = loop->getNextConnectionId();
            // }
            DEBUG_OUT("onConnection before on push");
            loop->events.push(Event(
                *userData ? (*userData)->id : 0, conn, EventType::TCPOnConnection, 0, "", nullptr, 0
            ));
            DEBUG_OUT("onConnection after on push");
        };
    }

    behavior.onClose =
        [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn, int code, void* data) {
            DEBUG_OUT("onDisconnected");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            auto connId = userData && *userData ? (*userData)->id : 0;
            // CHECK(*userData);
            loop->events.push(Event(connId, conn, EventType::TCPOnDisconnected, code, "", data, 0));
            if (*userData) {
                delete *userData;
                *userData = nullptr;

                loop->events.push(Event(
                    connId,
                    conn,
                    EventType::TCPOnTestTCPConnectionDestroyed,
                    code,
                    "onDisconnected",
                    data,
                    0
                ));
            }
        };

    behavior.onConnectError =
        [loop = loop](
            TCPConnection<SSL, TestTCPConnection<SSL>*>* conn, int code, std::string_view message
        ) {
            DEBUG_OUT("onConnectError");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK(*userData);
            auto connId = *userData ? (*userData)->id : 0;
            loop->events.push(Event(
                connId,
                conn,
                EventType::TCPOnConnectError,
                code,
                std::string(message),
                nullptr,
                message.size()
            ));
            if (*userData) {
                delete *userData;
                *userData = nullptr;

                std::string messageStr = std::string("onConnectError");
                if (!messageStr.empty()) {
                    messageStr.append(": ");
                    messageStr.append(message);
                }

                loop->events.push(Event(
                    connId,
                    conn,
                    EventType::TCPOnTestTCPConnectionDestroyed,
                    code,
                    messageStr,
                    nullptr,
                    0
                ));
            }
        };

    behavior.onWritable =
        [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn, uintmax_t bytes) -> bool {
        DEBUG_OUT("onWritable");
        auto connData = conn->getConnectionData();
        auto userData = conn->getUserData();
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            *userData ? (*userData)->id : 0, conn, EventType::TCPOnWritable, 0, "", nullptr, bytes
        ));
        return true;
    };

    behavior.onDrain = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
        DEBUG_OUT("onDrain");
        auto connData = conn->getConnectionData();
        auto userData = conn->getUserData();
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(
            Event(*userData ? (*userData)->id : 0, conn, EventType::TCPOnDrain, 0, "", nullptr, 0)
        );
    };

    behavior.onDropped =
        [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn, std::string_view data) {
            DEBUG_OUT("onDropped");
            auto connData = conn->getConnectionData();
            auto userData = conn->getUserData();
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK(*userData);
            loop->events.push(Event(
                *userData ? (*userData)->id : 0,
                conn,
                EventType::TCPOnDropped,
                0,
                std::string(data),
                nullptr,
                0
            ));
        };

    behavior.onEnd = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
        DEBUG_OUT("onEnd");
        auto connData = conn->getConnectionData();
        auto userData = conn->getUserData();
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(
            Event(*userData ? (*userData)->id : 0, conn, EventType::TCPOnEnd, 0, "", nullptr, 0)
        );
        // if (*userData) {
        //     loop->events.unregister((*userData)->tapId);
        //     DEBUG_OUT("deleting user data");
        //     delete *userData;
        //     *userData = nullptr;
        //     // *reinterpret_cast<TestTCPConnection<SSL>**>(conn->getUserData()) = nullptr;
        // }
        // close the connection
        return true;
    };

    behavior.onTimeout = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
        DEBUG_OUT("onTimeout");
        auto connData = conn->getConnectionData();
        auto userData = conn->getUserData();
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        // CHECK(*userData);
        loop->events.push(
            Event(*userData ? (*userData)->id : 0, conn, EventType::TCPOnTimeout, 0, "", nullptr, 0)
        );
        conn->close();
    };

    behavior.onLongTimeout = [loop = loop](TCPConnection<SSL, TestTCPConnection<SSL>*>* conn) {
        DEBUG_OUT("onLongTimeout");
        auto connData = conn->getConnectionData();
        auto userData = conn->getUserData();
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        // CHECK(*userData);
        loop->events.push(Event(
            *userData ? (*userData)->id : 0, conn, EventType::TCPOnLongTimeout, 0, "", nullptr, 0
        ));
        conn->close();
    };

    if (isServer) {
        behavior.onServerName = [loop = loop](
                                    TCPContext<SSL, TestTCPConnection<SSL>*>* context,
                                    std::string_view hostname
                                ) {
            DEBUG_OUT("onServerName");
            CHECK(context);
            loop->events.push(Event(
                0,
                context,
                EventType::TCPOnLongTimeout,
                0,
                std::string(hostname),
                nullptr,
                hostname.size()
            ));
        };
    }
}

template <bool SSL>
struct TestTCPServerApp {
    TestLoop* loop;
    TemplatedTCPServerApp<SSL, TestTCPConnection<SSL>*>* app;
    int port = 0;

  private:
    struct PrivateTag {};

  public:
    TestTCPServerApp(TestLoop* loop) : loop(loop) {
        app = loop->call([loop]() {
            TCPBehavior<SSL, TestTCPConnection<SSL>*> behavior;
            wireTCPBehavior<SSL>(true, loop, behavior);
            return TemplatedTCPServerApp<SSL, TestTCPConnection<SSL>*>::listenIP4(
                loop->loop, ip_to_uint32("127.0.0.1"), 0, 0, std::move(behavior)
            );
        });

        port = app->getListenerPort();
    }

    ~TestTCPServerApp() {
        DEBUG_OUT("TestTCPServerApp::~TestTCPServerApp");
        if (loop && app) {
            loop->call(
                [this]() {
                    delete app;
                    app = nullptr;
                },
                1000
            );
        }
        loop = nullptr;
    }
};

enum class ConnectFutureCode {
    CONNECTING,
    SUCCESS,
    BAD_HOST,
    FAILED,
    TIMED_OUT,
};

std::string ConnectFutureCode_to_string(ConnectFutureCode code) {
    switch (code) {
    case ConnectFutureCode::CONNECTING: return "WAITING";
    case ConnectFutureCode::SUCCESS: return "OK";
    case ConnectFutureCode::BAD_HOST: return "BAD_HOST";
    case ConnectFutureCode::FAILED: return "HOST_NOT_AVAILABLE";
    case ConnectFutureCode::TIMED_OUT: return "TIMED_OUT";
    }
    return "UNKNOWN";
}

template <bool SSL>
struct TCPConnectFuture : std::enable_shared_from_this<TCPConnectFuture<SSL>> {

    TestLoop* loop;
    uint64_t connId;
    std::string host;
    uint32_t host_ip4;
    int port;
    std::string source_host;
    uint32_t source_ip4;
    int options;

    ConnectFutureCode code = ConnectFutureCode::CONNECTING;
    TestTCPConnection<SSL>* connection = nullptr;
    uint64_t tapId = 0;
    int64_t connectingAt = 0;
    int64_t connectedAt = 0;
    int64_t completedAt = 0;
    int64_t connectErrorAt = 0;

    // Use mutex and condition_variable for synchronization
    std::mutex mutex;
    std::condition_variable cv;

    TCPConnectFuture(
        uint64_t connId, TestLoop* loop, /*std::future<TestTCPConnection<SSL>*> future,*/
        std::string host, uint32_t host_ip4, int port, std::string source_host, uint32_t source_ip4,
        int options
    )
        : connId(connId),
          loop(loop),
          host(host),
          host_ip4(host_ip4),
          port(port),
          source_host(source_host),
          source_ip4(source_ip4),
          options(options) {
        connectingAt = unixNanos();
    }

    ~TCPConnectFuture() {
        DEBUG_OUT("ConnectFuture::~ConnectFuture");
        // if (tapId > 0) {
        //     loop->events.unregister(tapId);
        //     tapId = 0;
        // }
        if (connection != nullptr) {
            if (code != ConnectFutureCode::SUCCESS) {
                // DEBUG_OUT("ConnectFuture::~ConnectFuture needs to close connection");
                loop->call([this]() {
                    auto c = connection;
                    if (c == nullptr) {
                        return;
                    }
                    auto conn = c->connection;
                    if (conn == nullptr) {
                        return;
                    }
                    if (connectErrorAt == 0) {
                        DEBUG_OUT("us_socket_close_connecting");
                        us_socket_close_connecting(SSL, (us_socket_t*)conn);
                    } else {
                        DEBUG_OUT("us_socket_close");
                        us_socket_close(SSL, (us_socket_t*)conn, 0, nullptr);
                    }
                });
            }
        }
    }

    ConnectFutureCode wait(int timeoutMs) {
        bool signaled = false;
        ConnectFutureCode code = ConnectFutureCode::CONNECTING;

        {
            std::unique_lock<std::mutex> lock(mutex);
            code = this->code;
            if (code != ConnectFutureCode::CONNECTING) {
                return code;
            }

            // DEBUG_OUT("ConnectFuture::wait");
            // Wait for code to change from WAITING, or timeout
            bool signaled = cv.wait_for(lock, std::chrono::milliseconds(timeoutMs), [this] {
                return this->code != ConnectFutureCode::CONNECTING;
            });
            // DEBUG_OUT(
            //     "ConnectFuture::wait signaled: " << signaled
            //                                      << " code: " <<
            //                                      ConnectFutureCode_to_string(code)
            // );

            code = this->code;
        }
        {
            std::unique_lock<std::mutex> lock(mutex);
            code = this->code;
            DEBUG_OUT("SIGNALED: " << ConnectFutureCode_to_string(this->code));
        }
        if (code == ConnectFutureCode::CONNECTING) {
            this->complete(ConnectFutureCode::TIMED_OUT);
        }
        return code;
    }

    void onConnectionDestroyed() {
        bool shouldFail = false;
        {
            std::lock_guard<std::mutex> lock(mutex);
            connection = nullptr;
            if (code == ConnectFutureCode::CONNECTING) {
                shouldFail = true;
            }
        }
        if (shouldFail) {
            this->complete(ConnectFutureCode::FAILED);
        }
    }

    void onConnectionError() {
        {
            std::lock_guard<std::mutex> lock(mutex);
            connection = nullptr;
            connectErrorAt = unixNanos();
        }
        this->complete(ConnectFutureCode::FAILED);
    }

    void onConnection() {
        {
            std::lock_guard<std::mutex> lock(mutex);
            connectedAt = unixNanos();
        }
        this->complete(ConnectFutureCode::SUCCESS);
    }

    ConnectFutureCode complete(ConnectFutureCode newCode) {
        bool shouldDisconnect = false;
        {
            DEBUG_OUT("ConnectFuture::complete " << ConnectFutureCode_to_string(newCode));
            std::lock_guard<std::mutex> lock(mutex);

            // Do we have progress?
            if (this->code != ConnectFutureCode::CONNECTING) {
                switch (this->code) {
                case ConnectFutureCode::BAD_HOST:
                    CHECK_NE(newCode, ConnectFutureCode::CONNECTING);
                    CHECK_NE(newCode, ConnectFutureCode::SUCCESS);
                    if (newCode == ConnectFutureCode::BAD_HOST) {
                        return ConnectFutureCode::BAD_HOST;
                    } else if (newCode == ConnectFutureCode::TIMED_OUT) {
                        shouldDisconnect = true;
                    }
                    break;

                case ConnectFutureCode::FAILED:
                    CHECK_NE(newCode, ConnectFutureCode::CONNECTING);
                    CHECK_NE(newCode, ConnectFutureCode::SUCCESS);

                    if (newCode == ConnectFutureCode::BAD_HOST) {
                        // FAILED is a better result than BAD_HOST since it happens via
                        // on_connect_error
                    } else if (newCode == ConnectFutureCode::TIMED_OUT) {
                        shouldDisconnect = false;
                    }
                    break;

                case ConnectFutureCode::SUCCESS:
                    CHECK_NE(newCode, ConnectFutureCode::CONNECTING);
                    CHECK_NE(newCode, ConnectFutureCode::BAD_HOST);
                    CHECK_NE(newCode, ConnectFutureCode::FAILED);

                    // Ignore timeout for success
                    if (newCode == ConnectFutureCode::TIMED_OUT) {
                        return ConnectFutureCode::SUCCESS;
                    }

                    break;

                case ConnectFutureCode::TIMED_OUT:
                    if (newCode == ConnectFutureCode::TIMED_OUT) {
                        return ConnectFutureCode::TIMED_OUT;
                    }
                    shouldDisconnect = connection != nullptr && connectErrorAt == 0;
                    break;

                default:
                    throw std::runtime_error(
                        "ConnectFuture::complete invalid code: " +
                        std::to_string(static_cast<int>(newCode))
                    );
                }
            } else {
                switch (newCode) {
                case ConnectFutureCode::BAD_HOST:
                    shouldDisconnect = connection != nullptr && connectErrorAt == 0;
                    break;
                case ConnectFutureCode::FAILED: shouldDisconnect = false; break;
                case ConnectFutureCode::SUCCESS: shouldDisconnect = false; break;
                case ConnectFutureCode::TIMED_OUT: break;

                default:
                    throw std::runtime_error(
                        "ConnectFuture::complete invalid code: " +
                        std::to_string(static_cast<int>(newCode))
                    );
                }
            }
            if (shouldDisconnect) {
                shouldDisconnect = connection != nullptr && connectErrorAt == 0;
            }
            this->code = newCode;
        }

        if (shouldDisconnect) {
            std::shared_ptr<TCPConnectFuture<SSL>> self = this->shared_from_this();
            loop->call([this, self = self, connection = this->connection]() {
                std::lock_guard<std::mutex> lock(mutex);
                auto c = connection;
                if (c == nullptr) {
                    return;
                }
                auto conn = c->connection;
                if (conn == nullptr || connectErrorAt != 0) {
                    return;
                }

                auto userData = conn->getUserData();
                if (userData == nullptr || *userData == nullptr) {
                    return;
                }

                if (connectedAt == 0) {
                    DEBUG_OUT("us_socket_close_connecting");
                    us_socket_close_connecting(SSL, (us_socket_t*)conn);
                } else {
                    DEBUG_OUT("us_socket_close");
                    us_socket_close(SSL, (us_socket_t*)conn, 0, nullptr);
                }
            });
        }

        std::lock_guard<std::mutex> lock(mutex);
        cv.notify_all();

        return this->code;
    }
};

template <bool SSL>
struct TestTCPClientApp {
    TestLoop* loop;
    std::unique_ptr<TemplatedTCPClientApp<SSL, TestTCPConnection<SSL>*>> app;

    mutable std::mutex connectionsMutex;
    std::unordered_map<uint64_t, TestTCPConnection<SSL>*> connections;
    std::unordered_map<uint64_t, std::shared_ptr<TCPConnectFuture<SSL>>> connecting;
    uint64_t tapId = 0;

  private:
    struct PrivateTag {};

  public:
    TestTCPClientApp(TestLoop* loop) : loop(loop) {
        // first event should determine if the connection is established or not
        tapId = loop->tap([this](Event& event) {
            DEBUG_OUT("tap :: TCPClient << " << event.to_string());
            std::lock_guard<std::mutex> lock(connectionsMutex);
            auto it = connecting.find(event.conn_id);
            if (it == connecting.end()) {
                auto it = connections.find(event.conn_id);
                if (it == connections.end()) {
                    return true;
                }
                auto connection = it->second;

                return true;
            }
            auto future = it->second;
            auto connId = future->connId;

            if (event.type == EventType::TCPOnConnecting) {
                // do nothing
            } else if (event.type == EventType::TCPOnTestTCPConnectionDestroyed) {
                connecting.erase(connId);
                future->onConnectionDestroyed();
            } else if (event.type == EventType::TCPOnConnection) {
                connecting.erase(connId);
                connections.insert({connId, (TestTCPConnection<SSL>*)event.connection});
                future->onConnection();
            } else if (event.type == EventType::TCPOnConnectError) {
                connecting.erase(connId);
                future->onConnectionError();
            } else {
                DEBUG_OUT("UNREACHABLE");
            }

            // unregister the event handler
            return true;
        });

        app = loop->call([loop]() {
            TCPBehavior<SSL, TestTCPConnection<SSL>*> behavior;
            wireTCPBehavior<SSL>(false, loop, behavior);
            return std::make_unique<TemplatedTCPClientApp<SSL, TestTCPConnection<SSL>*>>(
                loop->loop, std::move(behavior)
            );
        });
    }

    ~TestTCPClientApp() {
        DEBUG_OUT("TestTCPClientApp::~TestTCPClientApp");

        loop->events.unregister(tapId);

        loop->call(
            [this]() {
                for (auto& [connId, future] : connecting) {
                    future->onConnectionError();
                }

                for (auto& [connId, connection] : connections) {
                    // connection->connection->close();
                }

                connecting.clear();
                connections.clear();
                tapId = 0;

                app = nullptr;
            },
            10000
        );
    }

    // Connect to a host asynchronously, returning a future to the TCPConnection pointer.
    std::shared_ptr<TCPConnectFuture<SSL>> connectAsync(
        std::string host, uint32_t host_ip4, int port, const std::string& source_host = "",
        uint32_t source_ip4 = 0, int options = 0
    ) {
        auto future = std::make_shared<TCPConnectFuture<SSL>>(
            loop->getNextConnectionId(),
            loop,
            host,
            host_ip4,
            port,
            source_host,
            source_ip4,
            options
        );

        {
            std::lock_guard<std::mutex> lock(connectionsMutex);
            connecting.insert({future->connId, future});
        }

        // connect the client on loop
        loop->callAsync([this,
                         loop = loop,
                         future = future,
                         host = host,
                         host_ip4 = host_ip4,
                         port = port,
                         source_host = source_host,
                         source_ip4 = source_ip4,
                         options = options]() mutable {
            // assign a connection id
            auto connId = future->connId;
            auto connectingAt = unixNanos();

            DEBUG_OUT(
                "connecting " << host << ":" << port << " " << source_host << " " << options
                              << " conn_id = " << connId
            );

            // create the us_socket_t* connection
            TCPConnection<SSL, TestTCPConnection<SSL>*>* conn = nullptr;

            if (!host.empty()) {
                if (source_host.empty()) {
                    conn = app->connect(host.c_str(), port, nullptr, options);
                } else {
                    conn = app->connect(host.c_str(), port, source_host.c_str(), options);
                }
            } else if (host_ip4 != 0) {
                conn = app->connectIP4(host_ip4, port, source_ip4, options);
            }

            DEBUG_OUT("connection address: " << static_cast<uintptr_t>((uintptr_t)(void*)conn));

            // check if the connection is valid
            if (!conn) {
                {
                    std::lock_guard<std::mutex> lock(connectionsMutex);
                    connecting.erase(connId);
                }

                loop->events.push(Event(
                    connId,
                    nullptr,
                    EventType::TCPOnConnectError,
                    0,
                    std::string("REJECTED_HOST"),
                    nullptr,
                    std::string("REJECTED_HOST").size()
                ));

                future->complete(ConnectFutureCode::BAD_HOST);
                return;
            }

            // create a new TestTCPConnection on the heap
            auto connection = new TestTCPConnection<SSL>(loop, connId);
            future->connection = connection;
            connection->isClient = true;
            connection->host = host;
            connection->port = port;
            connection->source_host = source_host;
            connection->options = options;
            connection->loopEvents = &loop->events;
            connection->established = 0;
            connection->connection = conn;

            // store the pointer in the user data
            *reinterpret_cast<TestTCPConnection<SSL>**>(conn->getUserData()) = connection;

            loop->events.push(Event(connId, conn, EventType::TCPOnConnecting, 0, "", nullptr, 0));
        });

        return future;
    }

    std::shared_ptr<TCPConnectFuture<SSL>>
    connectAsyncIP4(uint32_t host_ip4, int port, uint32_t source_ip4 = 0, int options = 0) {
        return connectAsync("", host_ip4, port, "", source_ip4, options);
    }

    std::shared_ptr<TCPConnectFuture<SSL>>
    connectAsyncIP4(std::string host, int port, std::string source_host = "", int options = 0) {
        return connectAsync(host, 0, port, source_host, 0, options);
    }

    // Connect to a host synchronously, blocking until connection is established or timeoutMs
    // expires.
    std::shared_ptr<TCPConnectFuture<SSL>> connect(
        std::string host, int port, std::string source_host = "", int options = 0,
        int timeoutMs = 6000
    ) {
        auto fut = connectAsync(host, 0, port, source_host, 0, options);
        fut->wait(timeoutMs);
        return fut;
    }

    std::shared_ptr<TCPConnectFuture<SSL>> connectIP4(
        uint32_t host_ip4, int port, uint32_t source_ip4 = 0, int options = 0, int timeoutMs = 6000
    ) {
        auto fut = connectAsyncIP4(host_ip4, port, source_ip4, options);
        fut->wait(timeoutMs);
        return fut;
    }
};

#define CHECK_CONNECT_SUCCESS(result)                                                              \
    do {                                                                                           \
        auto r = result;                                                                           \
        CHECK_EQ(r->code, ConnectFutureCode::SUCCESS);                                             \
        CHECK(r->connection);                                                                      \
    } while (0)

#define CHECK_CONNECT_BAD_HOST(result)                                                             \
    do {                                                                                           \
        auto r = result;                                                                           \
        CHECK_EQ(r->code, ConnectFutureCode::BAD_HOST);                                            \
        CHECK_EQ(r->connection, nullptr);                                                          \
    } while (0)

#define CHECK_CONNECT_FAILED(result)                                                               \
    do {                                                                                           \
        auto r = result;                                                                           \
        CHECK_EQ(r->code, ConnectFutureCode::FAILED);                                              \
        CHECK(r->connection != nullptr);                                                           \
    } while (0)

#define CHECK_CONNECT_ERROR(result)                                                                \
    do {                                                                                           \
        auto r = result;                                                                           \
        CHECK_NE(r->code, ConnectFutureCode::SUCCESS);                                             \
        CHECK_NE(r->code, ConnectFutureCode::CONNECTING);                                          \
    } while (0)

#define CHECK_CONNECT_TIMED_OUT(result)                                                            \
    do {                                                                                           \
        auto r = result;                                                                           \
        CHECK_EQ(r->code, ConnectFutureCode::TIMED_OUT);                                           \
    } while (0)

struct TestFixture {
    TestLoop* loop;

    TestFixture() {
        loop = TestLoop::create();
        CHECK(loop);
    }

    ~TestFixture() {
        // DEBUG_OUT("Cleaning up loop");
        delete loop;
    }
};


TEST_CASE("Loop Creation") {
    DEBUG_OUT("Creating TestLoop...");
    // Try to create a loop directly
    auto loop = TestLoop::create();
    DEBUG_OUT("TestLoop created successfully");
    CHECK(loop != nullptr);
    DEBUG_OUT("Deleting TestLoop...");
    delete loop;
    DEBUG_OUT("TestLoop deleted successfully");
}

// TEST_CASE("Testing EventBuffer") {
//     EventBuffer buffer(16);
//     CHECK_EQ(buffer.empty(), true);
//     CHECK_EQ(buffer.size(), 0);

//     Event event;
//     event.timestamp = 12345;
//     event.connection = nullptr;
//     event.type = EventType::TCPOnData;
//     event.code = 0;
//     event.data = "test";
//     event.data_ptr = nullptr;
//     event.bytes = 4;

//     CHECK_EQ(buffer.push(std::move(event)), 0);
//     CHECK_EQ(buffer.empty(), false);
//     CHECK_EQ(buffer.size(), 1);
// }

// TEST_CASE("TCPConnectionData 16-byte aligned") {
//     constexpr size_t size = getTCPConnectionDataSize<true, void*>();
//     CHECK_EQ(size % 16, 0); // 16-byte aligned
// }

TEST_CASE_FIXTURE(TestFixture, "Client and Server") {
    auto server = TestTCPServerApp<false>(loop);
    CHECK(server.port != 0);
    DEBUG_OUT("server: " << server.port);
    auto client = TestTCPClientApp<false>(loop);
    auto clientConn = client.connect("127.0.0.1", server.port);
    CHECK(clientConn);
    // Invalid IPs now return nullptr instead of throwing
    CHECK_CONNECT_ERROR(client.connect("299.99.99.99", server.port, "", 0, 1000));
    CHECK_CONNECT_ERROR(client.connect("264.0.0.999", server.port, "", 0, 1000));
}

TEST_CASE_FIXTURE(TestFixture, "Bad Connect") {
    auto client = TestTCPClientApp<false>(loop);
    CHECK_CONNECT_ERROR(client.connect("299.99.99.99", 54585, "", 0, 5000));
    CHECK_CONNECT_ERROR(client.connect("264.0.0.999", 54585, "", 0, 5000));
    CHECK_CONNECT_ERROR(client.connect("999.999.999.999", 54585, "", 0, 5000));
    CHECK_CONNECT_ERROR(client.connect("127.0.0.1", 54585, "", 0, 500));
    CHECK_CONNECT_ERROR(client.connectIP4(ip_to_uint32("127.0.0.1"), 54585, 0, 0, 500));
    CHECK_CONNECT_ERROR(client.connectIP4(ip_to_uint32("127.0.0.1"), 54586, 0, 0, 20));
}

TEST_CASE("IPv4 Address Conversion") {
    // Test the utility function for converting IP addresses to uint32_t
    CHECK_EQ(ip_to_uint32("127.0.0.1"), 0x7F000001u);
    CHECK_EQ(ip_to_uint32("8.8.8.8"), 0x08080808u);
    CHECK_EQ(ip_to_uint32("192.168.1.1"), 0xC0A80101u);
    CHECK_EQ(ip_to_uint32("255.255.255.255"), 0xFFFFFFFFu);
    CHECK_EQ(ip_to_uint32("0.0.0.0"), 0x00000000u);

    // Test invalid IPs
    CHECK_EQ(ip_to_uint32("invalid"), 0u);
    CHECK_EQ(ip_to_uint32("299.99.99.99"), 0u);
    CHECK_EQ(ip_to_uint32(""), 0u);
}

TEST_CASE_FIXTURE(TestFixture, "Testing Loop and TCPClientApp and TCPServerApp Instantiation") {
    auto client = TestTCPClientApp<false>(loop);
    CHECK(client.app != nullptr);
    auto server = TestTCPServerApp<false>(loop);
    CHECK(server.port != 0);
}
