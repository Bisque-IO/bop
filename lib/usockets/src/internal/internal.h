/*
 * Authored by Alex Hultman, 2018-2019.
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

#ifndef INTERNAL_H
#define INTERNAL_H

// #if defined(_MSC_VER)
// #undef alignas
// #define alignas(x) __declspec(align(x))
// #else
// #include <stdalign.h>
// #endif

#if __has_include(<stdalign.h>)
#include <stdalign.h>
#endif
#if defined(alignas) || defined(__cplusplus)
#define LIBUS_ALIGNAS(N) alignas(N)
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#define LIBUS_ALIGNAS(N) _Alignas(N)
#elif defined(_MSC_VER)
#define LIBUS_ALIGNAS(N) __declspec(align(N))
#elif __has_attribute(__aligned__) || defined(__GNUC__)
#define LIBUS_ALIGNAS(N) __attribute__((__aligned__(N)))
#else
#error "FIXME: Required alignas() or equivalent."
#endif /* LIBUS_ALIGNAS */

/* We only have one networking implementation so far */
#include "internal/networking/bsd.h"

// WARNING: This MUST be included after bsd.h, otherwise sys/socket includes
// fail surprisingly.
#include <stdint.h>

#ifndef LIBUS_USE_IO_URING

/* We have many different eventing implementations */
#if defined(LIBUS_USE_EPOLL) || defined(LIBUS_USE_KQUEUE)
#include "internal/eventing/epoll_kqueue.h"
#endif
#ifdef LIBUS_USE_LIBUV
#include "internal/eventing/libuv.h"
#endif
#ifdef LIBUS_USE_GCD
#include "internal/eventing/gcd.h"
#endif
#ifdef LIBUS_USE_ASIO
#include "internal/eventing/asio.h"
#endif

/* Poll type and what it polls for */
enum {
    /* Two first bits */
    POLL_TYPE_SOCKET = 0,
    POLL_TYPE_SOCKET_SHUT_DOWN = 1,
    POLL_TYPE_SEMI_SOCKET = 2,
    POLL_TYPE_CALLBACK = 3,

    /* Two last bits */
    POLL_TYPE_POLLING_OUT = 4,
    POLL_TYPE_POLLING_IN = 8
};

/* Loop related */
void us_internal_dispatch_ready_poll(struct us_poll_t *p, int error, int events);
void us_internal_timer_sweep(struct us_loop_t *loop);
void us_internal_free_closed_sockets(struct us_loop_t *loop);
void us_internal_loop_link(struct us_loop_t *loop, struct us_socket_context_t *context);
void us_internal_loop_unlink(struct us_loop_t *loop, struct us_socket_context_t *context);
void us_internal_loop_data_init(struct us_loop_t *loop, void (*wakeup_cb)(struct us_loop_t *loop),
    void (*pre_cb)(struct us_loop_t *loop), void (*post_cb)(struct us_loop_t *loop));
void us_internal_loop_data_free(struct us_loop_t *loop);
void us_internal_loop_pre(struct us_loop_t *loop);
void us_internal_loop_post(struct us_loop_t *loop);

/* Asyncs (old) */
struct us_internal_async *us_internal_create_async(struct us_loop_t *loop, int fallthrough, unsigned int ext_size);
void us_internal_async_close(struct us_internal_async *a);
void us_internal_async_set(struct us_internal_async *a, void (*cb)(struct us_internal_async *));
void us_internal_async_wakeup(struct us_internal_async *a);

/* Eventing related */
unsigned int us_internal_accept_poll_event(struct us_poll_t *p);
int us_internal_poll_type(struct us_poll_t *p);
void us_internal_poll_set_type(struct us_poll_t *p, int poll_type);

/* SSL loop data */
void us_internal_init_loop_ssl_data(struct us_loop_t *loop);
void us_internal_free_loop_ssl_data(struct us_loop_t *loop);

