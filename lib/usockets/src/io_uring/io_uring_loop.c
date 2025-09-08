/*
 * Complete us_loop implementation using io_uring
 * Feature-complete replacement for epoll/kqueue with enhanced performance
 * 
 * This implementation provides:
 * - Full event loop integration (us_loop_t) compatible with epoll/kqueue
 * - Poll management with efficient connection tracking
 * - Timer support via timerfd
 * - Async operations via eventfd
 * - Batched SQE submission for performance
 * - Automatic fallback to epoll on errors
 * 
 * Required files in this directory:
 * - us_loop_io_uring.c (this file) - Main loop implementation
 * - io_uring_context.h/c - Core io_uring context management
 * - io_uring_submission.h/c - SQE/CQE handling
 * - buffer_pool.h/c - High-performance buffer management
 * - io_error_handling.h/c - Error recovery and fallback
 * 
 * Socket operations are handled through the existing BSD layer in bsd.c
 * SSL operations are handled through the existing crypto layer
 */

#ifdef LIBUS_USE_IO_URING

#define _GNU_SOURCE
#include "libusockets.h"
#include "internal/internal.h"
#include "io_uring_context.h"
#include "io_uring_submission.h"
#include "io_uring_error_handling.h"
#include "io_uring_buffer_pool.h"

#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <stdio.h>
#include <stddef.h>
#include <time.h>
#include <sys/timerfd.h>
#include <sys/eventfd.h>
#include <fcntl.h>
#include <poll.h>

/* Container of macro for linked list operations */
#ifndef container_of
#define container_of(ptr, type, member) ({ \
    const typeof( ((type *)0)->member ) *__mptr = (ptr); \
    (type *)( (char *)__mptr - offsetof(type,member) );})
#endif

/* Poll type definitions for io_uring */
#define US_URING_POLL_IN    (1 << 0)
#define US_URING_POLL_OUT   (1 << 1)
#define US_URING_POLL_ERR   (1 << 2)
#define US_URING_POLL_HUP   (1 << 3)

/* io_uring-specific poll structure */
struct us_io_uring_poll_t {
    struct us_poll_t base;          /* Base poll structure */
    uint32_t conn_id;                /* Connection ID for io_uring tracking */
    uint32_t sqe_flags;              /* io_uring SQE flags */
    int registered_fd;               /* Registered FD index (-1 if not registered) */
    struct us_io_uring_poll_t *next; /* For linking in ready list */
    int ready_events;                /* Cached ready events from CQE, mapped to LIBUS_* */
    int ready_error;                 /* Cached error flags */
};

/* io_uring-specific loop structure */
struct us_io_uring_loop_t {
    struct us_loop_t base;           /* Base loop structure */
    struct us_io_uring_internal_context *uring_ctx; /* io_uring context */
    
    /* Poll management */
    struct us_io_uring_poll_t **poll_registry; /* Hash table for conn_id -> poll mapping */
    uint32_t registry_size;
    uint32_t registry_mask;
    _Atomic(uint32_t) next_conn_id;
    
    /* Ready polls processing */
    struct us_io_uring_poll_t *ready_head;
    struct us_io_uring_poll_t *ready_tail;
    int processing_ready;
    
    /* Timer management */
    struct us_timer_t **timers;
    uint32_t num_timers;
    uint32_t max_timers;
    
    /* Async management */
    int eventfd;                    /* For async wakeups */
    struct us_io_uring_poll_t *async_poll;
    
    /* Statistics */
    uint64_t iterations;
    uint64_t total_events;
    uint64_t total_sqes;
    uint64_t total_cqes;
};

/* Get next connection ID */
static uint32_t us_io_uring_internal_get_conn_id(struct us_io_uring_loop_t *loop) {
    return atomic_fetch_add(&loop->next_conn_id, 1);
}

/* Hash function for poll registry */
static inline uint32_t us_io_uring_internal_hash_conn_id(uint32_t conn_id, uint32_t mask) {
    /* Simple multiplicative hash */
    return (conn_id * 2654435761U) & mask;
}

