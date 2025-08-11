// #pragma once

// #include "./core.hpp"
// #include <cstddef>

// #ifndef _WIN32
// #include <sys/socket.h>
// #endif

// #include <asio.hpp>
// #include <asio/signal_set.hpp>
// #include <asio/ssl.hpp>

// #include <algorithm>
// #include <atomic>
// #include <chrono>
// #include <cstdlib>
// #include <memory>
// #include <mutex>
// #include <optional>
// #include <print>
// #include <string>
// #include <thread>
// #include <tuple>
// #include <unordered_map>
// #include <unordered_set>
// #include <vector>

// namespace bop::net {
//     using tcp_endpoint = asio::ip::tcp::endpoint;
//     using tcp_acceptor = asio::ip::tcp::acceptor;
//     using tcp_socket = asio::ip::tcp::socket;
//     using tcp_ssl_socket = asio::ssl::stream<tcp_socket>;
//     using ssl_context = asio::ssl::context;
//     using udp_endpoint = asio::ip::udp::endpoint;
//     using udp_socket = asio::ip::udp::socket;
//     using io_context = asio::io_context;
//     using any_io_executor = asio::any_io_executor;
//     using steady_timer = asio::steady_timer;
//     using time_point = std::chrono::steady_clock::time_point;

//     struct TCP_Socket {
//         static constexpr bool SSL = false;

//         explicit TCP_Socket(tcp_socket socket) : socket_(std::move(socket)) {
//         }

//         inline tcp_socket &socket() noexcept {
//             return socket_;
//         }

//     private:
//         tcp_socket socket_;
//     };

//     /// TCP_SSL_Socket
//     struct TCP_SSL_Socket {
//         static constexpr bool SSL = true;

//         explicit TCP_SSL_Socket(tcp_ssl_socket socket) : socket_(std::move(socket)) {
//         }

//         inline tcp_ssl_socket &socket() noexcept {
//             return socket_;
//         }

//     private:
//         tcp_ssl_socket socket_;
//     };

//     tcp_endpoint make_endpoint(const char *s, u16 port) {
//         return {asio::ip::make_address(s), port};
//     }

//     tcp_endpoint make_endpoint(std::string &s, u16 port) {
//         return {asio::ip::make_address(s), port};
//     }

//     tcp_endpoint make_endpoint(std::string_view &s, u16 port) {
//         return {asio::ip::make_address(s), port};
//     }

//     enum class Listener_Error_Code : u8 {
//         None = 0,
//         Open = 1,
//         Reuse_Address = 2,
//         Bind = 3,
//         Max_Listen_Connections = 4,
//         Need_SSL_Context = 5,
//     };

//     struct Listener_Error {
//         Listener_Error_Code code{};
//         std::string message;
//     };

//     struct const_buffer : asio::const_buffer {
//         const_buffer(const void *data, std::size_t size) : asio::const_buffer(data, size) {
//         }

//         const_buffer() : const_buffer(nullptr, 0) {
//         }
//     };

//     struct const_bytes : public const_buffer {
//         bytes buf;

//         explicit const_bytes(bytes buf) : const_buffer(buf.data(), buf.size()), buf(std::move(buf)) {
//         }

//         const_bytes() : const_bytes("") {
//         }
//     };

//     template<typename T>
//     struct const_buffer_owned : public const_buffer {
//         T owner;

//         const_buffer_owned(const void *data, std::size_t size, T owner)
//             : const_buffer(data, size),
//               owner(std::move(owner)) {
//         }
//     };

//     struct Event_Loop;
//     struct Event_Loops;
//     struct Listener;

//     /// Class to manage the memory to be used for handler-based custom allocation.
//     class Handler_Memory {
//     public:
//         Handler_Memory() = default;

//         Handler_Memory(const Handler_Memory &) = delete;

//         Handler_Memory &operator=(const Handler_Memory &) = delete;

//         void *allocate(std::size_t size) {
//             return new char[size];
//         }

//         void deallocate(char *pointer) {
//             delete[] (char *) pointer;
//         }
//     };