/* Socket context related */
void us_internal_socket_context_link_socket(struct us_socket_context_t *context, struct us_socket_t *s);
void us_internal_socket_context_unlink_socket(struct us_socket_context_t *context, struct us_socket_t *s);

/* Sockets are polls */
struct us_socket_t {
    LIBUS_ALIGNAS(LIBUS_EXT_ALIGNMENT) struct us_poll_t p; // 4 bytes
    unsigned char timeout; // 1 byte
    unsigned char long_timeout; // 1 byte
    unsigned short low_prio_state; /* 0 = not in low-prio queue, 1 = is in low-prio queue, 2 = was in low-prio queue in this iteration */
    struct us_socket_context_t *context;
    struct us_socket_t *prev, *next;
};

/* Internal callback types are polls just like sockets */
struct us_internal_callback_t {
    LIBUS_ALIGNAS(LIBUS_EXT_ALIGNMENT) struct us_poll_t p;
    struct us_loop_t *loop;
    int cb_expects_the_loop;
    int leave_poll_ready;
    void (*cb)(struct us_internal_callback_t *cb);
};

/* Listen sockets are sockets */
struct us_listen_socket_t {
    LIBUS_ALIGNAS(LIBUS_EXT_ALIGNMENT) struct us_socket_t s;
    unsigned int socket_ext_size;
};

/* Listen sockets are kept in their own list */
void us_internal_socket_context_link_listen_socket(struct us_socket_context_t *context, struct us_listen_socket_t *s);
void us_internal_socket_context_unlink_listen_socket(struct us_socket_context_t *context, struct us_listen_socket_t *s);

struct us_socket_context_t {
    LIBUS_ALIGNAS(LIBUS_EXT_ALIGNMENT) struct us_loop_t *loop;
    uint32_t global_tick;
    unsigned char timestamp;
    unsigned char long_timestamp;
    struct us_socket_t *head_sockets;
    struct us_listen_socket_t *head_listen_sockets;
    struct us_socket_t *iterator;
    struct us_socket_context_t *prev, *next;

    LIBUS_SOCKET_DESCRIPTOR (*on_pre_open)(struct us_socket_context_t *context, LIBUS_SOCKET_DESCRIPTOR fd);
    struct us_socket_t *(*on_open)(struct us_socket_t *, int is_client, char *ip, int ip_length);
    struct us_socket_t *(*on_data)(struct us_socket_t *, char *data, int length);
    struct us_socket_t *(*on_writable)(struct us_socket_t *);
    struct us_socket_t *(*on_close)(struct us_socket_t *, int code, void *reason);
    //void (*on_timeout)(struct us_socket_context *);
    struct us_socket_t *(*on_socket_timeout)(struct us_socket_t *);
    struct us_socket_t *(*on_socket_long_timeout)(struct us_socket_t *);
    struct us_socket_t *(*on_end)(struct us_socket_t *);
    struct us_socket_t *(*on_connect_error)(struct us_socket_t *, int code);
    int (*is_low_prio)(struct us_socket_t *);
};

#endif

/* Internal SSL interface */
#ifndef LIBUS_NO_SSL

struct us_internal_ssl_socket_context_t;
struct us_internal_ssl_socket_t;

/* SNI functions */
void us_internal_ssl_socket_context_add_server_name(struct us_internal_ssl_socket_context_t *context, const char *hostname_pattern, struct us_socket_context_options_t options, void *user);
void us_internal_ssl_socket_context_remove_server_name(struct us_internal_ssl_socket_context_t *context, const char *hostname_pattern);
void us_internal_ssl_socket_context_on_server_name(struct us_internal_ssl_socket_context_t *context, void (*cb)(struct us_internal_ssl_socket_context_t *, const char *));
void *us_internal_ssl_socket_get_sni_userdata(struct us_internal_ssl_socket_t *s);
void *us_internal_ssl_socket_context_find_server_name_userdata(struct us_internal_ssl_socket_context_t *context, const char *hostname_pattern);

