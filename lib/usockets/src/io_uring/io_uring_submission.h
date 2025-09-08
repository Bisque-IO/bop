/*
 * io_uring submission and completion handling header
 */

#ifndef IO_URING_SUBMISSION_H
#define IO_URING_SUBMISSION_H

#ifdef LIBUS_USE_IO_URING

#include <stdint.h>
#include <sys/socket.h>
#include "liburing.h"
#include "io_uring_context.h"

/* User data structure encoded in 64-bit value */
struct user_data {
    uint32_t op_type : 8;      /* Operation type */
    uint32_t fd_idx : 16;      /* FD table index or actual FD */
    uint32_t flags : 8;        /* Custom flags */
    uint32_t conn_id;          /* Connection ID */
};

/* Operation types for user data encoding */
enum operation_type {
    OP_ACCEPT = 1,
    OP_CONNECT,
    OP_READ,
    OP_WRITE,
    OP_CLOSE,
    OP_TIMEOUT,
    OP_CANCEL,
    OP_POLL,
    OP_RECV,
    OP_SEND,
    OP_RECVMSG,
    OP_SENDMSG,
    OP_SHUTDOWN,
    OP_SPLICE,
    OP_TEE,
};

/* User data encoding/decoding functions */
uint64_t encode_user_data(struct user_data *ud);
void decode_user_data(uint64_t data, struct user_data *ud);

/* CQE event structure */
struct cqe_event {
    int32_t res;           /* Result from CQE */
    uint32_t flags;        /* CQE flags */
    uint8_t op_type;       /* Operation type */
    uint16_t fd_idx;       /* FD index or actual FD */
    uint32_t conn_id;      /* Connection ID */
    uint8_t user_flags;    /* User-defined flags */
};

/* Retry configuration */
struct retry_config {
    int max_attempts;         /* Maximum retry attempts */
    int initial_backoff_ms;   /* Initial backoff in milliseconds */
    int max_backoff_ms;       /* Maximum backoff in milliseconds */
};

/* CQE handler callback type */
typedef void (*us_io_uring_internal_cqe_handler)(struct us_io_uring_internal_context *ctx,
                                                 struct cqe_event *event,
                                                 void *user_data);

/* Submission functions */
int us_io_uring_internal_submit_sqe(struct us_io_uring_internal_context *ctx, struct io_uring_sqe *sqe);
int us_io_uring_internal_batch_add(struct us_io_uring_internal_context *ctx, struct io_uring_sqe *sqe);
int us_io_uring_internal_batch_submit(struct us_io_uring_internal_context *ctx);
int us_io_uring_internal_batch_check_timeout(struct us_io_uring_internal_context *ctx);

/* Completion processing */
int us_io_uring_internal_process_cqes(struct us_io_uring_internal_context *ctx, 
                                      us_io_uring_internal_cqe_handler handler,
                                      void *handler_data,
                                      unsigned int max_events);

/* Submission with retry */
int us_io_uring_internal_submit_with_retry(struct us_io_uring_internal_context *ctx,
                                           struct io_uring_sqe *sqe,
                                           struct retry_config *retry);

/* Operation preparation helpers */
void us_io_uring_internal_prep_accept_multishot(struct us_io_uring_internal_context *ctx,
                                                struct io_uring_sqe *sqe,
                                                int listen_fd,
                                                uint32_t conn_id);

void us_io_uring_internal_prep_connect_ex(struct us_io_uring_internal_context *ctx,
                                          struct io_uring_sqe *sqe,
                                          int socket_fd,
                                          const struct sockaddr *addr,
                                          socklen_t addrlen,
                                          uint32_t conn_id);

void us_io_uring_internal_prep_read_multishot(struct us_io_uring_internal_context *ctx,
                                              struct io_uring_sqe *sqe,
                                              int fd,
                                              size_t len,
                                              uint32_t conn_id,
                                              uint16_t buf_group);

void us_io_uring_internal_prep_write_zerocopy(struct us_io_uring_internal_context *ctx,
                                              struct io_uring_sqe *sqe,
                                              int fd,
                                              const void *buf,
                                              size_t len,
                                              uint32_t conn_id);

void us_io_uring_internal_prep_close_ex(struct us_io_uring_internal_context *ctx,
                                        struct io_uring_sqe *sqe,
                                        int fd,
                                        uint32_t conn_id);

void us_io_uring_internal_prep_timeout_ex(struct us_io_uring_internal_context *ctx,
                                          struct io_uring_sqe *sqe,
                                          struct __kernel_timespec *ts,
                                          uint32_t conn_id,
                                          unsigned int flags);

/* Operation management */
int us_io_uring_internal_cancel_by_conn(struct us_io_uring_internal_context *ctx, uint32_t conn_id);
void us_io_uring_internal_link_ops(struct io_uring_sqe *sqes[], int count);

/* CQE waiting */
int us_io_uring_internal_wait_cqe_timeout(struct us_io_uring_internal_context *ctx,
                                          struct io_uring_cqe **cqe_ptr,
                                          unsigned int timeout_ms);

int us_io_uring_internal_peek_cqe(struct us_io_uring_internal_context *ctx,
                                  struct io_uring_cqe **cqe_ptr);

/* SQE management */
struct io_uring_sqe *us_io_uring_internal_get_sqe_batched(struct us_io_uring_internal_context *ctx);

#endif /* LIBUS_USE_IO_URING */

#endif /* IO_URING_SUBMISSION_H */