//     /// The allocator to be associated with the handler objects.
//     // template <typename T = char*>
//     class Handler_Allocator {
//     public:
//         using value_type = char;

//         Handler_Allocator() = default;

//         Handler_Allocator(Handler_Allocator &) noexcept = default;

//         Handler_Allocator(const Handler_Allocator &) noexcept = default;

//         value_type *allocate(std::size_t n) {
//             return static_cast<value_type *>(new char[sizeof(value_type) * n]);
//         }

//         void deallocate(value_type *p, std::size_t /*n*/) const {
//             delete[] p;
//         }
//     };

//     /// Connection is the base type-erased structure of a Connection CRTP
//     /// implementation.
//     struct Connection {
//         friend class Listener;

//         explicit Connection(Listener &listener, u64 id, io_context &ioc)
//             : listener_(listener),
//               id_(id),
//               ioc_(ioc) {
//         }

//         // inline bytes& buffer() noexcept {
//         //     return read_buffer_;
//         // }

//         milliseconds read_timeout() const noexcept {
//             return read_timeout_;
//         }

//         void set_read_timeout(milliseconds millis) noexcept {
//             read_timeout_ = millis;
//         }

//         template<typename Num>
//         void set_read_timeout(Num millis) noexcept {
//             read_timeout_ = milliseconds(i64(millis));
//         }

//         milliseconds write_timeout() const noexcept {
//             return write_timeout_;
//         }

//         void set_write_timeout(milliseconds millis) noexcept {
//             write_timeout_ = millis;
//         }

//         template<typename Num>
//         void set_write_timeout(Num millis) noexcept {
//             write_timeout_ = milliseconds(i64(millis));
//         }

//         virtual void start() = 0;

//         virtual void stop() = 0;

//         virtual ~Connection() {
//         }

//     protected:
//         io_context &ioc_;
//         Listener &listener_;

//         mutable std::mutex mu_{};
//         mutable std::mutex write_mu_{};

//         u64 id_;
//         time_point started_{std::chrono::steady_clock::now()};
//         Handler_Memory handler_memory_{};
//         u64 tick_;
//         milliseconds read_timeout_{}; // read timeout milliseconds
//         milliseconds write_timeout_{}; // write timeout milliseconds
//         u64 read_deadline_at_{std::numeric_limits<u64>::max()};
//         u64 write_deadline_at_{std::numeric_limits<u64>::max()};

//         bytes read_buffer_{}; // read buffer

//         std::vector<const_buffer> write_queue_{};
//         std::atomic<bool> write_inflight_{false};

//         std::atomic<u64> write_ops_{0};
//         std::atomic<u64> write_sending_{0};
//         std::atomic<u64> write_bytes_{0};
//         std::atomic<u64> read_ops_{0};
//         std::atomic<u64> read_bytes_{0};
//     };

//     using connection_set = std::unordered_set<ptr<Connection> >;

//     /// Connection_Events provides the interface of event methods for the
//     /// lifecycle of a Connection.
//     struct Connection_Traits {
//         /// on_init is called immediately after connection is established.
//         void on_init() {
//         }

//         /// on_closing is called when a close is triggered
//         void on_closing(error_code ec) {
//         }

//         /// on_closed is called after the socket is closed
//         void on_closed(error_code ec) {
//         }

//         /// on_before_handshake is called immediately before SSL handshake
//         /// initiated.
//         void on_before_handshake(ptr<ssl_context> &ssl_ctx) {
//         }

//         /// on_before_handshake is called immediately after SSL handshake complted.
//         /// If there was an error, the connection is closed.
//         void on_after_handshake(error_code ec, ptr<ssl_context> &ssl_ctx) {
//         }

//         /// on_read is called when new data arrives.
//         void on_read(bytes &buffer, usize length) {
//         }

//         void on_read_completed(error_code ec, void *buffer, usize length) {
//         }

//         /// on_read_timeout read deadline expired.
//         void on_read_timeout() {
//         }

