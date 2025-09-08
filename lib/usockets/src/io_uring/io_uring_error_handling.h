/*
 * Enhanced error handling and recovery mechanisms for io_uring
 * Provides graceful degradation and automatic fallback to epoll
 */

#ifndef IO_URING_ERROR_HANDLING_H
#define IO_URING_ERROR_HANDLING_H

#ifdef LIBUS_USE_IO_URING

#include <stdint.h>
#include <errno.h>
#include <time.h>
#include "io_uring_context.h"

/* Error severity levels */
enum us_io_uring_internal_error_severity {
    US_IO_URING_INTERNAL_ERROR_SEVERITY_INFO = 0,     /* Informational, no action needed */
    US_IO_URING_INTERNAL_ERROR_SEVERITY_WARNING,      /* Warning, but operation can continue */
    US_IO_URING_INTERNAL_ERROR_SEVERITY_ERROR,        /* Error, operation failed but recoverable */
    US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL         /* Fatal error, requires fallback */
};

/* Error types for classification */
enum us_io_uring_internal_error_type {
    US_IO_URING_INTERNAL_ERROR_TYPE_SYSTEM = 0,       /* System-level errors (ENOMEM, etc.) */
    US_IO_URING_INTERNAL_ERROR_TYPE_IO_URING,         /* io_uring specific errors */
    US_IO_URING_INTERNAL_ERROR_TYPE_NETWORK,          /* Network-related errors */
    US_IO_URING_INTERNAL_ERROR_TYPE_SSL,              /* SSL/TLS errors */
    US_IO_URING_INTERNAL_ERROR_TYPE_BUFFER,           /* Buffer management errors */
    US_IO_URING_INTERNAL_ERROR_TYPE_TIMEOUT,          /* Operation timeouts */
    US_IO_URING_INTERNAL_ERROR_TYPE_USER              /* User/application errors */
};

/* Error recovery strategy */
enum us_io_uring_internal_recovery_strategy {
    US_IO_URING_INTERNAL_RECOVERY_NONE = 0,           /* No recovery, fail immediately */
    US_IO_URING_INTERNAL_RECOVERY_RETRY,              /* Simple retry with backoff */
    US_IO_URING_INTERNAL_RECOVERY_RETRY_DIFFERENT,    /* Retry with different parameters */
    US_IO_URING_INTERNAL_RECOVERY_FALLBACK_EPOLL,     /* Fallback to epoll mode */
    US_IO_URING_INTERNAL_RECOVERY_DEGRADE_FEATURES,   /* Disable advanced features and retry */
    US_IO_URING_INTERNAL_RECOVERY_CANCEL_OPERATION    /* Cancel related operations */
};

/* Error context information */
struct us_io_uring_internal_error_context {
    int error_code;              /* Original error code (errno) */
    enum us_io_uring_internal_error_type type;        /* Classification of error */
    enum us_io_uring_internal_error_severity severity; /* How serious is this error */
    uint32_t conn_id;            /* Connection ID if applicable */
    int fd;                      /* File descriptor if applicable */
    uint64_t timestamp_ns;       /* When error occurred */
    const char *operation;       /* What operation failed */
    const char *function;        /* Function where error occurred */
    int line;                    /* Line number where error occurred */
    char details[256];           /* Additional error details */
};

/* Recovery attempt tracking */
struct us_io_uring_internal_recovery_attempt {
    enum us_io_uring_internal_recovery_strategy strategy;
    int attempt_number;
    uint64_t timestamp_ns;
    int backoff_ms;
    int success;
    int final_error;
};

/* Error handler configuration */
struct us_io_uring_internal_error_handler_config {
    int max_retries;             /* Maximum retry attempts */
    int initial_backoff_ms;      /* Initial backoff in milliseconds */
    int max_backoff_ms;          /* Maximum backoff in milliseconds */
    double backoff_multiplier;   /* Exponential backoff multiplier */
    int enable_jitter;           /* Add random jitter to backoff */
    int timeout_ms;              /* Overall timeout for recovery */
    
    /* Callback functions */
    void (*on_error)(const struct us_io_uring_internal_error_context *ctx);
    int (*on_recovery_attempt)(const struct us_io_uring_internal_error_context *ctx, 
                              struct us_io_uring_internal_recovery_attempt *attempt);
    void (*on_recovery_success)(const struct us_io_uring_internal_error_context *ctx,
                               const struct us_io_uring_internal_recovery_attempt *attempt);
    void (*on_recovery_failed)(const struct us_io_uring_internal_error_context *ctx);
};

/* Global error statistics */
struct us_io_uring_internal_error_statistics {
    _Atomic(uint64_t) total_errors;
    _Atomic(uint64_t) errors_by_type[US_IO_URING_INTERNAL_ERROR_TYPE_USER + 1];
    _Atomic(uint64_t) errors_by_severity[US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL + 1];
    _Atomic(uint64_t) recovery_attempts;
    _Atomic(uint64_t) recovery_successes;
    _Atomic(uint64_t) recovery_failures;
    _Atomic(uint64_t) fallback_activations;
    
    /* Performance impact */
    _Atomic(uint64_t) total_recovery_time_ns;
    _Atomic(uint64_t) max_recovery_time_ns;
    
    /* Recent error rate tracking */
    uint64_t error_timestamps[100];  /* Ring buffer of recent errors */
    _Atomic(int) error_index;
};

/* Fallback mechanism state */
enum us_io_uring_internal_fallback_state {
    US_IO_URING_INTERNAL_FALLBACK_DISABLED = 0,       /* io_uring working normally */
    US_IO_URING_INTERNAL_FALLBACK_DEGRADED,           /* Some features disabled */
    US_IO_URING_INTERNAL_FALLBACK_EPOLL_PARTIAL,      /* Some connections use epoll */
    US_IO_URING_INTERNAL_FALLBACK_EPOLL_FULL          /* All connections use epoll */
};