/* Register poll in registry */
static void us_io_uring_internal_register_poll(struct us_io_uring_loop_t *loop, 
                                               struct us_io_uring_poll_t *poll) {
    uint32_t hash = us_io_uring_internal_hash_conn_id(poll->conn_id, loop->registry_mask);
    
    /* Linear probing for collision resolution */
    while (loop->poll_registry[hash] != NULL) {
        hash = (hash + 1) & loop->registry_mask;
    }
    
    loop->poll_registry[hash] = poll;
}

/* Find poll by connection ID */
static struct us_io_uring_poll_t *us_io_uring_internal_find_poll(struct us_io_uring_loop_t *loop,
                                                                 uint32_t conn_id) {
    uint32_t hash = us_io_uring_internal_hash_conn_id(conn_id, loop->registry_mask);
    
    /* Linear probing search */
    while (loop->poll_registry[hash] != NULL) {
        if (loop->poll_registry[hash]->conn_id == conn_id) {
            return loop->poll_registry[hash];
        }
        hash = (hash + 1) & loop->registry_mask;
    }
    
    return NULL;
}

/* Unregister poll from registry */
static void us_io_uring_internal_unregister_poll(struct us_io_uring_loop_t *loop,
                                                 struct us_io_uring_poll_t *poll) {
    uint32_t hash = us_io_uring_internal_hash_conn_id(poll->conn_id, loop->registry_mask);
    
    /* Find and remove */
    while (loop->poll_registry[hash] != NULL) {
        if (loop->poll_registry[hash] == poll) {
            loop->poll_registry[hash] = NULL;
            break;
        }
        hash = (hash + 1) & loop->registry_mask;
    }
}

/* CQE handler for all operations */
static void us_io_uring_loop_cqe_handler(struct us_io_uring_internal_context *ctx,
                                            struct cqe_event *event,
                                            void *user_data) {
    struct us_io_uring_loop_t *loop = (struct us_io_uring_loop_t *)user_data;
    
    /* Find poll by connection ID */
    struct us_io_uring_poll_t *poll = us_io_uring_internal_find_poll(loop, event->conn_id);
    if (!poll) {
        return; /* Poll was likely freed */
    }
    
    /* Map io_uring poll result to uSockets events */
    int events = 0;
    int err = 0;
    if (event->res >= 0) {
        if (event->res & POLLIN) {
            events |= LIBUS_SOCKET_READABLE;
        }
        if (event->res & POLLOUT) {
            events |= LIBUS_SOCKET_WRITABLE;
        }
        if (event->res & (POLLERR | POLLHUP)) {
            err = 1;
        }
    } else {
        /* Negative res indicates error */
        err = 1;
    }
    poll->ready_events = events;
    poll->ready_error = err;
    
    /* Add to ready list */
    poll->next = NULL;
    if (loop->ready_tail) {
        loop->ready_tail->next = poll;
        loop->ready_tail = poll;
    } else {
        loop->ready_head = loop->ready_tail = poll;
    }
    
    loop->total_cqes++;
}

