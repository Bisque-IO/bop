#include <assert.h>
#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include <doctest/doctest.h>
#include <optional>
#include <unordered_set>

// Debug output control
#ifndef TCP_TEST_DEBUG
#define TCP_TEST_DEBUG 1
#endif

static std::mutex coutMutex;

#if TCP_TEST_DEBUG
#define DEBUG_OUT(x)                                                                               \
    do {                                                                                           \
        std::lock_guard<std::mutex> lock(coutMutex);                                               \
        std::cout << "[DEBUG] :: " << x << std::endl;                                              \
    } while (0)
#else
#define DEBUG_OUT(x)                                                                               \
    do {                                                                                           \
    } while (0)
#endif

#include "../lib/src/uws/Loop.h"
#include "../lib/src/uws/TCPClientApp.h"
#include "../lib/src/uws/TCPConnection.h"
#include "../lib/src/uws/TCPContext.h"
#include "../lib/src/uws/TCPServerApp.h"
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

using namespace uWS;

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
    uint64_t id{};
    void* connection{nullptr};
    EventType type{};
    int code{};
    std::string data{};
    void* data_ptr{};
    uintmax_t bytes;
    bool isClient = false;

    Event(
        uint64_t sequence, int64_t timestamp, uint64_t id, void* connection, EventType type,
        int code, std::string data, void* data_ptr, uintmax_t bytes
    )
        : sequence(sequence),
          timestamp(timestamp),
          id(id),
          connection(connection),
          type(type),
          code(code),
          data(data),
          data_ptr(data_ptr),
          bytes(bytes),
          isClient(
              connection ? reinterpret_cast<TCPConnection<false, void*>*>(connection)
                               ->getConnectionData()
                               ->isClient
                         : false
          ) {}

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
        out.append(std::to_string(id));
        out.append(", connection=");
        out.append(std::to_string(reinterpret_cast<uintptr_t>(connection)));
        out.append(", isClient=");
        out.append(std::to_string(isClient));
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

    explicit EventBuffer(size_t capacity) : buffer_(capacity), head_(0), tail_(0) {}

    // Move-only support: delete copy constructor and copy assignment
    EventBuffer(const EventBuffer&) = delete;
    EventBuffer& operator=(const EventBuffer&) = delete;

    // Push an item to the buffer. Automatically grows if full.
    bool push(Event& item) {
        DEBUG_OUT("EventBuffer::push => " << item.to_string());
        size_t index = 0;
        {
            std::lock_guard<std::recursive_mutex> lock(mutex_);
            size_t cap = buffer_.size();
            if (tail_ - head_ == cap) {
                // Buffer full, grow automatically
                grow(cap == 0 ? 8 : cap * 2);
                cap = buffer_.size();
            }
            index = tail_ % cap;
            buffer_[index] = std::move(item);
            ++tail_;

            auto& event = buffer_[index];
            std::lock_guard<std::recursive_mutex> cb_lock(handler_mutex_);
            for (auto it = handlers_.begin(); it != handlers_.end();) {
                if (!((*it).second)(event)) {
                    it = handlers_.erase(it);
                    continue;
                }
                ++it;
            }
        }
        return true;
    }

    // Push an item to the buffer. Automatically grows if full.
    bool push(Event&& item) {
        DEBUG_OUT("EventBuffer::push => " << item.to_string());
        size_t index = 0;
        {
            std::lock_guard<std::recursive_mutex> lock(mutex_);
            size_t cap = buffer_.size();
            if (tail_ - head_ == cap) {
                // Buffer full, grow automatically
                grow(cap == 0 ? 8 : cap * 2);
                cap = buffer_.size();
            }
            index = tail_ % cap;
            buffer_[index] = std::move(item);
            ++tail_;

            auto& event = buffer_[index];
            std::lock_guard<std::recursive_mutex> cb_lock(handler_mutex_);
            for (auto it = handlers_.begin(); it != handlers_.end();) {
                if (!((*it).second)(event)) {
                    it = handlers_.erase(it);
                    continue;
                }
                ++it;
            }
        }

        return true;
    }

    // Grows the buffer to at least new_capacity. Preserves order and elements.
    // Returns true if grown, false if new_capacity <= current capacity.
    bool grow(size_t new_capacity) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        size_t old_capacity = buffer_.size();
        if (new_capacity <= old_capacity) {
            return false; // No need to shrink or same size
        }

        size_t count = tail_ - head_;
        std::vector<Event> new_buffer(new_capacity);

        // Move elements in order from old buffer to new buffer
        for (size_t i = 0; i < count; ++i) {
            new_buffer[i] = std::move(buffer_[(head_ + i) % old_capacity]);
        }

        buffer_ = std::move(new_buffer);
        head_ = 0;
        tail_ = count;
        // Note: head_ and tail_ are always modulo (capacity * 2) for overflow safety

        return true;
    }

    // Pop an item from the buffer. Returns false if empty.
    bool pop(Event& item) {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        size_t cap = buffer_.size();
        if (tail_ - head_ == 0) {
            return false; // Buffer empty
        }
        item = std::move(buffer_[head_ % cap]);
        ++head_;
        return true;
    }

    // Check if buffer is empty
    bool empty() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return tail_ - head_ == 0;
    }

    // Check if buffer is full
    bool full() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return tail_ - head_ == buffer_.size();
    }

    size_t size() const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        return tail_ - head_;
    }

    // Get capacity
    size_t capacity() const {
        return buffer_.size();
    }

    // Register a callback to be called on each push.
    // Returns a HandlerRegistration object that can be used to unregister the handler in O(1).
    uint64_t on(OnPush cb) {
        std::lock_guard<std::recursive_mutex> lock(handler_mutex_);
        auto id = handler_id_.fetch_add(1);
        handlers_.insert({id, std::move(cb)});
        return id;
    }

    // Unregister a handler by iterator (O(1)), used by HandlerRegistration.
    void unregister(uint64_t id) {
        std::lock_guard<std::recursive_mutex> lock(handler_mutex_);
        handlers_.erase(id);
    }

    // Clear the buffer
    void clear() {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        size_t cap = buffer_.size();
        for (size_t i = head_; i < tail_; ++i) {
            buffer_[(i % cap)] = Event{};
        }
        head_ = 0;
        tail_ = 0;
    }

    // Iterate through all valid values in the buffer, protected by mutex.
    // The lambda should take a const Event&.
    template <typename Func>
    void iterate(Func&& func) const {
        std::lock_guard<std::recursive_mutex> lock(mutex_);
        size_t cap = buffer_.size();
        if (tail_ - head_ == 0)
            return;
        for (size_t idx = head_; idx < tail_; ++idx) {
            func(buffer_[(idx % cap)]);
        }
    }

  protected:
    std::vector<Event> buffer_;
    size_t head_;
    size_t tail_;
    mutable std::recursive_mutex mutex_;
    mutable std::recursive_mutex handler_mutex_;
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

