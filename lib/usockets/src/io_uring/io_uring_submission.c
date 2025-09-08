/*
 * io_uring submission and completion handling
 * Implements batching, error handling, and async operations
 */

#ifdef LIBUS_USE_IO_URING

#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <time.h>
#include <sys/eventfd.h>
#include "liburing.h"
#include "io_uring_context.h"
#include "io_uring_submission.h"


/* Encode user data into 64-bit value */
uint64_t encode_user_data(struct user_data *ud) {
    return ((uint64_t)ud->conn_id << 32) | 
           ((uint64_t)ud->flags << 24) |
           ((uint64_t)ud->fd_idx << 8) |
           ud->op_type;
}

/* Decode 64-bit value into user data */
void decode_user_data(uint64_t data, struct user_data *ud) {
    ud->op_type = data & 0xFF;
    ud->fd_idx = (data >> 8) & 0xFFFF;
    ud->flags = (data >> 24) & 0xFF;
    ud->conn_id = data >> 32;
}

/* Get current time in nanoseconds */
static inline uint64_t get_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}

/* Submit a single SQE immediately */
int us_io_uring_internal_submit_sqe(struct us_io_uring_internal_context *ctx, struct io_uring_sqe *sqe) {
    if (!ctx || !sqe) {
        return -EINVAL;
    }
    
    int ret = io_uring_submit(&ctx->ring);
    if (ret < 0) {
        atomic_fetch_add(&ctx->stats.sqes_dropped, 1);
        atomic_fetch_add(&ctx->stats.errors[US_IO_URING_INTERNAL_EAGAIN], 1);
        return ret;
    }
    
    atomic_fetch_add(&ctx->stats.sqes_submitted, 1);
    return 0;
}

/* Add SQE to batch */
int us_io_uring_internal_batch_add(struct us_io_uring_internal_context *ctx, struct io_uring_sqe *sqe) {
    if (!ctx || !sqe) {
        return -EINVAL;
    }
    
    pthread_mutex_lock(&ctx->mutex);
    
    /* Check if batch is full */
    if (ctx->batch.count >= MAX_BATCH_SIZE) {
        /* Submit current batch */
        us_io_uring_internal_batch_submit(ctx);
    }
    
    /* Add to batch */
    ctx->batch.sqes[ctx->batch.count++] = sqe;
    
    /* Update timestamp if this is first in batch */
    if (ctx->batch.count == 1) {
        ctx->batch.submit_ts = get_time_ns();
    }
    
    pthread_mutex_unlock(&ctx->mutex);
    return 0;
}

/* Submit batched SQEs */
int us_io_uring_internal_batch_submit(struct us_io_uring_internal_context *ctx) {
    if (!ctx || ctx->batch.count == 0) {
        return 0;
    }
    
    pthread_mutex_lock(&ctx->mutex);
    
    int ret = io_uring_submit(&ctx->ring);
    if (ret < 0) {
        atomic_fetch_add(&ctx->stats.sqes_dropped, ctx->batch.count);
        atomic_fetch_add(&ctx->stats.errors[US_IO_URING_INTERNAL_EAGAIN], 1);
    } else {
        atomic_fetch_add(&ctx->stats.sqes_submitted, ctx->batch.count);
        atomic_fetch_add(&ctx->stats.batch_count, 1);
    }
    
    /* Reset batch */
    ctx->batch.count = 0;
    ctx->batch.submit_ts = 0;
    
    pthread_mutex_unlock(&ctx->mutex);
    return ret;
}

/* Check if batch should be submitted based on timeout */
int us_io_uring_internal_batch_check_timeout(struct us_io_uring_internal_context *ctx) {
    if (!ctx || ctx->batch.count == 0) {
        return 0;
    }
    
    uint64_t now = get_time_ns();
    if (now - ctx->batch.submit_ts >= ctx->batch_timeout_ns) {
        return us_io_uring_internal_batch_submit(ctx);
    }
    
    return 0;
}