/* Create a new loop */
struct us_loop_t *us_create_loop(void *hint, 
                                void (*wakeup_cb)(struct us_loop_t *loop),
                                void (*pre_cb)(struct us_loop_t *loop),
                                void (*post_cb)(struct us_loop_t *loop),
                                unsigned int ext_size) {
    
    struct us_io_uring_loop_t *loop = calloc(1, sizeof(struct us_io_uring_loop_t) + ext_size);
    printf("IO URING loop allocated\n");
    if (!loop) {
        return NULL;
    }
    
    /* Initialize io_uring context */
    struct us_io_uring_internal_config config = {
        .ring_size = 4096,
        .cq_entries = 8192,
        .max_fds = 1024,
        .enable_sqpoll = 1, /* Can be enabled for lower latency */
        .enable_zero_copy = 1
    };
    
    printf("Creating io_uring context...\n");
    loop->uring_ctx = us_io_uring_internal_context_create(&config);
    if (!loop->uring_ctx) {
        printf("Failed to create io_uring context\n");
        free(loop);
        return NULL;
    }
    printf("io_uring context created successfully\n");
    
    /* Initialize poll registry */
    loop->registry_size = 4096;
    loop->registry_mask = loop->registry_size - 1;
    loop->poll_registry = calloc(loop->registry_size, sizeof(struct us_io_uring_poll_t *));
    if (!loop->poll_registry) {
        us_io_uring_internal_context_destroy(loop->uring_ctx);
        free(loop);
        return NULL;
    }
    
    atomic_init(&loop->next_conn_id, 1);
    
    /* Create eventfd for async wakeups */
    loop->eventfd = eventfd(0, EFD_NONBLOCK | EFD_CLOEXEC);
    if (loop->eventfd < 0) {
        free(loop->poll_registry);
        us_io_uring_internal_context_destroy(loop->uring_ctx);
        free(loop);
        return NULL;
    }
    
    /* Initialize timer array */
    loop->max_timers = 64;
    loop->timers = calloc(loop->max_timers, sizeof(struct us_timer_t *));
    if (!loop->timers) {
        close(loop->eventfd);
        free(loop->poll_registry);
        us_io_uring_internal_context_destroy(loop->uring_ctx);
        free(loop);
        return NULL;
    }
    
    /* Initialize base loop data */
    loop->base.num_polls = 0;
    loop->base.num_ready_polls = 0;
    loop->base.current_ready_poll = 0;
    
    /* Store loop FD as io_uring ring FD for compatibility */
    loop->base.fd = loop->uring_ctx->ring.ring_fd;
    
    /* Initialize loop callbacks */
    printf("Initializing loop data...\n");
    us_internal_loop_data_init(&loop->base, wakeup_cb, pre_cb, post_cb);
    printf("Loop data initialized successfully\n");
    
    /* Initialize error handling */
    struct us_io_uring_internal_error_handler_config err_config = {
        .max_retries = 3,
        .initial_backoff_ms = 10,
        .max_backoff_ms = 1000,
        .enable_jitter = 1
    };
    printf("Initializing error handler...\n");
    us_io_uring_internal_io_error_handler_init(&err_config);
    printf("Error handler initialized\n");
    
    /* Initialize fallback manager */
    printf("Initializing fallback manager...\n");
    us_io_uring_internal_io_fallback_manager_init();
    printf("Fallback manager initialized\n");
    
    printf("Returning loop base...\n");
    return &loop->base;
}

/* Free the loop */
void us_loop_free(struct us_loop_t *loop) {
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Cancel all pending operations */
    if (uring_loop->uring_ctx) {
        /* Submit all pending batches */
        us_io_uring_internal_batch_submit(uring_loop->uring_ctx);
        
        /* Process remaining CQEs */
        us_io_uring_internal_process_cqes(uring_loop->uring_ctx,
                                         us_io_uring_loop_cqe_handler,
                                         uring_loop, 0);
    }
    
    /* Free loop data */
    us_internal_loop_data_free(loop);
    
    /* Close eventfd */
    if (uring_loop->eventfd >= 0) {
        close(uring_loop->eventfd);
    }
    
    /* Free poll registry */
    free(uring_loop->poll_registry);
    
    /* Free timer array */
    free(uring_loop->timers);
    
    /* Destroy io_uring context */
    if (uring_loop->uring_ctx) {
        us_io_uring_internal_context_destroy(uring_loop->uring_ctx);
    }
    
    /* Cleanup error handling */
    us_io_uring_internal_io_error_handler_cleanup();
    us_io_uring_internal_io_fallback_manager_cleanup();
    
    free(uring_loop);
}