//         void on_write_completed(error_code ec, size_t bytes_wrote, void *data, size_t length) {
//         }

//         void on_write_timeout() {
//         }

//         /// on_write_idle is called when there are no more writes pending and
//         /// there are no more writes in flight.
//         void on_write_idle() {
//         }

//         /// on_tick is called on each Event_Loop timer tick.
//         void on_tick(time_point time, u64 count) {
//         }
//     };

//     /// Connection_Base is the base struct of a CRTP based design.
//     /// Static dispatch is utilized for all methods in Connection_Events.
//     /// This includes hot path methods like "on_read".
//     template<class Handler>
//     struct Connection_Base : public Connection,
//                              public Connection_Traits {
//     public:
//         friend class Listener;

//         // friend class Listener_Base;
//         explicit Connection_Base(::bop::net::Listener &listener, u64 id, io_context &ioc)
//             : Connection(listener, id, ioc) {
//         }

//         Handler &derived() {
//             return static_cast<Handler &>(*this);
//         }

//         void stop() override {
//             std::lock_guard<std::mutex> lock(mu_);
//             // derived().socket().async_shutdown()
//             do_stop();
//         }

//         bool stopped() const;

//         bool enqueue_write(const_buffer buffer) noexcept {
//             std::lock_guard<std::mutex> lock(write_mu_);

//             // Do not write to queue if first.
//             if (write_queue_.empty() && !write_inflight_) {
//                 do_write(buffer);
//                 return true;
//             }

//             // Enqueue write.
//             write_queue_.push_back(buffer);

//             // Start writes if idle.
//             if (!write_inflight_) {
//                 write_inflight_.store(true);
//                 do_write();
//             }

//             return true;
//         }

//     BOOST_FORCEINLINE void do_tick(time_point time, u64 count) {
//             derived().on_tick(time, count);
//         }

//         void start() override {
//             do_init();
//         }

//     protected:
//         friend class Listener;

//         void do_init() {
//             derived().on_init();
//             // start reading
//             do_read();
//         }

//     BOOST_FORCEINLINE void write(const_buffer buffer) {
//             if (write_inflight_.load(std::memory_order_seq_cst)) {
//                 enqueue_write(buffer);
//             } else {
//                 write_inflight_.store(true, std::memory_order_seq_cst);
//                 do_write(buffer);
//             }
//         }

//         void do_stop() {
//         }

//     private:
//         void do_read() {
//             if (read_buffer_.empty()) {
//                 read_buffer_.resize(4096);
//             };
//             auto self = derived().shared_from_this();
//             do_read(std::move(self));
//         }

//         void do_read(ptr<Handler> self);

//         void do_write() {
//             if (write_queue_.empty()) {
//                 write_inflight_.store(false);
//                 write_deadline_at_ = std::numeric_limits<u64>::max();
//                 return;
//             }

//             auto self = derived().shared_from_this();
//             auto write_count = write_ops_++;

//             if (write_timeout_ < milliseconds(i64(1000))) {
//                 set_write_timeout(1000);
//             }

//             // Has more than 1 buffer?
//             if (write_queue_.size() > 1) {
//                 // Flush entire queue.
//                 std::vector<const_buffer> buffers;
//                 buffers.swap(write_queue_);

//                 u64 total_size = 0;
//                 for (auto &buffer: buffers) {
//                     total_size += buffer.size();
//                 }
//                 write_sending_ = total_size;

//                 // derived().socket().async_write(
//                 //   buffers,
//                 //   asio::bind_allocator(
//                 //     Handler_Allocator<int>(handler_memory_),
//                 //     [this, self](error_code ec, std::size_t length) {
//                 //       do_write_completed(ec, length);
//                 //     }
//                 //   )
//                 // );

//                 asio::async_write(
//                     derived().socket(),
//                     buffers,
//                     [this, self](error_code ec, std::size_t length) { do_write_completed(ec, length); }
//                 );
//             } else {
//                 auto buffer = std::move(write_queue_[0]);
//                 write_queue_.clear();
//                 write_sending_ = buffer.size();