/* Process completion queue entries */
int us_io_uring_internal_process_cqes(struct us_io_uring_internal_context *ctx, 
                                      us_io_uring_internal_cqe_handler handler,
                          void *handler_data,
                          unsigned int max_events) {
    if (!ctx || !handler) {
        return -EINVAL;
    }
    
    struct io_uring_cqe *cqe;
    unsigned head;
    unsigned count = 0;
    
    pthread_spin_lock(&ctx->cqe_lock);
    
    io_uring_for_each_cqe(&ctx->ring, head, cqe) {
        if (++count > max_events && max_events > 0) {
            break;
        }
        
        /* Decode user data */
        struct user_data ud;
        decode_user_data((uint64_t)io_uring_cqe_get_data(cqe), &ud);
        
        /* Update statistics */
        atomic_fetch_add(&ctx->stats.cqes_processed, 1);
        
        /* Track errors */
        if (cqe->res < 0) {
            enum us_io_uring_internal_error err = US_IO_URING_INTERNAL_EINVAL;
            switch (-cqe->res) {
                case EAGAIN: err = US_IO_URING_INTERNAL_EAGAIN; break;
                case ENOMEM: err = US_IO_URING_INTERNAL_ENOMEM; break;
                case ENOBUFS: err = US_IO_URING_INTERNAL_ENOBUFS; break;
                case ECANCELED: err = US_IO_URING_INTERNAL_ECANCELED; break;
                case ETIMEDOUT: err = US_IO_URING_INTERNAL_ETIMEDOUT; break;
                case ECONNRESET: err = US_IO_URING_INTERNAL_ECONNRESET; break;
                case EPIPE: err = US_IO_URING_INTERNAL_EPIPE; break;
            }
            atomic_fetch_add(&ctx->stats.errors[err], 1);
        }
        
        /* Call handler */
        struct cqe_event event = {
            .res = cqe->res,
            .flags = cqe->flags,
            .op_type = ud.op_type,
            .fd_idx = ud.fd_idx,
            .conn_id = ud.conn_id,
            .user_flags = ud.flags
        };
        
        handler(ctx, &event, handler_data);
    }
    
    /* Advance CQ */
    if (count > 0) {
        io_uring_cq_advance(&ctx->ring, count);
    }
    
    pthread_spin_unlock(&ctx->cqe_lock);
    
    return count;
}

/* Submit with retry logic */
int us_io_uring_internal_submit_with_retry(struct us_io_uring_internal_context *ctx,
                               struct io_uring_sqe *sqe,
                               struct retry_config *retry) {
    if (!ctx || !sqe) {
        return -EINVAL;
    }
    
    int attempts = 0;
    int backoff_ms = retry ? retry->initial_backoff_ms : 10;
    int max_attempts = retry ? retry->max_attempts : 3;
    
    while (attempts < max_attempts) {
        int ret = io_uring_submit(&ctx->ring);
        if (ret >= 0) {
            atomic_fetch_add(&ctx->stats.sqes_submitted, 1);
            return 0;
        }
        
        if (ret != -EAGAIN && ret != -EBUSY) {
            atomic_fetch_add(&ctx->stats.sqes_dropped, 1);
            return ret;
        }
        
        /* Exponential backoff */
        if (attempts > 0) {
            usleep(backoff_ms * 1000);
            backoff_ms *= 2;
            if (retry && backoff_ms > retry->max_backoff_ms) {
                backoff_ms = retry->max_backoff_ms;
            }
        }
        
        attempts++;
    }
    
    atomic_fetch_add(&ctx->stats.sqes_dropped, 1);
    return -EAGAIN;
}