struct TestLoop : std::enable_shared_from_this<TestLoop> {
    Loop* loop;
    EventBuffer events;
    std::thread loopThread;
    std::atomic<bool> running{false};
    std::mutex loopMutex{};
    std::condition_variable loopReady{};
    std::atomic<bool> loopInitialized{false};
    std::atomic<uint64_t> connectionId{1};
    std::atomic<uint64_t> sequence_{0};
    us_timer_t* timer = nullptr;

    TestLoop() : events(8192) {
        // Constructor just initializes the events buffer
        // Thread creation will be done in create()
    }

    void initialize(TestLoop* self) {
        loopThread = std::thread([this, self]() {
            // Get the loop for this thread
            loop = Loop::get();

            timer = us_create_timer(reinterpret_cast<us_loop_t*>(loop), 0, 0);
            us_timer_set(timer, [](struct us_timer_t*) {}, 1, 1000 * 60 * 60 * 24);

            // Signal that loop is ready
            {
                std::lock_guard<std::mutex> lock(loopMutex);
                loopInitialized = true;
            }
            loopReady.notify_all();

            // Run the event loop
            running = true;
            loop->run();
            running = false;
            loop = nullptr;

            DEBUG_OUT("loopThread exit");
        });

        std::unique_lock<std::mutex> lock(loopMutex);
        loopReady.wait(lock, [this]() { return loopInitialized.load(); });
    }

    ~TestLoop() {
        DEBUG_OUT("TestLoop::~TestLoop start");

        if (timer) {
            call([this]() {
                us_timer_close(timer);
                timer = nullptr;
            });
        }

        // Wait for the thread to finish
        if (loopThread.joinable()) {
            loopThread.join();
        }

        loop = nullptr;
        loopInitialized = false;

        DEBUG_OUT("TestLoop::~TestLoop end");
    }