//                 // derived().socket().async_write(
//                 //   buffer,
//                 //   asio::bind_allocator(
//                 //     Handler_Allocator<int>(handler_memory_),
//                 //     [this, self](error_code ec, std::size_t length) {
//                 //       do_write_completed(ec, length);
//                 //     }
//                 //   )
//                 // );

//                 asio::async_write(
//                     derived().socket(),
//                     buffer,
//                     [this, self](error_code ec, std::size_t length) { do_write_completed(ec, length); }
//                 );
//             }
//         }

//         void do_write(const_buffer buffer) {
//             write_inflight_.store(true);
//             write_sending_ = buffer.size();

//             auto self = derived().shared_from_this();
//             auto write_count = write_ops_++;

//             if (write_timeout_ < milliseconds(1000)) {
//                 set_write_timeout(1000);
//             }

//             // derived().socket().async_write(
//             //   std::move(buffer),
//             //   asio::bind_allocator(
//             //     Handler_Allocator<int>(handler_memory_),
//             //     [this, self](error_code ec, std::size_t length) {
//             //       do_write_completed(ec, length);
//             //     }
//             //   )
//             // );

//             asio::async_write(
//                 derived().socket(),
//                 buffer,
//                 [this, self](error_code ec, std::size_t length) { do_write_completed(ec, length); }
//             );
//         }

//         void do_write_completed(error_code ec, std::size_t length) {
//             write_sending_ = 0;

//             if (ec) {
//                 derived().on_write_error(ec);
//                 return;
//             }

//             write_bytes_ += length;
//             if (write_deadline_at_ >=
//                 std::chrono::high_resolution_clock::now().time_since_epoch().count()) {
//                 // expired
//                 derived().on_write_timeout();
//             }

//             std::lock_guard<std::mutex> lock(write_mu_);
//             do_write();
//         }
//     };

//     /**
//      *
//      */
//     struct Listener {
//     public:
//         friend class Event_Loop;

//         Listener(io_context &ioc, Event_Loop &loop, tcp_endpoint endpoint)
//             : Listener(ioc, loop, endpoint, nullptr) {
//         }

//         Listener(io_context &ioc, Event_Loop &loop, tcp_endpoint endpoint, ptr<ssl_context> ctx)
//             : ioc_(ioc),
//               loop_(loop),
//               acceptor_(ioc),
//               ssl_ctx_(std::move(ctx)),
//               endpoint_(std::move(endpoint)),
//               timer_(ioc) {
//         }

//         inline io_context &ioc() {
//             return ioc_;
//         }

//         virtual std::optional<Listener_Error> start() = 0;

//         virtual void stop(error_code &ec) = 0;

//         virtual void on_connection_closed(u64 id) = 0;

//     protected:
//         virtual void tick(time_point time, u64 count) = 0;

//         inline u64 next_conn_id() {
//             return ++connection_counter_;
//         }

//     private:
//         REMOVE_COPY_CAPABILITY(Listener);

//         REMOVE_MOVE_CAPABILITY(Listener);

//     protected:
//         mutable std::mutex mu_;
//         io_context &ioc_;
//         Event_Loop &loop_;
//         ptr<ssl_context> ssl_ctx_;
//         tcp_acceptor acceptor_;
//         tcp_endpoint endpoint_;
//         steady_timer timer_;
//         std::atomic<u64> connection_counter_{0};
//     };

//     template<typename Derived, typename Handler>
//     struct Listener_Base : public Listener {
//         friend class Event_Loop;
//         friend class Connection_Base<Handler>;

//         Listener_Base(io_context &ioc, Event_Loop &loop, tcp_endpoint endpoint, ptr<ssl_context> ctx)
//             : Listener(ioc, loop, endpoint, std::move(ctx)) {
//         }

//         Derived &derived() {
//             return static_cast<Derived &>(*this);
//         }

//         std::optional<Listener_Error> start() override {
//             error_code ec;

