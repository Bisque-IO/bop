// #ifndef BOP_ASIO_H
// #define BOP_ASIO_H

// #ifdef __cplusplus
// extern "C" {
// #endif

// #include "./lib.h"

// struct bop_asio_timer;

// struct bop_asio_loop {
//     ptr<Event_Loop> loop;
// };

// struct bop_asio_listener {
//     ptr<Listener> listener;
// };

// struct bop_asio_tcp_socket {
//     ptr<Connection> connection;
// };

// void bop_asio_loop_make() {
// }

// void bop_asio_loop_run() {
// }

// void bop_asio_loop_listener_make() {
// }

// struct bop_buffer_hdr {
//     u32 len;
//     u32 cap;
// };

// u32 bop_buffer_size(const bop_buffer_hdr *hdr) {
//     return hdr->len;
// }

// u32 bop_buffer_cap(const bop_buffer_hdr *hdr) {
//     return hdr->cap;
// }

// u8 *bop_buffer_data(bop_buffer_hdr *hdr) {
//     return reinterpret_cast<u8 *>(hdr) + sizeof(bop_buffer_hdr);
// }

// struct bop_asio_buffer {
//     void *data;
//     size_t size;
// };

// typedef void (*bop_asio_tcp_socket_next_read_buffer_fn)(bop_asio_buffer &buffer);

// typedef void (*bop_asio_tcp_socket_on_read_completed_fn)(i32 err, bop_asio_buffer &buffer);

// typedef void (*bop_asio_tcp_socket_on_closed_cb)();

// void bop_asio_tcp_socket_close() {
// }

// #ifdef __cplusplus
// }
// #endif

// #endif