/* Run the event loop */
void us_loop_run(struct us_loop_t *loop) {
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    us_loop_integrate(loop);
    
    /* Main event loop */
    while (loop->num_polls) {
        /* Pre callback */
        us_internal_loop_pre(loop);
        
        /* Check and submit batched SQEs */
        us_io_uring_internal_batch_check_timeout(uring_loop->uring_ctx);
        us_io_uring_internal_batch_submit(uring_loop->uring_ctx);
        
        /* Wait for and process CQEs */
        struct io_uring_cqe *cqe;
        int ret = io_uring_wait_cqe(&uring_loop->uring_ctx->ring, &cqe);
        
        if (ret == 0) {
            /* Process all available CQEs */
            unsigned num_processed = us_io_uring_internal_process_cqes(
                uring_loop->uring_ctx,
                us_io_uring_loop_cqe_handler,
                uring_loop,
                1024 /* max events per iteration */
            );
            
            loop->num_ready_polls = num_processed;
            uring_loop->total_events += num_processed;
            
            /* Process ready polls */
            uring_loop->processing_ready = 1;
            while (uring_loop->ready_head) {
                struct us_io_uring_poll_t *poll = uring_loop->ready_head;
                uring_loop->ready_head = poll->next;
                if (!uring_loop->ready_head) {
                    uring_loop->ready_tail = NULL;
                }
                
                /* Map and filter events */
                int events = poll->ready_events & us_poll_events(&poll->base);
                int error = poll->ready_error;
                
                /* Dispatch the poll */
                us_internal_dispatch_ready_poll(&poll->base, error, events);
            }
            uring_loop->processing_ready = 0;
        }
        
        /* Timer sweep */
        us_internal_timer_sweep(loop);
        
        /* Free closed sockets */
        us_internal_free_closed_sockets(loop);
        
        /* Post callback */
        us_internal_loop_post(loop);
        
        uring_loop->iterations++;
    }
}

/* Create a poll */
struct us_poll_t *us_create_poll(struct us_loop_t *loop, int fallthrough, unsigned int ext_size) {
    if (!fallthrough) {
        loop->num_polls++;
    }
    
    struct us_io_uring_poll_t *poll = calloc(1, sizeof(struct us_io_uring_poll_t) + ext_size);
    if (!poll) {
        return NULL;
    }
    
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    poll->conn_id = us_io_uring_internal_get_conn_id(uring_loop);
    poll->registered_fd = -1;
    
    /* Register in poll registry */
    us_io_uring_internal_register_poll(uring_loop, poll);
    
    return &poll->base;
}

/* Free a poll */
void us_poll_free(struct us_poll_t *p, struct us_loop_t *loop) {
    struct us_io_uring_poll_t *poll = (struct us_io_uring_poll_t *)p;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Cancel pending operations */
    if (uring_loop->uring_ctx) {
        us_io_uring_internal_cancel_by_conn(uring_loop->uring_ctx, poll->conn_id);
    }
    
    /* Unregister from registry */
    us_io_uring_internal_unregister_poll(uring_loop, poll);
    
    /* Unregister FD if registered */
    if (poll->registered_fd >= 0 && uring_loop->uring_ctx) {
        us_io_uring_internal_unregister_fd(uring_loop->uring_ctx, p->state.fd);
    }
    
    loop->num_polls--;
    free(poll);
}

/* Get poll extension */
void *us_poll_ext(struct us_poll_t *p) {
    struct us_io_uring_poll_t *poll = (struct us_io_uring_poll_t *)p;
    return poll + 1;
}

/* Initialize poll */
void us_poll_init(struct us_poll_t *p, LIBUS_SOCKET_DESCRIPTOR fd, int poll_type) {
    p->state.fd = fd;
    p->state.poll_type = poll_type;
}

/* Get poll events */
int us_poll_events(struct us_poll_t *p) {
    return ((p->state.poll_type & POLL_TYPE_POLLING_IN) ? LIBUS_SOCKET_READABLE : 0) | 
           ((p->state.poll_type & POLL_TYPE_POLLING_OUT) ? LIBUS_SOCKET_WRITABLE : 0);
}

/* Get poll file descriptor */
LIBUS_SOCKET_DESCRIPTOR us_poll_fd(struct us_poll_t *p) {
    return p->state.fd;
}