struct us_io_uring_internal_fallback_manager {
    enum us_io_uring_internal_fallback_state state;
    pthread_mutex_t state_mutex;
    
    /* Trigger conditions */
    int error_rate_threshold;    /* Errors per second to trigger fallback */
    int consecutive_failures;    /* Consecutive failures to trigger */
    int max_consecutive_failures;
    
    /* Epoll fallback context */
    int epoll_fd;
    struct epoll_event *events;
    int max_events;
    
    /* Recovery conditions */
    uint64_t fallback_start_time;
    int recovery_check_interval_ms;
    int min_stable_time_ms;      /* Minimum stable time before recovery */
    
    /* Statistics */
    _Atomic(uint64_t) fallback_count;
    _Atomic(uint64_t) recovery_count;
    _Atomic(uint64_t) total_fallback_time_ns;
};

/* Error handling API */

/* Initialize error handling system */
int us_io_uring_internal_io_error_handler_init(struct us_io_uring_internal_error_handler_config *config);
void us_io_uring_internal_io_error_handler_cleanup(void);

/* Error reporting */
int us_io_uring_internal_io_error_report(int error_code, enum us_io_uring_internal_error_type type, 
                   enum us_io_uring_internal_error_severity severity,
                   uint32_t conn_id, int fd,
                   const char *operation, const char *function, int line);

/* Convenience macros for error reporting */
#define IO_ERROR_REPORT_SYSTEM(code, conn_id, fd, op) \
    us_io_uring_internal_io_error_report(code, US_IO_URING_INTERNAL_ERROR_TYPE_SYSTEM, US_IO_URING_INTERNAL_ERROR_SEVERITY_ERROR, \
                    conn_id, fd, op, __FUNCTION__, __LINE__)

#define IO_ERROR_REPORT_IO_URING(code, conn_id, fd, op) \
    us_io_uring_internal_io_error_report(code, US_IO_URING_INTERNAL_ERROR_TYPE_IO_URING, US_IO_URING_INTERNAL_ERROR_SEVERITY_ERROR, \
                    conn_id, fd, op, __FUNCTION__, __LINE__)

#define IO_ERROR_REPORT_FATAL(code, type, op) \
    us_io_uring_internal_io_error_report(code, type, US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL, 0, -1, \
                    op, __FUNCTION__, __LINE__)

/* Error recovery */
int us_io_uring_internal_io_error_attempt_recovery(struct us_io_uring_internal_context *ctx,
                             const struct us_io_uring_internal_error_context *error_ctx);

/* Fallback management */
int us_io_uring_internal_io_fallback_manager_init(void);
void us_io_uring_internal_io_fallback_manager_cleanup(void);
int us_io_uring_internal_io_fallback_check_trigger(void);
int us_io_uring_internal_io_fallback_activate(enum us_io_uring_internal_fallback_state target_state);
int us_io_uring_internal_io_fallback_attempt_recovery(void);
enum us_io_uring_internal_fallback_state us_io_uring_internal_io_fallback_get_state(void);

/* Statistics */
void us_io_uring_internal_io_error_get_statistics(struct us_io_uring_internal_error_statistics *stats);
void us_io_uring_internal_io_error_reset_statistics(void);
double us_io_uring_internal_io_error_get_rate(int window_seconds);

/* Utility functions */
const char *us_io_uring_internal_io_error_type_string(enum us_io_uring_internal_error_type type);
const char *us_io_uring_internal_io_error_severity_string(enum us_io_uring_internal_error_severity severity);
const char *us_io_uring_internal_io_error_strategy_string(enum us_io_uring_internal_recovery_strategy strategy);
const char *us_io_uring_internal_io_fallback_state_string(enum us_io_uring_internal_fallback_state state);

/* Error context helpers */
void us_io_uring_internal_io_error_context_init(struct us_io_uring_internal_error_context *ctx);
void us_io_uring_internal_io_error_context_set_details(struct us_io_uring_internal_error_context *ctx, 
                                 const char *format, ...);

/* Timeout handling */
int us_io_uring_internal_io_timeout_manager_init(void);
void us_io_uring_internal_io_timeout_manager_cleanup(void);
int us_io_uring_internal_io_timeout_schedule(struct us_io_uring_internal_context *ctx, uint32_t conn_id, 
                       int timeout_ms, void (*callback)(uint32_t conn_id));
int us_io_uring_internal_io_timeout_cancel(uint32_t conn_id);

/* Circuit breaker pattern */
struct us_io_uring_internal_circuit_breaker {
    atomic_int failures;
    atomic_int successes;
    _Atomic(uint64_t) last_failure_time;
    atomic_int state;  /* 0=closed, 1=open, 2=half-open */
    
    int failure_threshold;
    int success_threshold;
    int timeout_ms;
    
    pthread_mutex_t mutex;
};

int us_io_uring_internal_io_circuit_breaker_init(struct us_io_uring_internal_circuit_breaker *cb,
                           int failure_threshold,
                           int success_threshold, 
                           int timeout_ms);
int us_io_uring_internal_io_circuit_breaker_call(struct us_io_uring_internal_circuit_breaker *cb);
void us_io_uring_internal_io_circuit_breaker_success(struct us_io_uring_internal_circuit_breaker *cb);
void us_io_uring_internal_io_circuit_breaker_failure(struct us_io_uring_internal_circuit_breaker *cb);
int us_io_uring_internal_io_circuit_breaker_is_open(struct us_io_uring_internal_circuit_breaker *cb);

#endif /* LIBUS_USE_IO_URING */

#endif /* IO_URING_ERROR_HANDLING_H */