//             // Open the acceptor
//             std::ignore = acceptor_.open(endpoint_.protocol(), ec);
//             if (ec) {
//                 return Listener_Error{Listener_Error_Code::Open, "open"};
//             }

//             // Allow address reuse
//             std::ignore = acceptor_.set_option(asio::socket_base::reuse_address(true), ec);
//             if (ec) {
//                 return Listener_Error{Listener_Error_Code::Reuse_Address, "reuse_address"};
//             }

// #ifndef _WIN32
//         // Set SO_REUSEPORT on non windows platforms
//         int value{1};
//         setsockopt(acceptor_.native_handle(), SOL_SOCKET, SO_REUSEPORT, &value, sizeof(value));
// #endif

//             // Bind to the server address
//             std::ignore = acceptor_.bind(endpoint_, ec);
//             if (ec) {
//                 return Listener_Error{Listener_Error_Code::Bind, "bind"};
//             }

//             // Start listening for connections
//             std::ignore = acceptor_.listen(asio::socket_base::max_listen_connections, ec);
//             if (ec) {
//                 return Listener_Error{
//                     Listener_Error_Code::Max_Listen_Connections, "max_listen_connections"
//                 };
//             }

//             do_accept();

//             return std::nullopt;
//         }

//         void stop(error_code &err) override {
//             if (!acceptor_.is_open()) {
//                 return;
//             }
//         }

//         void on_connection_closed(u64 id) override {
//             std::println("remove connection {}", id);
//             connections_.erase(id);
//         }

//     protected:
//         void tick(time_point time, u64 count) override {
//             for (auto iter = connections_.begin(); iter != connections_.end(); ++iter) {
//                 iter->second->do_tick(time, count);
//             }
//         }

//     protected:
//         std::unordered_map<u64, ptr<Handler> > connections_;

//         void on_connection(u64 id, ptr<Handler> connection) {
//             connections_.insert_or_assign(id, connection);
//             std::println("on_connection: count {}", connections_.size());
//             connection->start();
//         }

//         void do_accept() {
//             if constexpr (std::is_convertible<Handler, TCP_SSL_Socket>::value) {
//                 acceptor_.async_accept(
//                     ioc_,
//                     [this, self = derived().shared_from_this()](error_code ec, tcp_socket socket) {
//                         if (ec) {
//                             return;
//                         }

//                         auto id = next_conn_id();

//                         auto connection =
//                                 make_ptr<Handler>(*this, id, tcp_ssl_socket(std::move(socket), *ssl_ctx_));
//                         connection->derived().on_before_handshake(ssl_ctx_);
//                         connection->socket().async_handshake(
//                             asio::ssl::stream_base::server,
//                             [id, this, self, session = std::move(connection)](const error_code ec) {
//                                 session->derived().on_after_handshake(ec, ssl_ctx_);
//                                 if (ec) {
//                                     return;
//                                 }
//                                 on_connection(id, session);
//                             }
//                         );

//                         do_accept();
//                     }
//                 );
//             } else {
//                 auto self = derived().shared_from_this();
//                 acceptor_.async_accept(ioc_, [this, self](error_code ec, tcp_socket socket) {
//                     if (ec) {
//                         return;
//                     }

//                     auto id = next_conn_id();

//                     on_connection(id, make_ptr<Handler>(*this, id, std::move(socket)));

//                     do_accept();
//                 });
//             }
//         }
//     };

//     template<typename T>
//     concept Listener_T = std::is_assignable_v<ptr<Listener>, ptr<T>>;

//     /// Event_Loop maps to exactly 1 io_context and 1 thread. This thread
//     /// drives the io_context.
//     struct Event_Loop : std::enable_shared_from_this<Event_Loop> {
//         Event_Loop(std::size_t id, Event_Loops &loops)
//             : id_(id),
//               loops_(loops),
//               tick_(0),
//               ioc_(1),
//               thread_(std::nullopt),
//               timer_(ioc_) {
//         }

//         ~Event_Loop() {
//             if (thread_) {
//             }
//         }

