/*
 * io_uring eventing header for uSockets
 * Complete replacement for epoll/kqueue with enhanced performance
 */

#ifndef US_INTERNAL_IO_URING_H
#define US_INTERNAL_IO_URING_H

#ifdef LIBUS_USE_IO_URING

#include <stdint.h>
#include <sys/poll.h>
#include <signal.h>
#include "liburing.h"

/* Socket event constants for compatibility */
#define LIBUS_SOCKET_READABLE 1
#define LIBUS_SOCKET_WRITABLE 2

/* Forward declarations */
struct us_loop_t;
struct us_poll_t;
struct us_timer_t;
struct us_internal_async;

/* Include loop data structure definition */
#include "../loop_data.h"

/* io_uring loop structure - must match epoll_kqueue.h interface */
struct us_loop_t {
    alignas(LIBUS_EXT_ALIGNMENT) struct us_internal_loop_data_t data;

    /* Number of non-fallthrough polls */
    int num_polls;

    /* Number of ready polls this iteration */
    int num_ready_polls;

    /* Current ready poll being processed */
    int current_ready_poll;

    /* Loop file descriptor (io_uring ring fd) */
    int fd;

    /* Ready polls array (compatibility with epoll/kqueue) */
    void *ready_polls;
};

/* Poll structure - must be compatible with epoll_kqueue.h */
struct us_poll_t {
    alignas(LIBUS_EXT_ALIGNMENT) struct {
        int fd;
        unsigned char poll_type;
    } state;
};

/* Internal callback structure for timers and asyncs */
struct us_internal_callback_t {
    struct us_poll_t p;
    struct us_loop_t *loop;
    void (*cb)(struct us_internal_callback_t *cb);
    int cb_expects_the_loop;
    int leave_poll_ready;
};

/* Async structure */
struct us_internal_async {
    struct us_internal_callback_t cb;
};

/* Timer structure */
struct us_timer_t {
    struct us_internal_callback_t cb;
};

/* Loop functions */
struct us_loop_t *us_create_loop(void *hint, void (*wakeup_cb)(struct us_loop_t *loop),
    void (*pre_cb)(struct us_loop_t *loop), void (*post_cb)(struct us_loop_t *loop), 
    unsigned int ext_size);
void us_loop_free(struct us_loop_t *loop);
void us_loop_run(struct us_loop_t *loop);

/* Poll functions */
struct us_poll_t *us_create_poll(struct us_loop_t *loop, int fallthrough, unsigned int ext_size);
void us_poll_free(struct us_poll_t *p, struct us_loop_t *loop);
void *us_poll_ext(struct us_poll_t *p);
void us_poll_init(struct us_poll_t *p, LIBUS_SOCKET_DESCRIPTOR fd, int poll_type);
void us_poll_start(struct us_poll_t *p, struct us_loop_t *loop, int events);
void us_poll_change(struct us_poll_t *p, struct us_loop_t *loop, int events);
void us_poll_stop(struct us_poll_t *p, struct us_loop_t *loop);
int us_poll_events(struct us_poll_t *p);
LIBUS_SOCKET_DESCRIPTOR us_poll_fd(struct us_poll_t *p);
struct us_poll_t *us_poll_resize(struct us_poll_t *p, struct us_loop_t *loop, unsigned int ext_size);

/* Timer functions */
struct us_timer_t *us_create_timer(struct us_loop_t *loop, int fallthrough, unsigned int ext_size);
void us_timer_set(struct us_timer_t *timer, void (*cb)(struct us_timer_t *t), int ms, int repeat_ms);
void us_timer_close(struct us_timer_t *t);
void *us_timer_ext(struct us_timer_t *timer);
struct us_loop_t *us_timer_loop(struct us_timer_t *t);

/* Async functions */
struct us_internal_async *us_internal_create_async(struct us_loop_t *loop, int fallthrough, unsigned int ext_size);
void us_internal_async_set(struct us_internal_async *a, void (*cb)(struct us_internal_async *));
void us_internal_async_wakeup(struct us_internal_async *a);
void us_internal_async_close(struct us_internal_async *a);

/* Internal functions */
unsigned int us_internal_accept_poll_event(struct us_poll_t *p);
int us_internal_poll_type(struct us_poll_t *p);
void us_internal_poll_set_type(struct us_poll_t *p, int poll_type);

/* Wakeup feature - integrate loop */
#define LIBUS_INTEGRATION_LOOP_ID 255
void us_loop_integrate(struct us_loop_t *loop);
void us_wakeup_loop(struct us_loop_t *loop);

#endif /* LIBUS_USE_IO_URING */

#endif /* US_INTERNAL_IO_URING_H */