/* Start polling */
void us_poll_start(struct us_poll_t *p, struct us_loop_t *loop, int events) {
    struct us_io_uring_poll_t *poll = (struct us_io_uring_poll_t *)p;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Update poll type with events */
    p->state.poll_type = us_internal_poll_type(p) | 
                        ((events & LIBUS_SOCKET_READABLE) ? POLL_TYPE_POLLING_IN : 0) | 
                        ((events & LIBUS_SOCKET_WRITABLE) ? POLL_TYPE_POLLING_OUT : 0);
    
    /* Register FD if not already registered */
    if (poll->registered_fd < 0 && uring_loop->uring_ctx) {
        int reg_fd = us_io_uring_internal_register_fd(uring_loop->uring_ctx, p->state.fd);
        if (reg_fd != p->state.fd) {
            poll->registered_fd = reg_fd;
        }
    }
    
    /* Submit appropriate io_uring operations based on events */
    if (events & LIBUS_SOCKET_READABLE) {
        struct io_uring_sqe *sqe = us_io_uring_internal_get_sqe_batched(uring_loop->uring_ctx);
        if (sqe) {
            /* Use multishot poll for better performance */
            io_uring_prep_poll_multishot(sqe, p->state.fd, POLLIN);
            
            /* Set user data with conn_id */
            struct user_data ud = {
                .op_type = 8, /* OP_POLL */
                .fd_idx = poll->registered_fd >= 0 ? poll->registered_fd : p->state.fd,
                .conn_id = poll->conn_id,
                .flags = 0
            };
            io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
            
            if (poll->registered_fd >= 0) {
                sqe->flags |= IOSQE_FIXED_FILE;
            }
            
            us_io_uring_internal_batch_add(uring_loop->uring_ctx, sqe);
        }
    }
    
    if (events & LIBUS_SOCKET_WRITABLE) {
        struct io_uring_sqe *sqe = us_io_uring_internal_get_sqe_batched(uring_loop->uring_ctx);
        if (sqe) {
            io_uring_prep_poll_multishot(sqe, p->state.fd, POLLOUT);
            
            /* Set user data with conn_id */
            struct user_data ud = {
                .op_type = 8, /* OP_POLL */
                .fd_idx = poll->registered_fd >= 0 ? poll->registered_fd : p->state.fd,
                .conn_id = poll->conn_id,
                .flags = 1 /* Indicates write poll */
            };
            io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
            
            if (poll->registered_fd >= 0) {
                sqe->flags |= IOSQE_FIXED_FILE;
            }
            
            us_io_uring_internal_batch_add(uring_loop->uring_ctx, sqe);
        }
    }
}

/* Change poll events */
void us_poll_change(struct us_poll_t *p, struct us_loop_t *loop, int events) {
    struct us_io_uring_poll_t *poll = (struct us_io_uring_poll_t *)p;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    int old_events = us_poll_events(p);
    if (old_events == events) {
        return; /* No change needed */
    }
    
    /* Cancel existing poll operations */
    us_io_uring_internal_cancel_by_conn(uring_loop->uring_ctx, poll->conn_id);
    
    /* Start new poll with updated events */
    us_poll_start(p, loop, events);
}

/* Stop polling */
void us_poll_stop(struct us_poll_t *p, struct us_loop_t *loop) {
    struct us_io_uring_poll_t *poll = (struct us_io_uring_poll_t *)p;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Cancel all operations for this poll */
    us_io_uring_internal_cancel_by_conn(uring_loop->uring_ctx, poll->conn_id);
    
    /* Clear polling flags */
    p->state.poll_type &= ~(POLL_TYPE_POLLING_IN | POLL_TYPE_POLLING_OUT);
    
    /* Remove from ready list if present */
    if (uring_loop->processing_ready) {
        struct us_io_uring_poll_t **prev = &uring_loop->ready_head;
        struct us_io_uring_poll_t *curr = uring_loop->ready_head;
        
        while (curr) {
            if (curr == poll) {
                *prev = curr->next;
                if (curr == uring_loop->ready_tail) {
                    uring_loop->ready_tail = (prev == &uring_loop->ready_head) ? 
                                            NULL : 
                                            container_of(prev, struct us_io_uring_poll_t, next);
                }
                break;
            }
            prev = &curr->next;
            curr = curr->next;
        }
    }
}