//         std::size_t id() const noexcept {
//             return id_;
//         }

//         io_context &ioc() noexcept {
//             return ioc_;
//         }

//     private:
//         REMOVE_COPY_CAPABILITY(Event_Loop);

//         REMOVE_MOVE_CAPABILITY(Event_Loop);

//         static constexpr std::chrono::milliseconds TIMER_CADENCE = milliseconds(2900);

//         void on_tick() {
//             auto now = steady_timer::time_point::clock::now();
//             tick_++;

//             std::lock_guard<std::mutex> lock(mu_);

//             for (auto iter = listeners_.begin(); iter != listeners_.end(); ++iter) {
//                 (*iter)->tick(now, tick_);
//             }
//         }

//         void schedule_timer() {
//             timer_.expires_after(milliseconds(timer_timeout_));
//             timer_.async_wait([this, self = shared_from_this()](error_code ec) {
//                 if (ec) {
//                     return;
//                 }
//                 try {
//                     on_tick();
//                 } catch (...) {
//                 }
//                 schedule_timer();
//             });
//         }

//     public:
//         void start() {
//             if (thread_) {
//                 return;
//             }

//             schedule_timer();

//             thread_ = std::thread{
//                 [this, self = shared_from_this()]() {
//                     ioc_.run();
//                 }
//             };
//         }

//         void add_listener(ptr<Listener> ln) {
//             std::lock_guard lock(mu_);
//             listeners_.insert(std::move(ln));
//         }

//         void remove_listener(ptr<Listener> &ln) {
//             std::lock_guard lock(mu_);
//             listeners_.erase(ln);
//         }

//         template<Listener_T Impl>
//         auto listen(
//             tcp_endpoint endpoint,
//             ptr<ssl_context> ssl_ctx = nullptr
//         ) -> expected<ptr<Listener>, Listener_Error> {
//             if constexpr (std::is_convertible_v<typename Impl::connection, TCP_SSL_Socket>) {
//                 if (ssl_ctx == nullptr) {
//                     return unexpected<Listener_Error>(
//                         Listener_Error{Listener_Error_Code::Need_SSL_Context, "SSL context was null"}
//                     );
//                 }
//             }

//             std::lock_guard lock(mu_);

//             // ensure we only have a single listener per tcp endpoint
//             for (auto& listener : listeners_) {
//                 if (listener->endpoint_ == endpoint) {
//                     return unexpected(Listener_Error{Listener_Error_Code::Bind, "bind"});
//                 }
//             }

//             // construct and start listening
//             auto listener = make_ptr<Impl>(ioc(), *this, endpoint);
//             auto err = listener->start();
//             if (err) {
//                 return unexpected<Listener_Error>(err.value());
//             }
//             return listener;
//         }

//     private:
//         mutable std::mutex mu_;
//         usize id_;
//         Event_Loops &loops_;
//         io_context ioc_;
//         std::optional<std::thread> thread_;
//         steady_timer timer_;
//         std::chrono::milliseconds timer_timeout_{TIMER_CADENCE};
//         u64 tick_;
//         std::unordered_set<ptr<Listener>> listeners_;
//     };

//     struct Event_Loop_Listener {
//         ptr<Event_Loop> event_loop;
//         ptr<Listener> listener;
//     };

//     struct Listener_Group : std::enable_shared_from_this<Listener_Group> {
//         std::vector<Event_Loop_Listener> listeners;
//     };

//     struct Event_Loops : std::enable_shared_from_this<Event_Loops> {
//         explicit Event_Loops(usize threads) : spawn_counter_(0) {
//             if (threads == 0) {
//                 threads = std::thread::hardware_concurrency();
//             } else if (threads > 8192) {
//                 threads = 8192;
//             }
//             for (std::size_t i = 0; i < threads; i++) {
//                 loops_.emplace_back(make_ptr<Event_Loop>(i, *this));
//             }
//             for (auto &loop: loops_) {
//                 loop->start();
//             }
//         }

//         ~Event_Loops() {
//         }

