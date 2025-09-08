/*
 * Buffer pool header for io_uring implementation
 */

#ifndef BUFFER_POOL_H
#define BUFFER_POOL_H

#ifdef LIBUS_USE_IO_URING

#include <stddef.h>
#include <stdint.h>
#include <pthread.h>

struct io_uring;
struct us_io_uring_internal_buffer_pool;

/* Handle for an allocated buffer */
struct us_io_uring_internal_buffer_handle {
    void *data;           /* Pointer to buffer data */
    size_t size;          /* Size of buffer */
    uint16_t bid;         /* Buffer ID for io_uring */
    struct us_io_uring_internal_buffer_pool *pool; /* Pool this buffer came from */
};

/* Statistics for monitoring */
struct us_io_uring_internal_buffer_pool_stats {
    uint64_t total_allocations;
    uint64_t total_deallocations;
    uint64_t total_failed_allocations;
    uint32_t total_buffers;
    size_t total_memory;
};

/* Initialize the buffer pool manager */
int us_io_uring_internal_buffer_pool_init(struct io_uring *ring);

/* Allocate a buffer of at least 'size' bytes */
struct us_io_uring_internal_buffer_handle *us_io_uring_internal_buffer_pool_alloc(size_t size);

/* Return a buffer to the pool */
void us_io_uring_internal_buffer_pool_free(struct us_io_uring_internal_buffer_handle *handle);

/* Get pool statistics */
void us_io_uring_internal_buffer_pool_get_stats(struct us_io_uring_internal_buffer_pool_stats *stats);

/* Clean up all buffer pools */
void us_io_uring_internal_buffer_pool_cleanup(void);

/* Helper macros for common buffer operations */
#define BUFFER_DATA(h) ((h)->data)
#define BUFFER_SIZE(h) ((h)->size)
#define BUFFER_ID(h) ((h)->bid)

#endif /* LIBUS_USE_IO_URING */

#endif /* BUFFER_POOL_H */