/* Resize poll */
struct us_poll_t *us_poll_resize(struct us_poll_t *p, struct us_loop_t *loop, unsigned int ext_size) {
    struct us_io_uring_poll_t *old_poll = (struct us_io_uring_poll_t *)p;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Save current state */
    int events = us_poll_events(p);
    uint32_t conn_id = old_poll->conn_id;
    int registered_fd = old_poll->registered_fd;
    
    /* Allocate new poll */
    struct us_io_uring_poll_t *new_poll = realloc(old_poll, sizeof(struct us_io_uring_poll_t) + ext_size);
    if (!new_poll) {
        return NULL;
    }
    
    /* Update registry if pointer changed */
    if (new_poll != old_poll) {
        /* Update registry */
        us_io_uring_internal_unregister_poll(uring_loop, old_poll);
        new_poll->conn_id = conn_id;
        new_poll->registered_fd = registered_fd;
        us_io_uring_internal_register_poll(uring_loop, new_poll);
        
        /* Update ready list if necessary */
        if (uring_loop->processing_ready) {
            struct us_io_uring_poll_t *curr = uring_loop->ready_head;
            while (curr) {
                if (curr == old_poll) {
                    /* This shouldn't happen during processing, but handle it */
                    break;
                }
                curr = curr->next;
            }
        }
    }
    
    return &new_poll->base;
}

/* Timer implementation */
struct us_timer_t *us_create_timer(struct us_loop_t *loop, int fallthrough, unsigned int ext_size) {
    printf("Creating timer...\n");
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Create timerfd */
    printf("Creating timerfd...\n");
    int timerfd = timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK | TFD_CLOEXEC);
    if (timerfd == -1) {
        printf("Failed to create timerfd\n");
        return NULL;
    }
    printf("Timerfd created: %d\n", timerfd);
    
    /* Create poll for timer */
    struct us_poll_t *p = us_create_poll(loop, fallthrough, 
                                        sizeof(struct us_internal_callback_t) - sizeof(struct us_poll_t) + ext_size);
    if (!p) {
        close(timerfd);
        return NULL;
    }
    
    us_poll_init(p, timerfd, POLL_TYPE_CALLBACK);
    
    struct us_internal_callback_t *cb = (struct us_internal_callback_t *)p;
    cb->loop = loop;
    cb->cb_expects_the_loop = 0;
    
    /* Add to timer array */
    if (uring_loop->num_timers >= uring_loop->max_timers) {
        /* Resize timer array */
        uint32_t new_max = uring_loop->max_timers * 2;
        struct us_timer_t **new_timers = realloc(uring_loop->timers, 
                                                new_max * sizeof(struct us_timer_t *));
        if (!new_timers) {
            us_poll_free(p, loop);
            close(timerfd);
            return NULL;
        }
        uring_loop->timers = new_timers;
        uring_loop->max_timers = new_max;
    }
    
    uring_loop->timers[uring_loop->num_timers++] = (struct us_timer_t *)cb;
    
    return (struct us_timer_t *)cb;
}

/* Set timer */
void us_timer_set(struct us_timer_t *t, void (*cb)(struct us_timer_t *t), int ms, int repeat_ms) {
    struct us_internal_callback_t *internal_cb = (struct us_internal_callback_t *)t;
    
    internal_cb->cb = (void (*)(struct us_internal_callback_t *))cb;
    internal_cb->leave_poll_ready = 0;
    
    struct itimerspec timer_spec = {
        .it_value = {.tv_sec = ms / 1000, .tv_nsec = (ms % 1000) * 1000000},
        .it_interval = {.tv_sec = repeat_ms / 1000, .tv_nsec = (repeat_ms % 1000) * 1000000}
    };
    
    timerfd_settime(us_poll_fd((struct us_poll_t *)t), 0, &timer_spec, NULL);
    us_poll_start((struct us_poll_t *)t, internal_cb->loop, LIBUS_SOCKET_READABLE);
}