//         Event_Loop &next_loop() {
//             return *loops_[spawn_counter_++ % loops_.size()];
//         }

//     private:
//         REMOVE_COPY_CAPABILITY(Event_Loops);

//         REMOVE_MOVE_CAPABILITY(Event_Loops);

//     public:
//         template<Listener_T Impl>
//         auto listen(
//             tcp_endpoint endpoint,
//             ptr<ssl_context> ssl_ctx = nullptr
//         ) -> expected<ptr<Listener_Group>, Listener_Error> {
//             if constexpr (std::is_convertible_v<typename Impl::connection, TCP_SSL_Socket>) {
//                 if (ssl_ctx == nullptr) {
//                     return unexpected<Listener_Error>(
//                         Listener_Error{Listener_Error_Code::Need_SSL_Context, "SSL context was null"}
//                     );
//                 }
//             }
//             std::lock_guard<std::mutex> lock(mu_);

//             if (listeners_.find(endpoint) != listeners_.end()) {
//                 return unexpected<Listener_Error>(Listener_Error{Listener_Error_Code::Bind, "bind"});
//             }
//             auto ln_endpoint = make_ptr<Listener_Group>();
//             ln_endpoint->listeners.reserve(loops_.size());
//             for (auto &loop: loops_) {
//                 if constexpr (std::is_convertible<typename Impl::connection, TCP_SSL_Socket>::value) {
//                     ln_endpoint->listeners.push_back(
//                         {
//                             .event_loop = loop,
//                             .listener = make_ptr<Impl>(loop->ioc(), *loop, endpoint, ssl_ctx)
//                         }
//                     );
//                 } else {
//                     ln_endpoint->listeners.push_back(
//                         {.event_loop = loop, .listener = make_ptr<Impl>(loop->ioc(), *loop, endpoint)}
//                     );
//                 }
//             }

//             std::optional<Listener_Error> err = std::nullopt;

//             for (auto &loop: ln_endpoint->listeners) {
//                 err = loop.listener->start();
//                 if (err) {
//                     break;
//                 }
//                 loop.event_loop->add_listener(loop.listener);
//             }

//             if (err) {
//                 for (auto &loop: ln_endpoint->listeners) {
//                     error_code ec;
//                     loop.listener->stop(ec);
//                     loop.event_loop->remove_listener(loop.listener);
//                 }
//                 return unexpected<Listener_Error>(err.value());
//             }

//             listeners_[endpoint] = ln_endpoint;

//             return ln_endpoint;
//         }

//     private:
//         mutable std::mutex mu_;
//         usize spawn_counter_;
//         std::vector<ptr<Event_Loop> > loops_;

//         std::unordered_map<tcp_endpoint, ptr<Listener_Group>> listeners_;
//     };

//     template<class Handler>
//     void Connection_Base<Handler>::do_read(ptr<Handler> self) {
//         derived().socket().async_read_some(
//             asio::buffer(read_buffer_, read_buffer_.size()),
//             [this, self](error_code ec, std::size_t length) {
//                 if (ec) {
//                     if constexpr (std::is_convertible_v<Handler, TCP_SSL_Socket>) {
//                         derived().socket().async_shutdown([this, self](error_code ec2) {
//                         });
//                     } else {
//                         derived().socket().close(ec);
//                     }

//                     listener_.on_connection_closed(id_);
//                     return;
//                 }
//                 derived().on_read(read_buffer_, length);
//                 do_read(std::move(self));
//             }
//         );
//     }

// #define DEFINE_CONNECTION(name, impl)                                                              \
//     template <typename Derived>                                                                    \
//     struct name##_Connection : public ::bop::net::Connection_Base<Derived> {                       \
//         impl##impl_;                                                                               \
//         explicit name##_Connection(::bop::net::any_io_executor executor)                           \
//             : ::bop::net::Connection_Base<Derived>(std::move(executor)) {}                         \
//                                                                                                    \
//         Derived& derived() {                                                                       \
//             return static_cast<Derived&>(*this);                                                   \
//         }                                                                                          \
//     };