void *us_internal_ssl_socket_get_native_handle(struct us_internal_ssl_socket_t *s);
void *us_internal_ssl_socket_context_get_native_handle(struct us_internal_ssl_socket_context_t *context);

struct us_internal_ssl_socket_context_t *us_internal_create_ssl_socket_context(struct us_loop_t *loop,
    int context_ext_size, struct us_socket_context_options_t options);

void us_internal_ssl_socket_context_free(struct us_internal_ssl_socket_context_t *context);
void us_internal_ssl_socket_context_on_open(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_open)(struct us_internal_ssl_socket_t *s, int is_client, char *ip, int ip_length));

void us_internal_ssl_socket_context_on_close(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_close)(struct us_internal_ssl_socket_t *s, int code, void *reason));

void us_internal_ssl_socket_context_on_data(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_data)(struct us_internal_ssl_socket_t *s, char *data, int length));

void us_internal_ssl_socket_context_on_writable(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_writable)(struct us_internal_ssl_socket_t *s));

void us_internal_ssl_socket_context_on_timeout(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_timeout)(struct us_internal_ssl_socket_t *s));

void us_internal_ssl_socket_context_on_long_timeout(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_timeout)(struct us_internal_ssl_socket_t *s));

void us_internal_ssl_socket_context_on_end(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_end)(struct us_internal_ssl_socket_t *s));

void us_internal_ssl_socket_context_on_connect_error(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *(*on_connect_error)(struct us_internal_ssl_socket_t *s, int code));

struct us_listen_socket_t *us_internal_ssl_socket_context_listen(struct us_internal_ssl_socket_context_t *context,
    const char *host, int port, int options, int socket_ext_size);

struct us_listen_socket_t *us_internal_ssl_socket_context_listen_unix(struct us_internal_ssl_socket_context_t *context,
    const char *path, int options, int socket_ext_size);

struct us_internal_ssl_socket_t *us_internal_ssl_adopt_accepted_socket(struct us_internal_ssl_socket_context_t *context, LIBUS_SOCKET_DESCRIPTOR accepted_fd,
    unsigned int socket_ext_size, char *addr_ip, int addr_ip_length);

struct us_internal_ssl_socket_t *us_internal_ssl_socket_context_connect(struct us_internal_ssl_socket_context_t *context,
    const char *host, int port, const char *source_host, int options, int socket_ext_size);

    
struct us_internal_ssl_socket_t *us_internal_ssl_socket_context_connect_unix(struct us_internal_ssl_socket_context_t *context,
    const char *server_path, int options, int socket_ext_size);

int us_internal_ssl_socket_write(struct us_internal_ssl_socket_t *s, const char *data, int length, int msg_more);
void us_internal_ssl_socket_timeout(struct us_internal_ssl_socket_t *s, unsigned int seconds);
void *us_internal_ssl_socket_context_ext(struct us_internal_ssl_socket_context_t *s);
struct us_internal_ssl_socket_context_t *us_internal_ssl_socket_get_context(struct us_internal_ssl_socket_t *s);
void *us_internal_ssl_socket_ext(struct us_internal_ssl_socket_t *s);
int us_internal_ssl_socket_is_shut_down(struct us_internal_ssl_socket_t *s);
void us_internal_ssl_socket_shutdown(struct us_internal_ssl_socket_t *s);

struct us_internal_ssl_socket_t *us_internal_ssl_socket_context_adopt_socket(struct us_internal_ssl_socket_context_t *context,
    struct us_internal_ssl_socket_t *s, int ext_size);

struct us_internal_ssl_socket_context_t *us_internal_create_child_ssl_socket_context(struct us_internal_ssl_socket_context_t *context, int context_ext_size);
struct us_loop_t *us_internal_ssl_socket_context_loop(struct us_internal_ssl_socket_context_t *context);

#endif

#endif // INTERNAL_H
