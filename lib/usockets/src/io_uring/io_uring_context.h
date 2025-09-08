/*
 * io_uring context header for uSockets
 */

#ifndef IO_URING_CONTEXT_H
#define IO_URING_CONTEXT_H

#ifdef LIBUS_USE_IO_URING

#include <stdint.h>
#include <stdatomic.h>
#include <pthread.h>
#include <time.h>
#include "liburing.h"
#include "io_uring_buffer_pool.h"

/* Feature detection flags */
enum io_uring_features {
    URING_FEAT_SQPOLL           = (1 << 0),
    URING_FEAT_SQ_AFF           = (1 << 1),
    URING_FEAT_COOP_TASKRUN     = (1 << 2),
    URING_FEAT_SINGLE_ISSUER    = (1 << 3),
    URING_FEAT_MULTISHOT_ACCEPT = (1 << 4),
    URING_FEAT_BUFFER_SELECT    = (1 << 5),
    URING_FEAT_REGISTERED_FDS   = (1 << 6),
    URING_FEAT_REGISTERED_BUFS  = (1 << 7),
    URING_FEAT_FAST_POLL        = (1 << 8),
    URING_FEAT_RING_MSG         = (1 << 9),
};

/* Forward declarations */
struct us_io_uring_internal_context;
struct io_uring_sqe_batch;

/* Maximum batch size for SQE submission */
#define MAX_BATCH_SIZE 256

/* Error codes */
enum us_io_uring_internal_error {
    US_IO_URING_INTERNAL_OK = 0,
    US_IO_URING_INTERNAL_EAGAIN,
    US_IO_URING_INTERNAL_ENOMEM,
    US_IO_URING_INTERNAL_ENOBUFS,
    US_IO_URING_INTERNAL_ECANCELED,
    US_IO_URING_INTERNAL_ETIMEDOUT,
    US_IO_URING_INTERNAL_ECONNRESET,
    US_IO_URING_INTERNAL_EPIPE,
    US_IO_URING_INTERNAL_EINVAL,
    US_IO_URING_INTERNAL_ERROR_MAX
};

/* Simple bitmap structure */
struct us_io_uring_internal_bitmap {
    uint64_t *data;
    uint32_t size;
    uint32_t words;
};

/* File descriptor table for registered FDs */
struct us_io_uring_internal_fd_table {
    int *registered_fds;      /* Array of registered file descriptors */
    uint32_t *fd_map;         /* Map from FD to registered index */
    uint32_t capacity;        /* Total capacity */
    struct us_io_uring_internal_bitmap *free_slots; /* Bitmap of free slots */
};

/* SQE batching structure */
struct us_io_uring_internal_sqe_batch {
    struct io_uring_sqe **sqes; /* Array of SQE pointers */
    uint32_t count;             /* Current count in batch */
    uint64_t submit_ts;         /* Timestamp of last submit */
};

/* Statistics structure */
struct us_io_uring_internal_stats {
    /* Submission stats */
    _Atomic(uint64_t) sqes_submitted;
    _Atomic(uint64_t) sqes_dropped;
    _Atomic(uint64_t) batch_count;
    
    /* Completion stats */
    _Atomic(uint64_t) cqes_processed;
    _Atomic(uint64_t) cqes_overflow;
    
    /* Performance metrics */
    _Atomic(uint64_t) total_latency_ns;
    _Atomic(uint64_t) max_latency_ns;
    
    /* Buffer stats */
    _Atomic(uint64_t) buffer_exhaustion;
    _Atomic(uint64_t) zero_copy_sends;
    
    /* Error stats */
    _Atomic(uint64_t) errors[US_IO_URING_INTERNAL_ERROR_MAX];
    
    /* Buffer pool stats */
    struct us_io_uring_internal_buffer_pool_stats buffer_stats;
};

/* Configuration for io_uring context */
struct us_io_uring_internal_config {
    uint32_t ring_size;        /* Size of submission queue */
    uint32_t cq_entries;       /* Size of completion queue */
    uint32_t max_fds;          /* Maximum registered FDs */
    int enable_sqpoll;         /* Enable SQPOLL mode */
    int sq_thread_idle;        /* SQPOLL idle time in ms */
    int sq_cpu;                /* CPU for SQPOLL thread */
    int enable_huge_pages;     /* Use huge pages for buffers */
    int enable_zero_copy;      /* Enable zero-copy operations */
};

/* Main io_uring context structure */
struct us_io_uring_internal_context {
    struct io_uring ring;      /* io_uring instance */
    uint32_t features;         /* Available features bitmap */
    
    /* Buffer management */
    struct us_io_uring_internal_buffer_pool *small_pool;   /* 512B - 4KB buffers */
    struct us_io_uring_internal_buffer_pool *medium_pool;  /* 4KB - 16KB buffers */
    struct us_io_uring_internal_buffer_pool *large_pool;   /* 16KB - 64KB buffers */
    
    /* File descriptor management */
    struct us_io_uring_internal_fd_table fd_table;
    
    /* Submission batching */
    struct us_io_uring_internal_sqe_batch batch;
    uint64_t batch_timeout_ns;
    
    /* Completion handling */
    pthread_spinlock_t cqe_lock;
    
    /* Statistics */
    struct us_io_uring_internal_stats stats;
    
    /* Configuration */
    struct us_io_uring_internal_config config;
    
    /* CPU affinity for SQPOLL */
    int sq_cpu;
    
    /* Synchronization */
    pthread_mutex_t mutex;
    _Atomic(int) refs;
};

/* Context lifecycle management */
struct us_io_uring_internal_context *us_io_uring_internal_context_create(struct us_io_uring_internal_config *config);
void us_io_uring_internal_context_ref(struct us_io_uring_internal_context *ctx);
void us_io_uring_internal_context_unref(struct us_io_uring_internal_context *ctx);
void us_io_uring_internal_context_destroy(struct us_io_uring_internal_context *ctx);

/* File descriptor registration */
int us_io_uring_internal_register_fd(struct us_io_uring_internal_context *ctx, int fd);
void us_io_uring_internal_unregister_fd(struct us_io_uring_internal_context *ctx, int fd);

/* Statistics */
void us_io_uring_internal_context_get_stats(struct us_io_uring_internal_context *ctx, struct us_io_uring_internal_stats *stats);

/* Bitmap utilities */
struct us_io_uring_internal_bitmap *us_io_uring_internal_bitmap_create(uint32_t size);
void us_io_uring_internal_bitmap_destroy(struct us_io_uring_internal_bitmap *bm);
void us_io_uring_internal_bitmap_set(struct us_io_uring_internal_bitmap *bm, uint32_t bit);
void us_io_uring_internal_bitmap_clear(struct us_io_uring_internal_bitmap *bm, uint32_t bit);
int us_io_uring_internal_bitmap_find_first_set(struct us_io_uring_internal_bitmap *bm, uint32_t max);

/* Helper to get global context */
static inline struct us_io_uring_internal_context *us_io_uring_internal_get_context(void) {
    extern struct us_io_uring_internal_context *g_uring_context;
    return g_uring_context;
}

#endif /* LIBUS_USE_IO_URING */

#endif /* IO_URING_CONTEXT_H */