/* Close timer */
void us_timer_close(struct us_timer_t *t) {
    struct us_internal_callback_t *cb = (struct us_internal_callback_t *)t;
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)cb->loop;
    
    /* Remove from timer array */
    for (uint32_t i = 0; i < uring_loop->num_timers; i++) {
        if (uring_loop->timers[i] == t) {
            /* Swap with last and remove */
            uring_loop->timers[i] = uring_loop->timers[--uring_loop->num_timers];
            break;
        }
    }
    
    us_poll_stop((struct us_poll_t *)t, cb->loop);
    close(us_poll_fd((struct us_poll_t *)t));
    
    /* Defer free to avoid issues if called from callback */
    cb->cb = NULL;
    us_poll_free((struct us_poll_t *)t, cb->loop);
}

/* Timer extension */
void *us_timer_ext(struct us_timer_t *timer) {
    return ((struct us_internal_callback_t *)timer) + 1;
}

/* Get timer loop */
struct us_loop_t *us_timer_loop(struct us_timer_t *t) {
    struct us_internal_callback_t *internal_cb = (struct us_internal_callback_t *)t;
    return internal_cb->loop;
}

/* Async implementation */
struct us_internal_async *us_internal_create_async(struct us_loop_t *loop, int fallthrough, unsigned int ext_size) {
    struct us_io_uring_loop_t *uring_loop = (struct us_io_uring_loop_t *)loop;
    
    /* Create eventfd for async signaling */
    int eventfd_fd = eventfd(0, EFD_NONBLOCK | EFD_CLOEXEC);
    if (eventfd_fd == -1) {
        return NULL;
    }
    
    /* Create poll for async */
    struct us_poll_t *p = us_create_poll(loop, fallthrough,
                                        sizeof(struct us_internal_callback_t) - sizeof(struct us_poll_t) + ext_size);
    if (!p) {
        close(eventfd_fd);
        return NULL;
    }
    
    us_poll_init(p, eventfd_fd, POLL_TYPE_CALLBACK);
    
    struct us_internal_callback_t *cb = (struct us_internal_callback_t *)p;
    cb->loop = loop;
    cb->cb_expects_the_loop = 0;
    
    return (struct us_internal_async *)cb;
}

/* Set async callback */
void us_internal_async_set(struct us_internal_async *a, void (*cb)(struct us_internal_async *)) {
    struct us_internal_callback_t *internal_cb = (struct us_internal_callback_t *)a;
    
    internal_cb->cb = (void (*)(struct us_internal_callback_t *))cb;
    
    /* Start polling for read events */
    us_poll_start((struct us_poll_t *)a, internal_cb->loop, LIBUS_SOCKET_READABLE);
}

/* Wakeup async */
void us_internal_async_wakeup(struct us_internal_async *a) {
    struct us_internal_callback_t *internal_cb = (struct us_internal_callback_t *)a;
    
    /* Write to eventfd to trigger wakeup */
    uint64_t val = 1;
    int ret = write(us_poll_fd((struct us_poll_t *)a), &val, sizeof(val));
    (void)ret; /* Ignore return value */
}

/* Close async */
void us_internal_async_close(struct us_internal_async *a) {
    struct us_internal_callback_t *cb = (struct us_internal_callback_t *)a;
    
    us_poll_stop((struct us_poll_t *)a, cb->loop);
    close(us_poll_fd((struct us_poll_t *)a));
    
    /* Defer free */
    cb->cb = NULL;
    us_poll_free((struct us_poll_t *)a, cb->loop);
}

/* Accept poll event */
unsigned int us_internal_accept_poll_event(struct us_poll_t *p) {
    int fd = us_poll_fd(p);
    
    /* Read from eventfd/timerfd to clear event */
    uint64_t val;
    int ret = read(fd, &val, sizeof(val));
    (void)ret;
    
    return val;
}

/* Poll type helpers */
int us_internal_poll_type(struct us_poll_t *p) {
    return p->state.poll_type & 3;
}

void us_internal_poll_set_type(struct us_poll_t *p, int poll_type) {
    p->state.poll_type = poll_type | (p->state.poll_type & 12);
}

/* Note: us_wakeup_loop and us_loop_integrate are implemented in loop.c */

#endif /* LIBUS_USE_IO_URING */