    static TestLoop* create() {
        auto loop = new TestLoop();
        loop->initialize(loop);
        return loop;
    }

    uint64_t getNextConnectionId() {
        return connectionId.fetch_add(1);
    }

    uint64_t getNextSequence() {
        return sequence_.fetch_add(1);
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
    auto call(F&& fn, int timeoutMs = 5000) -> std::invoke_result_t<F&&> {
        using ReturnT = std::invoke_result_t<F&&>;
        auto future = callAsync(std::forward<F>(fn));
        if constexpr (std::is_void_v<ReturnT>) {
            if (future.wait_for(std::chrono::milliseconds(timeoutMs)) !=
                std::future_status::ready) {
                DEBUG_OUT(
                    "[WARNING] TestLoop::call timed out after " << timeoutMs << "ms" << std::endl
                );
                throw std::runtime_error("runOnLoop timed out");
            }
            future.get();
        } else {
            if (future.wait_for(std::chrono::milliseconds(timeoutMs)) !=
                std::future_status::ready) {
                DEBUG_OUT(
                    "[WARNING] TestLoop::call timed out after " << timeoutMs << "ms" << std::endl
                );
                throw std::runtime_error("runOnLoop timed out");
            }
            return future.get();
        }
    }

    // Overload for simple async execution, returns a future with the result type of F
    template <typename F>
    auto callAsync(F&& fn) -> std::future<std::invoke_result_t<F>> {
        using ReturnT = std::invoke_result_t<F>;
        auto promise = std::make_shared<std::promise<ReturnT>>();
        auto future = promise->get_future();
        auto loop = this->loop;
        assert(loop != nullptr);

        loop->defer([fn = std::forward<F>(fn), promise]() mutable {
            try {
                if constexpr (std::is_void_v<ReturnT>) {
                    fn();
                    promise->set_value();
                } else {
                    promise->set_value(fn());
                }
            } catch (...) { promise->set_exception(std::current_exception()); }
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
    uint64_t tapId = 0;
    uint64_t id = 0;
    int64_t established = 0;
    std::string host = "";
    int port = 0;
    std::string source_host = "";
    int options = 0;
    TCPConnection<SSL, TestTCPConnection<SSL>*>* connection = nullptr;
    bool closed = false;
    EventBuffer localEvents{128};

    TestTCPConnection() = default;

    ~TestTCPConnection() {
        DEBUG_OUT("TestTCPConnection::~TestTCPConnection => " << toString());
        closed = true;
        // loopEvents->push(NetworkEvent(
        //     0,
        //     unixNanos(),
        //     id,
        //     this,
        //     NetworkEventType::TCPOnTestTCPConnectionDestroyed,
        //     0,
        //     std::string(),
        //     nullptr,
        //     0
        // ));
        // loop = nullptr;
        // connection = nullptr;

        // DEBUG_OUT("TestTCPConnection::~TestTCPConnection" << std::endl);
    }

    std::string toString() const {
        std::ostringstream oss;
        oss << "TestTCPConnection[" << (SSL ? "SSL" : "PLAIN") << "]{"
            << "id=" << id << ", established=" << established << ", isClient="
            << (connection
                    ? reinterpret_cast<TCPConnection<SSL, TestTCPConnection<SSL>>*>(connection)
                          ->getConnectionData()
                          ->isClient
                    : false)
            << ", host=" << host << ", port=" << port << ", source_host=" << source_host
            << ", options=" << options << ", closed=" << (closed ? "true" : "false")
            << ", connection=" << static_cast<const void*>(connection)
            << ", localEvents.size=" << localEvents.size()
            << ", localEvents.capacity=" << localEvents.capacity() << "}";
        return oss.str();
    }
};

template <bool SSL>
void setupBehavior(
    bool isServer, TestLoop* loop, TCPBehavior<SSL, TestTCPConnection<SSL>*>& behavior
) {
    behavior.onData = [loop = loop](
                          TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                          TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                          TestTCPConnection<SSL>** userData,
                          std::string_view data
                      ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
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
        behavior.onConnection = [loop = loop](
                                    TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                                    TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                                    TestTCPConnection<SSL>** userData
                                ) {
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK_EQ(*userData, nullptr);

            // For server, create a new TestTCPConnection when connection is accepted
            auto connection = new TestTCPConnection<SSL>();
            connection->connection = conn;
            connection->id = loop->getNextConnectionId();
            connection->loop = loop;
            connection->loopEvents = &loop->events;
            connection->established = unixNanos();

            // connection->tapId = loop->tap([connection](Event& event) {
            //     if (event.id == connection->id) {
            //         connection->localEvents.push(event);
            //     }
            //     return true;
            // });

            // Store the pointer in user data
            *userData = connection;

            loop->events.push(Event(
                loop->getNextSequence(),
                unixNanos(),
                connection->id,
                conn,
                EventType::TCPOnConnection,
                0,
                std::string(),
                nullptr,
                0
            ));
        };
    } else {
        behavior.onConnection = [loop = loop](
                                    TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                                    TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                                    TestTCPConnection<SSL>** userData
                                ) {
            CHECK(conn);
            CHECK(connData);
            CHECK(userData);
            CHECK(*userData);

            (*userData)->connection = conn;
            if ((*userData)->id == 0) {
                (*userData)->id = loop->getNextConnectionId();
            }
            loop->events.push(Event(
                loop->getNextSequence(),
                unixNanos(),
                *userData ? (*userData)->id : 0,
                conn,
                EventType::TCPOnConnection,
                0,
                std::string(),
                nullptr,
                0
            ));
        };
    }

    behavior.onDisconnected = [loop = loop](
                                  TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                                  TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                                  TestTCPConnection<SSL>** userData,
                                  int code,
                                  void* data
                              ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnDisconnected,
            0,
            std::string(),
            data,
            0
        ));
        // if (*userData) {
        //     loop->events.unregister((*userData)->tapId);
        //     DEBUG_OUT("deleting user data");
        //     delete *userData;
        //     *userData = nullptr;
        //     // *reinterpret_cast<TestTCPConnection<SSL>**>(conn->getUserData()) = nullptr;
        // }
    };

    behavior.onConnectError = [loop = loop](
                                  TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                                  TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                                  TestTCPConnection<SSL>** userData,
                                  int code,
                                  std::string_view message
                              ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnConnectError,
            0,
            std::string(message),
            nullptr,
            message.size()
        ));
        if (*userData) {
            // delete *userData;
            *userData = nullptr;
            // *reinterpret_cast<TestTCPConnection<SSL>**>(conn->getUserData()) = nullptr;
        }
    };