/* Prepare accept operation */
void us_io_uring_internal_prep_accept_multishot(struct us_io_uring_internal_context *ctx,
                                    struct io_uring_sqe *sqe,
                                    int listen_fd,
                                    uint32_t conn_id) {
    /* Use multishot accept if available */
    if (ctx->features & (1 << 4)) { /* URING_FEAT_MULTISHOT_ACCEPT */
        io_uring_prep_multishot_accept_direct(sqe, listen_fd, 
                                              NULL, NULL, 0);
        sqe->flags |= IOSQE_BUFFER_SELECT;
    } else {
        io_uring_prep_accept(sqe, listen_fd, NULL, NULL, 0);
    }
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_ACCEPT,
        .fd_idx = listen_fd,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Prepare connect operation */
void us_io_uring_internal_prep_connect_ex(struct us_io_uring_internal_context *ctx,
                              struct io_uring_sqe *sqe,
                              int socket_fd,
                              const struct sockaddr *addr,
                              socklen_t addrlen,
                              uint32_t conn_id) {
    io_uring_prep_connect(sqe, socket_fd, addr, addrlen);
    
    /* Use registered FD if available */
    int reg_fd = us_io_uring_internal_register_fd(ctx, socket_fd);
    if (reg_fd != socket_fd) {
        sqe->fd = reg_fd;
        sqe->flags |= IOSQE_FIXED_FILE;
    }
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_CONNECT,
        .fd_idx = reg_fd,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Prepare read operation with buffer selection */
void us_io_uring_internal_prep_read_multishot(struct us_io_uring_internal_context *ctx,
                                  struct io_uring_sqe *sqe,
                                  int fd,
                                  size_t len,
                                  uint32_t conn_id,
                                  uint16_t buf_group) {
    /* Use multishot receive if available */
    io_uring_prep_recv_multishot(sqe, fd, NULL, len, 0);
    
    /* Enable buffer selection */
    if (ctx->features & (1 << 5)) { /* URING_FEAT_BUFFER_SELECT */
        sqe->flags |= IOSQE_BUFFER_SELECT;
        sqe->buf_group = buf_group;
    }
    
    /* Use registered FD if available */
    int reg_fd = us_io_uring_internal_register_fd(ctx, fd);
    if (reg_fd != fd) {
        sqe->fd = reg_fd;
        sqe->flags |= IOSQE_FIXED_FILE;
    }
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_READ,
        .fd_idx = reg_fd,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Prepare write operation with zero-copy */
void us_io_uring_internal_prep_write_zerocopy(struct us_io_uring_internal_context *ctx,
                                  struct io_uring_sqe *sqe,
                                  int fd,
                                  const void *buf,
                                  size_t len,
                                  uint32_t conn_id) {
    /* Check for zero-copy support */
    if (ctx->config.enable_zero_copy && len >= 4096) {
        io_uring_prep_send_zc(sqe, fd, buf, len, 0, 0);
        atomic_fetch_add(&ctx->stats.zero_copy_sends, 1);
    } else {
        io_uring_prep_send(sqe, fd, buf, len, 0);
    }
    
    /* Use registered FD if available */
    int reg_fd = us_io_uring_internal_register_fd(ctx, fd);
    if (reg_fd != fd) {
        sqe->fd = reg_fd;
        sqe->flags |= IOSQE_FIXED_FILE;
    }
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_WRITE,
        .fd_idx = reg_fd,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Prepare close operation */
void us_io_uring_internal_prep_close_ex(struct us_io_uring_internal_context *ctx,
                            struct io_uring_sqe *sqe,
                            int fd,
                            uint32_t conn_id) {
    /* Check if FD is registered */
    int reg_fd = fd;
    if (ctx->fd_table.fd_map[fd] > 0) {
        reg_fd = ctx->fd_table.fd_map[fd] - 1;
        io_uring_prep_close_direct(sqe, reg_fd);
        
        /* Unregister after close */
        us_io_uring_internal_unregister_fd(ctx, fd);
    } else {
        io_uring_prep_close(sqe, fd);
    }
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_CLOSE,
        .fd_idx = reg_fd,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Prepare timeout operation */
void us_io_uring_internal_prep_timeout_ex(struct us_io_uring_internal_context *ctx,
                              struct io_uring_sqe *sqe,
                              struct __kernel_timespec *ts,
                              uint32_t conn_id,
                              unsigned int flags) {
    io_uring_prep_timeout(sqe, ts, 0, flags);
    
    /* Set user data */
    struct user_data ud = {
        .op_type = OP_TIMEOUT,
        .fd_idx = 0,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
}

/* Cancel pending operations for a connection */
int us_io_uring_internal_cancel_by_conn(struct us_io_uring_internal_context *ctx, uint32_t conn_id) {
    if (!ctx) {
        return -EINVAL;
    }
    
    struct io_uring_sqe *sqe = io_uring_get_sqe(&ctx->ring);
    if (!sqe) {
        return -ENOBUFS;
    }
    
    /* Cancel all operations for this connection ID */
    io_uring_prep_cancel64(sqe, conn_id, IORING_ASYNC_CANCEL_ANY);
    
    /* Set user data for cancel operation itself */
    struct user_data ud = {
        .op_type = OP_CANCEL,
        .fd_idx = 0,
        .conn_id = conn_id,
        .flags = 0
    };
    io_uring_sqe_set_data64(sqe, encode_user_data(&ud));
    
    return us_io_uring_internal_submit_sqe(ctx, sqe);
}

/* Link multiple operations */
void us_io_uring_internal_link_ops(struct io_uring_sqe *sqes[], int count) {
    if (!sqes || count < 2) {
        return;
    }
    
    /* Link all operations except the last one */
    for (int i = 0; i < count - 1; i++) {
        sqes[i]->flags |= IOSQE_IO_LINK;
    }
}

/* Wait for completion events */
int us_io_uring_internal_wait_cqe_timeout(struct us_io_uring_internal_context *ctx,
                              struct io_uring_cqe **cqe_ptr,
                              unsigned int timeout_ms) {
    if (!ctx || !cqe_ptr) {
        return -EINVAL;
    }
    
    struct __kernel_timespec ts = {
        .tv_sec = timeout_ms / 1000,
        .tv_nsec = (timeout_ms % 1000) * 1000000
    };
    
    int ret = io_uring_wait_cqe_timeout(&ctx->ring, cqe_ptr, &ts);
    if (ret == 0) {
        atomic_fetch_add(&ctx->stats.cqes_processed, 1);
    }
    
    return ret;
}

/* Peek for available CQEs without blocking */
int us_io_uring_internal_peek_cqe(struct us_io_uring_internal_context *ctx,
                      struct io_uring_cqe **cqe_ptr) {
    if (!ctx || !cqe_ptr) {
        return -EINVAL;
    }
    
    return io_uring_peek_cqe(&ctx->ring, cqe_ptr);
}

/* Get SQE with automatic batching */
struct io_uring_sqe *us_io_uring_internal_get_sqe_batched(struct us_io_uring_internal_context *ctx) {
    if (!ctx) {
        return NULL;
    }
    
    struct io_uring_sqe *sqe = io_uring_get_sqe(&ctx->ring);
    if (!sqe) {
        /* Try to submit current batch to free space */
        us_io_uring_internal_batch_submit(ctx);
        sqe = io_uring_get_sqe(&ctx->ring);
    }
    
    return sqe;
}

#endif /* LIBUS_USE_IO_URING */