// #define BISQUE_NETWORK_CONNECTION_NAME(name) name##_Connection

// #define DEFINE_CONNECTION_INIT(name)                                                               \
//     explicit name##_Connection(                                                                    \
//         ::bop::net::Listener& listener, ::bop::u64 id, ::bop::net::io_context& ioc                 \
//     )                                                                                              \
//         : ::bop::net::Connection_Base<Derived>(listener, id, ioc) {}                               \
//                                                                                                    \
//     Derived& derived() {                                                                           \
//         return static_cast<Derived&>(*this);                                                       \
//     }

// #define BOP_NETWORK_HANDLER_TYPES(name)                                                            \
//     struct name##_Connection_Plain                                                                 \
//         : public ::bop::net::TCP_Socket,                                                           \
//           public name##_Connection<name##_Connection_Plain>,                                       \
//           public std::enable_shared_from_this<name##_Connection_Plain> {                           \
//         explicit name##_Connection_Plain(                                                          \
//             ::bop::net::Listener& ln, ::bop::u64 id, ::bop::net::tcp_socket socket                 \
//         )                                                                                          \
//             : ::bop::net::TCP_Socket(std::move(socket)),                                           \
//               name##_Connection<name##_Connection_Plain>(ln, id, ln.ioc()) {}                      \
//     };                                                                                             \
//                                                                                                    \
//     struct name##_Connection_SSL : public ::bop::net::TCP_SSL_Socket,                              \
//                                    public name##_Connection<name##_Connection_SSL>,                \
//                                    public std::enable_shared_from_this<name##_Connection_SSL> {    \
//         explicit name##_Connection_SSL(                                                            \
//             ::bop::net::Listener& ln, ::bop::u64 id, ::bop::net::tcp_ssl_socket socket             \
//         )                                                                                          \
//             : ::bop::net::TCP_SSL_Socket(std::move(socket)),                                       \
//               name##_Connection<name##_Connection_SSL>(ln, id, ln.ioc()) {}                        \
//     };                                                                                             \
//                                                                                                    \
//     struct name##_Listener                                                                         \
//         : public ::bop::net::Listener_Base<name##_Listener, name##_Connection_Plain>,              \
//           public std::enable_shared_from_this<name##_Listener> {                                   \
//         name##_Listener(                                                                           \
//             ::bop::net::io_context& ioc, ::bop::net::Event_Loop& loop,                             \
//             ::bop::net::tcp_endpoint endpoint                                                      \
//         )                                                                                          \
//             : ::bop::net::Listener_Base<name##_Listener, name##_Connection_Plain>(                 \
//                   ioc, loop, std::move(endpoint), nullptr                                          \
//               ) {}                                                                                 \
//         using connection = name##_Connection_Plain;                                                \
//     };                                                                                             \
//                                                                                                    \
//     struct name##_SSL_Listener                                                                     \
//         : public ::bop::net::Listener_Base<name##_SSL_Listener, name##_Connection_SSL>,            \
//           public std::enable_shared_from_this<name##_SSL_Listener> {                               \
//         name##_SSL_Listener(                                                                       \
//             ::bop::net::io_context& ioc, ::bop::net::Event_Loop& loop,                             \
//             ::bop::net::tcp_endpoint endpoint,                                                     \
//             ::bop::ptr<::bop::net::ssl_context> ssl_ctx                                            \
//         )                                                                                          \
//             : ::bop::net::Listener_Base<name##_SSL_Listener, name##_Connection_SSL>(               \
//                   ioc, loop, std::move(endpoint), std::move(ssl_ctx)                               \
//               ) {}                                                                                 \
//         using connection = name##_Connection_SSL;                                                  \
//     };

//     template<class Derived>
//     struct Echo_Connection : public Connection_Base<Derived> {
//         DEFINE_CONNECTION_INIT(Echo)
//     };

//     BOP_NETWORK_HANDLER_TYPES(Echo);
// } // namespace bisque::net