    behavior.onWritable = [loop = loop](
                              TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                              TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                              TestTCPConnection<SSL>** userData,
                              uintmax_t bytes
                          ) -> bool {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnWritable,
            0,
            std::string(),
            nullptr,
            bytes
        ));
        return true;
    };

    behavior.onDrain = [loop = loop](
                           TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                           TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                           TestTCPConnection<SSL>** userData
                       ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnDrain,
            0,
            std::string(),
            nullptr,
            0
        ));
    };

    behavior.onDropped = [loop = loop](
                             TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                             TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                             TestTCPConnection<SSL>** userData,
                             std::string_view data
                         ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnDropped,
            0,
            std::string(data),
            nullptr,
            0
        ));
    };

    behavior.onEnd = [loop = loop](
                         TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                         TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                         TestTCPConnection<SSL>** userData
                     ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnEnd,
            0,
            std::string(),
            nullptr,
            0
        ));
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

    behavior.onTimeout = [loop = loop](
                             TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                             TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                             TestTCPConnection<SSL>** userData
                         ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnTimeout,
            0,
            std::string(),
            nullptr,
            0
        ));
    };

    behavior.onLongTimeout = [loop = loop](
                                 TCPConnection<SSL, TestTCPConnection<SSL>*>* conn,
                                 TCPConnectionData<SSL, TestTCPConnection<SSL>*>* connData,
                                 TestTCPConnection<SSL>** userData
                             ) {
        CHECK(conn);
        CHECK(connData);
        CHECK(userData);
        CHECK(*userData);
        loop->events.push(Event(
            loop->getNextSequence(),
            unixNanos(),
            *userData ? (*userData)->id : 0,
            conn,
            EventType::TCPOnLongTimeout,
            0,
            std::string(),
            nullptr,
            0
        ));
    };

    if (isServer) {
        behavior.onServerName = [loop = loop](
                                    TCPContext<SSL, TestTCPConnection<SSL>*>* context,
                                    std::string_view hostname
                                ) {
            CHECK(context);
            loop->events.push(Event(
                loop->getNextSequence(),
                unixNanos(),
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
    TestTCPServerApp(TestLoop* loop) : loop(loop), app(app) {
        app = loop->call([loop]() {
            TCPBehavior<SSL, TestTCPConnection<SSL>*> behavior;
            setupBehavior<SSL>(true, loop, behavior);
            return TemplatedTCPServerApp<SSL, TestTCPConnection<SSL>*>::listen(
                loop->loop, 0, std::move(behavior)
            );
        });

        port = app->getLocalPort();
    }

    ~TestTCPServerApp() {
        DEBUG_OUT("[1] TestTCPServerApp::~TestTCPServerApp");
        if (loop && app) {
            try {
                loop->call(
                    [this]() {
                        delete app;
                        app = nullptr;
                    },
                    1000
                ); // Short timeout
            } catch (const std::exception& e) {
                DEBUG_OUT("TestTCPServerApp::~TestTCPServerApp - execute failed: " << e.what());
                // If we can't execute on loop thread, still try to clean up
                delete app;
                app = nullptr;
            }
        }
        loop = nullptr;
        DEBUG_OUT("[2] TestTCPServerApp::~TestTCPServerApp");
    }
};

template <bool SSL>
struct TestTCPClientApp {
    TestLoop* loop;
    std::unique_ptr<TemplatedTCPClientApp<SSL, TestTCPConnection<SSL>*>> app;

    mutable std::mutex connectionsMutex;
    std::unordered_map<uint64_t, TestTCPConnection<SSL>*> connections;
    std::unordered_set<uint64_t> tapIds;

  private:
    struct PrivateTag {};

  public:
    TestTCPClientApp(TestLoop* loop) : loop(loop) {
        app = loop->call([loop]() {
            TCPBehavior<SSL, TestTCPConnection<SSL>*> behavior;
            setupBehavior<SSL>(false, loop, behavior);
            return std::make_unique<TemplatedTCPClientApp<SSL, TestTCPConnection<SSL>*>>(
                loop->loop, std::move(behavior)
            );
        });
    }

    ~TestTCPClientApp() {
        DEBUG_OUT("[1] TestTCPClientApp::~TestTCPClientApp");

        // Clean up connections and app on the loop thread
        if (loop) {
            // Clean up event handlers
            loop->call(
                [this]() {
                    for (auto& tapId : tapIds) {
                        loop->events.unregister(tapId);
                    }
                    for (auto& [id, connection] : connections) {
                        loop->events.unregister(connection->tapId);
                        if (connection && connection->connection) {
                            // reinterpret_cast<TCPConnection<SSL,
                            // TestTCPConnection<SSL>>*>(connection->connection)->close();
                        }
                    }
                    connections.clear();
                    // app = nullptr;
                },
                1000
            ); // Short timeout

            loop = nullptr;
        }

        DEBUG_OUT("[2] TestTCPClientApp::~TestTCPClientApp");
    }

    // Connect to a host asynchronously, returning a future to the TCPConnection pointer.
    std::future<TestTCPConnection<SSL>*>
    connectAsync(std::string host, int port, const std::string& source_host = "", int options = 0) {
        auto promise = std::make_shared<std::promise<TestTCPConnection<SSL>*>>();
        auto future = promise->get_future();

        // connect the client on loop
        loop->callAsync([this,
                         loop = loop,
                         promise = promise,
                         host = host,
                         port = port,
                         source_host = source_host,
                         options = options]() mutable {
            auto connId = loop->getNextConnectionId();
            TCPConnection<SSL, TestTCPConnection<SSL>*>* conn = nullptr;
            if (source_host.empty()) {
                conn = app->connect(host.c_str(), port, nullptr, options);
            } else {
                conn = app->connect(host.c_str(), port, source_host.c_str(), options);
            }

            DEBUG_OUT("connect result: " << static_cast<uintptr_t>((uintptr_t)(void*)conn));

            if (!conn) {
                loop->events.push(Event(
                    loop->getNextSequence(),
                    unixNanos(),
                    0,
                    nullptr,
                    EventType::TCPOnConnectError,
                    0,
                    std::string("connect return nullptr"),
                    nullptr,
                    0
                ));
                promise->set_value(nullptr);
                return;
            }

            // Create a new TestTCPConnection on the heap
            auto connection = new TestTCPConnection<SSL>();

            // Store the pointer in the user data
            *reinterpret_cast<TestTCPConnection<SSL>**>(conn->getUserData()) = connection;
            connection->id = connId;
            connection->host = host;
            connection->port = port;
            connection->source_host = source_host;
            connection->options = options;
            connection->loopEvents = &loop->events;
            connection->connection = conn;
            connection->tapId = loop->tap([this, connection](Event& event) {
                if (event.id == connection->id) {
                    connection->localEvents.push(event);

                    if (event.type == EventType::TCPOnDisconnected ||
                        event.type == EventType::TCPOnConnectError) {
                        std::lock_guard<std::mutex> lock(connectionsMutex);
                        connections.erase(connection->id);
                    }
                }
                return true;
            });

            {
                std::lock_guard<std::mutex> lock(connectionsMutex);
                connections[connection->id] = connection;
            }

            if (connection->established > 0) {
                promise->set_value(connection);
                return;
            }

            loop->events.push(Event(
                loop->getNextSequence(),
                unixNanos(),
                connection->id,
                conn,
                EventType::TCPOnConnecting,
                0,
                std::string(),
                nullptr,
                0
            ));

            // first event should determine if the connection is established or not
            tapIds.insert(
                loop->tap([this, connection = connection, promise = promise](Event& event) {
                    // ignore events for other connections
                    if (event.id != connection->id) {
                        return true;
                    }

                    if (event.type == EventType::TCPOnConnecting) {
                        return true;
                    }

                    // set the connection established time
                    if (event.type == EventType::TCPOnConnection) {
                        connection->established = unixNanos();

                        // complete the promise
                        promise->set_value(connection);
                    } else if (event.type == EventType::TCPOnConnectError) {
                        promise->set_exception(
                            std::make_exception_ptr(std::runtime_error("connect error"))
                        );
                    } else {
                        promise->set_exception(
                            std::make_exception_ptr(std::runtime_error("unexpected connect error"))
                        );
                    }

                    // unregister the event handler
                    return false;
                })
            );
        });

        return future;
    }

    // Connect to a host synchronously, blocking until connection is established or timeoutMs
    // expires.
    TestTCPConnection<SSL>* connect(
        std::string host, int port, std::string source_host = "", int options = 0,
        int timeoutMs = 5000
    ) {
        auto fut = connectAsync(host, port, source_host, options);
        if (fut.wait_for(std::chrono::milliseconds(timeoutMs)) == std::future_status::ready) {
            return fut.get();
        }
        return nullptr;
    }
};

struct TestFixture {
    TestLoop* loop;

    TestFixture() {
        loop = TestLoop::create();
    }

    ~TestFixture() {
        DEBUG_OUT("Cleaning up loop");
        delete loop;
    }
};

TEST_CASE("Test Loop creation") {
    // Try to create a loop directly
    auto loop = TestLoop::create();
    CHECK(loop != nullptr);
}

TEST_CASE("Test NetworkEventBuffer") {
    EventBuffer buffer(16);
    CHECK_EQ(buffer.empty(), true);
    CHECK_EQ(buffer.size(), 0);

    Event event;
    event.timestamp = 12345;
    event.connection = nullptr;
    event.type = EventType::TCPOnData;
    event.code = 0;
    event.data = "test";
    event.data_ptr = nullptr;
    event.bytes = 4;

    CHECK(buffer.push(std::move(event)));
    CHECK(!buffer.empty());
    CHECK(buffer.size() == 1);

    Event received;
    CHECK_EQ(buffer.pop(received), true);
    DEBUG_OUT("received: " << received.to_string() << std::endl);
    CHECK_EQ(received.timestamp, 12345);
    CHECK_EQ(received.connection, nullptr);
    CHECK_EQ(received.type, EventType::TCPOnData);
    CHECK_EQ(received.code, 0);
    CHECK_EQ(received.data, "test");
    CHECK_EQ(received.data_ptr, nullptr);
    CHECK_EQ(received.bytes, 4);
    CHECK(buffer.empty());
}

TEST_CASE("sizeof TCPConnectionData") {
    constexpr size_t size = getTCPConnectionDataSize<true, void*>();
    // constexpr size_t size = getTCPConnectionDataSize<true, TestTCPConnection<true>>();
    DEBUG_OUT("sizeof TCPConnectionData: " << size << std::endl);
}

TEST_CASE_FIXTURE(TestFixture, "Test Client and Server") {
    auto server = std::make_unique<TestTCPServerApp<false>>(loop);
    DEBUG_OUT("server: " << server->port << std::endl);
    auto client = std::make_unique<TestTCPClientApp<false>>(loop);
    auto clientConn = client->connect("127.0.0.1", server->port);
    CHECK(clientConn != nullptr);
}
