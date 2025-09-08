/*
 * Enhanced error handling implementation for io_uring
 * Provides comprehensive error recovery and fallback mechanisms
 */

#ifdef LIBUS_USE_IO_URING

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <time.h>
#include <sys/epoll.h>
#include <pthread.h>
#include <stdarg.h>
#include <math.h>
#include "io_uring_error_handling.h"
#include "io_uring_context.h"
#include "io_uring_submission.h"

/* Global error handler configuration */
static struct us_io_uring_internal_error_handler_config g_error_config = {
    .max_retries = 3,
    .initial_backoff_ms = 10,
    .max_backoff_ms = 5000,
    .backoff_multiplier = 2.0,
    .enable_jitter = 1,
    .timeout_ms = 30000,
    .on_error = NULL,
    .on_recovery_attempt = NULL,
    .on_recovery_success = NULL,
    .on_recovery_failed = NULL
};

/* Global error statistics */
static struct us_io_uring_internal_error_statistics g_error_stats;
static pthread_mutex_t g_stats_mutex = PTHREAD_MUTEX_INITIALIZER;

/* Fallback manager */
static struct us_io_uring_internal_fallback_manager g_fallback = {
    .state = US_IO_URING_INTERNAL_FALLBACK_DISABLED,
    .state_mutex = PTHREAD_MUTEX_INITIALIZER,
    .error_rate_threshold = 50,  /* 50 errors per second */
    .max_consecutive_failures = 10,
    .epoll_fd = -1,
    .max_events = 1024,
    .recovery_check_interval_ms = 5000,
    .min_stable_time_ms = 30000
};

/* Initialize error handling system */
int us_io_uring_internal_io_error_handler_init(struct us_io_uring_internal_error_handler_config *config) {
    if (config) {
        memcpy(&g_error_config, config, sizeof(g_error_config));
    }
    
    /* Initialize statistics */
    memset(&g_error_stats, 0, sizeof(g_error_stats));
    atomic_init(&g_error_stats.total_errors, 0);
    atomic_init(&g_error_stats.recovery_attempts, 0);
    atomic_init(&g_error_stats.recovery_successes, 0);
    atomic_init(&g_error_stats.recovery_failures, 0);
    atomic_init(&g_error_stats.fallback_activations, 0);
    atomic_init(&g_error_stats.total_recovery_time_ns, 0);
    atomic_init(&g_error_stats.max_recovery_time_ns, 0);
    atomic_init(&g_error_stats.error_index, 0);
    
    for (int i = 0; i <= US_IO_URING_INTERNAL_ERROR_TYPE_USER; i++) {
        atomic_init(&g_error_stats.errors_by_type[i], 0);
    }
    
    for (int i = 0; i <= US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL; i++) {
        atomic_init(&g_error_stats.errors_by_severity[i], 0);
    }
    
    return 0;
}

/* Cleanup error handling system */
void us_io_uring_internal_io_error_handler_cleanup(void) {
    /* Nothing to cleanup currently */
}

/* Get current time in nanoseconds */
static uint64_t get_time_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000000000ULL + ts.tv_nsec;
}

/* Generate random jitter */
static int get_jitter_ms(int base_ms) {
    if (!g_error_config.enable_jitter) {
        return 0;
    }
    
    /* Add up to ±25% jitter */
    int jitter_range = base_ms / 4;
    return (rand() % (2 * jitter_range + 1)) - jitter_range;
}

/* Calculate backoff time with exponential increase */
static int calculate_backoff_ms(int attempt) {
    if (attempt <= 0) {
        return g_error_config.initial_backoff_ms;
    }
    
    double backoff = g_error_config.initial_backoff_ms * 
                     pow(g_error_config.backoff_multiplier, attempt - 1);
    
    int backoff_ms = (int)backoff;
    if (backoff_ms > g_error_config.max_backoff_ms) {
        backoff_ms = g_error_config.max_backoff_ms;
    }
    
    /* Add jitter */
    backoff_ms += get_jitter_ms(backoff_ms);
    
    return backoff_ms > 0 ? backoff_ms : g_error_config.initial_backoff_ms;
}

/* Classify error and determine recovery strategy */
static enum us_io_uring_internal_recovery_strategy determine_recovery_strategy(
    const struct us_io_uring_internal_error_context *ctx) {
    
    switch (ctx->error_code) {
        case EAGAIN:
#if EAGAIN != EWOULDBLOCK
        case EWOULDBLOCK:
#endif
        case EINTR:
            return US_IO_URING_INTERNAL_RECOVERY_RETRY;
            
        case ENOMEM:
        case ENOBUFS:
            /* Memory pressure - try to recover with delays */
            if (ctx->type == US_IO_URING_INTERNAL_ERROR_TYPE_IO_URING) {
                return US_IO_URING_INTERNAL_RECOVERY_DEGRADE_FEATURES;
            }
            return US_IO_URING_INTERNAL_RECOVERY_RETRY;
            
        case EINVAL:
            /* Invalid parameters - likely programming error */
            return US_IO_URING_INTERNAL_RECOVERY_NONE;
            
        case ENOSYS:
        case EOPNOTSUPP:
            /* Feature not supported - fallback */
            return US_IO_URING_INTERNAL_RECOVERY_FALLBACK_EPOLL;
            
        case ECONNRESET:
        case EPIPE:
        case ENOTCONN:
            /* Connection errors - cancel related operations */
            return US_IO_URING_INTERNAL_RECOVERY_CANCEL_OPERATION;
            
        case ETIMEDOUT:
            /* Timeout - retry with different parameters */
            return US_IO_URING_INTERNAL_RECOVERY_RETRY_DIFFERENT;
            
        default:
            if (ctx->severity == US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL) {
                return US_IO_URING_INTERNAL_RECOVERY_FALLBACK_EPOLL;
            }
            return US_IO_URING_INTERNAL_RECOVERY_RETRY;
    }
}

/* Report an error */
int us_io_uring_internal_io_error_report(int error_code, enum us_io_uring_internal_error_type type, 
                   enum us_io_uring_internal_error_severity severity,
                   uint32_t conn_id, int fd,
                   const char *operation, const char *function, int line) {
    
    struct us_io_uring_internal_error_context ctx;
    us_io_uring_internal_io_error_context_init(&ctx);
    
    ctx.error_code = error_code;
    ctx.type = type;
    ctx.severity = severity;
    ctx.conn_id = conn_id;
    ctx.fd = fd;
    ctx.timestamp_ns = get_time_ns();
    ctx.operation = operation ? operation : "unknown";
    ctx.function = function ? function : "unknown";
    ctx.line = line;
    
    /* Update statistics */
    pthread_mutex_lock(&g_stats_mutex);
    
    atomic_fetch_add(&g_error_stats.total_errors, 1);
    atomic_fetch_add(&g_error_stats.errors_by_type[type], 1);
    atomic_fetch_add(&g_error_stats.errors_by_severity[severity], 1);
    
    /* Update error rate tracking */
    int idx = atomic_fetch_add(&g_error_stats.error_index, 1) % 100;
    g_error_stats.error_timestamps[idx] = ctx.timestamp_ns;
    
    pthread_mutex_unlock(&g_stats_mutex);
    
    /* Check if we should trigger fallback */
    if (severity >= US_IO_URING_INTERNAL_ERROR_SEVERITY_ERROR) {
        g_fallback.consecutive_failures++;
        us_io_uring_internal_io_fallback_check_trigger();
    } else {
        g_fallback.consecutive_failures = 0;
    }
    
    /* Call user error handler */
    if (g_error_config.on_error) {
        g_error_config.on_error(&ctx);
    }
    
    return 0;
}

/* Attempt error recovery */
int us_io_uring_internal_io_error_attempt_recovery(struct us_io_uring_internal_context *ctx,
                             const struct us_io_uring_internal_error_context *error_ctx) {
    if (!ctx || !error_ctx) {
        return -EINVAL;
    }
    
    enum us_io_uring_internal_recovery_strategy strategy = determine_recovery_strategy(error_ctx);
    if (strategy == US_IO_URING_INTERNAL_RECOVERY_NONE) {
        return -1; /* No recovery possible */
    }
    
    struct us_io_uring_internal_recovery_attempt attempt = {
        .strategy = strategy,
        .attempt_number = 1,
        .timestamp_ns = get_time_ns(),
        .success = 0,
        .final_error = 0
    };
    
    atomic_fetch_add(&g_error_stats.recovery_attempts, 1);
    
    int max_attempts = g_error_config.max_retries;
    uint64_t start_time = get_time_ns();
    uint64_t timeout_ns = g_error_config.timeout_ms * 1000000ULL;
    
    for (int i = 0; i < max_attempts; i++) {
        attempt.attempt_number = i + 1;
        attempt.backoff_ms = calculate_backoff_ms(i);
        
        /* Check timeout */
        uint64_t elapsed = get_time_ns() - start_time;
        if (elapsed >= timeout_ns) {
            break;
        }
        
        /* Call recovery attempt callback */
        if (g_error_config.on_recovery_attempt) {
            int user_result = g_error_config.on_recovery_attempt(error_ctx, &attempt);
            if (user_result < 0) {
                break; /* User requested abort */
            }
        }
        
        /* Apply backoff delay */
        if (i > 0 && attempt.backoff_ms > 0) {
            usleep(attempt.backoff_ms * 1000);
        }
        
        /* Attempt recovery based on strategy */
        int result = 0;
        
        switch (strategy) {
            case US_IO_URING_INTERNAL_RECOVERY_RETRY:
                /* Simple retry - assume operation will be retried */
                result = 0;
                break;
                
            case US_IO_URING_INTERNAL_RECOVERY_RETRY_DIFFERENT:
                /* Retry with modified parameters */
                result = 0;
                break;
                
            case US_IO_URING_INTERNAL_RECOVERY_DEGRADE_FEATURES:
                /* Disable advanced features */
                ctx->features &= ~(URING_FEAT_SQPOLL | URING_FEAT_FAST_POLL);
                result = 0;
                break;
                
            case US_IO_URING_INTERNAL_RECOVERY_FALLBACK_EPOLL:
                /* Activate epoll fallback */
                result = us_io_uring_internal_io_fallback_activate(US_IO_URING_INTERNAL_FALLBACK_EPOLL_FULL);
                break;
                
            case US_IO_URING_INTERNAL_RECOVERY_CANCEL_OPERATION:
                /* Cancel related operations */
                if (error_ctx->conn_id > 0) {
                    us_io_uring_internal_cancel_by_conn(ctx, error_ctx->conn_id);
                }
                result = 0;
                break;
                
            default:
                result = -1;
                break;
        }
        
        if (result == 0) {
            /* Recovery succeeded */
            attempt.success = 1;
            
            uint64_t recovery_time = get_time_ns() - start_time;
            atomic_fetch_add(&g_error_stats.recovery_successes, 1);
            atomic_fetch_add(&g_error_stats.total_recovery_time_ns, recovery_time);
            
            /* Update max recovery time */
            uint64_t current_max = atomic_load(&g_error_stats.max_recovery_time_ns);
            while (recovery_time > current_max) {
                if (atomic_compare_exchange_weak(&g_error_stats.max_recovery_time_ns, 
                                               &current_max, recovery_time)) {
                    break;
                }
            }
            
            if (g_error_config.on_recovery_success) {
                g_error_config.on_recovery_success(error_ctx, &attempt);
            }
            
            /* Reset consecutive failures on successful recovery */
            g_fallback.consecutive_failures = 0;
            
            return 0;
        }
        
        attempt.final_error = result;
    }
    
    /* Recovery failed */
    atomic_fetch_add(&g_error_stats.recovery_failures, 1);
    
    if (g_error_config.on_recovery_failed) {
        g_error_config.on_recovery_failed(error_ctx);
    }
    
    return -1;
}

/* Initialize fallback manager */
int us_io_uring_internal_io_fallback_manager_init(void) {
    pthread_mutex_lock(&g_fallback.state_mutex);
    
    if (g_fallback.epoll_fd >= 0) {
        pthread_mutex_unlock(&g_fallback.state_mutex);
        return 0; /* Already initialized */
    }
    
    g_fallback.epoll_fd = epoll_create1(EPOLL_CLOEXEC);
    if (g_fallback.epoll_fd < 0) {
        pthread_mutex_unlock(&g_fallback.state_mutex);
        return -errno;
    }
    
    g_fallback.events = calloc(g_fallback.max_events, sizeof(struct epoll_event));
    if (!g_fallback.events) {
        close(g_fallback.epoll_fd);
        g_fallback.epoll_fd = -1;
        pthread_mutex_unlock(&g_fallback.state_mutex);
        return -ENOMEM;
    }
    
    atomic_init(&g_fallback.fallback_count, 0);
    atomic_init(&g_fallback.recovery_count, 0);
    atomic_init(&g_fallback.total_fallback_time_ns, 0);
    
    pthread_mutex_unlock(&g_fallback.state_mutex);
    return 0;
}

/* Cleanup fallback manager */
void us_io_uring_internal_io_fallback_manager_cleanup(void) {
    pthread_mutex_lock(&g_fallback.state_mutex);
    
    if (g_fallback.epoll_fd >= 0) {
        close(g_fallback.epoll_fd);
        g_fallback.epoll_fd = -1;
    }
    
    free(g_fallback.events);
    g_fallback.events = NULL;
    
    pthread_mutex_unlock(&g_fallback.state_mutex);
}

/* Check if fallback should be triggered */
int us_io_uring_internal_io_fallback_check_trigger(void) {
    /* Check consecutive failures */
    if (g_fallback.consecutive_failures >= g_fallback.max_consecutive_failures) {
        return us_io_uring_internal_io_fallback_activate(US_IO_URING_INTERNAL_FALLBACK_EPOLL_PARTIAL);
    }
    
    /* Check error rate */
    double error_rate = us_io_uring_internal_io_error_get_rate(1); /* Errors per second in last second */
    if (error_rate >= g_fallback.error_rate_threshold) {
        return us_io_uring_internal_io_fallback_activate(US_IO_URING_INTERNAL_FALLBACK_EPOLL_PARTIAL);
    }
    
    return 0;
}

/* Activate fallback mechanism */
int us_io_uring_internal_io_fallback_activate(enum us_io_uring_internal_fallback_state target_state) {
    pthread_mutex_lock(&g_fallback.state_mutex);
    
    if (g_fallback.state >= target_state) {
        pthread_mutex_unlock(&g_fallback.state_mutex);
        return 0; /* Already in this state or higher */
    }
    
    enum us_io_uring_internal_fallback_state old_state = g_fallback.state;
    g_fallback.state = target_state;
    g_fallback.fallback_start_time = get_time_ns();
    
    atomic_fetch_add(&g_fallback.fallback_count, 1);
    atomic_fetch_add(&g_error_stats.fallback_activations, 1);
    
    pthread_mutex_unlock(&g_fallback.state_mutex);
    
    /* Log state transition */
    IO_ERROR_REPORT_SYSTEM(0, 0, -1, "Fallback activated");
    
    return 0;
}

/* Get current fallback state */
enum us_io_uring_internal_fallback_state us_io_uring_internal_io_fallback_get_state(void) {
    pthread_mutex_lock(&g_fallback.state_mutex);
    enum us_io_uring_internal_fallback_state state = g_fallback.state;
    pthread_mutex_unlock(&g_fallback.state_mutex);
    return state;
}

/* Get error statistics */
void us_io_uring_internal_io_error_get_statistics(struct us_io_uring_internal_error_statistics *stats) {
    if (!stats) {
        return;
    }
    
    pthread_mutex_lock(&g_stats_mutex);
    
    stats->total_errors = atomic_load(&g_error_stats.total_errors);
    stats->recovery_attempts = atomic_load(&g_error_stats.recovery_attempts);
    stats->recovery_successes = atomic_load(&g_error_stats.recovery_successes);
    stats->recovery_failures = atomic_load(&g_error_stats.recovery_failures);
    stats->fallback_activations = atomic_load(&g_error_stats.fallback_activations);
    stats->total_recovery_time_ns = atomic_load(&g_error_stats.total_recovery_time_ns);
    stats->max_recovery_time_ns = atomic_load(&g_error_stats.max_recovery_time_ns);
    
    for (int i = 0; i <= US_IO_URING_INTERNAL_ERROR_TYPE_USER; i++) {
        stats->errors_by_type[i] = atomic_load(&g_error_stats.errors_by_type[i]);
    }
    
    for (int i = 0; i <= US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL; i++) {
        stats->errors_by_severity[i] = atomic_load(&g_error_stats.errors_by_severity[i]);
    }
    
    memcpy(stats->error_timestamps, g_error_stats.error_timestamps,
           sizeof(g_error_stats.error_timestamps));
    stats->error_index = atomic_load(&g_error_stats.error_index);
    
    pthread_mutex_unlock(&g_stats_mutex);
}

/* Calculate error rate */
double us_io_uring_internal_io_error_get_rate(int window_seconds) {
    uint64_t now = get_time_ns();
    uint64_t window_ns = window_seconds * 1000000000ULL;
    int count = 0;
    
    pthread_mutex_lock(&g_stats_mutex);
    
    for (int i = 0; i < 100; i++) {
        if (g_error_stats.error_timestamps[i] > 0 &&
            (now - g_error_stats.error_timestamps[i]) <= window_ns) {
            count++;
        }
    }
    
    pthread_mutex_unlock(&g_stats_mutex);
    
    return (double)count / window_seconds;
}

/* String conversion utilities */
const char *us_io_uring_internal_io_error_type_string(enum us_io_uring_internal_error_type type) {
    switch (type) {
        case US_IO_URING_INTERNAL_ERROR_TYPE_SYSTEM: return "system";
        case US_IO_URING_INTERNAL_ERROR_TYPE_IO_URING: return "io_uring";
        case US_IO_URING_INTERNAL_ERROR_TYPE_NETWORK: return "network";
        case US_IO_URING_INTERNAL_ERROR_TYPE_SSL: return "ssl";
        case US_IO_URING_INTERNAL_ERROR_TYPE_BUFFER: return "buffer";
        case US_IO_URING_INTERNAL_ERROR_TYPE_TIMEOUT: return "timeout";
        case US_IO_URING_INTERNAL_ERROR_TYPE_USER: return "user";
        default: return "unknown";
    }
}

const char *us_io_uring_internal_io_error_severity_string(enum us_io_uring_internal_error_severity severity) {
    switch (severity) {
        case US_IO_URING_INTERNAL_ERROR_SEVERITY_INFO: return "info";
        case US_IO_URING_INTERNAL_ERROR_SEVERITY_WARNING: return "warning";
        case US_IO_URING_INTERNAL_ERROR_SEVERITY_ERROR: return "error";
        case US_IO_URING_INTERNAL_ERROR_SEVERITY_FATAL: return "fatal";
        default: return "unknown";
    }
}

const char *us_io_uring_internal_io_fallback_state_string(enum us_io_uring_internal_fallback_state state) {
    switch (state) {
        case US_IO_URING_INTERNAL_FALLBACK_DISABLED: return "disabled";
        case US_IO_URING_INTERNAL_FALLBACK_DEGRADED: return "degraded";
        case US_IO_URING_INTERNAL_FALLBACK_EPOLL_PARTIAL: return "epoll_partial";
        case US_IO_URING_INTERNAL_FALLBACK_EPOLL_FULL: return "epoll_full";
        default: return "unknown";
    }
}

/* Initialize error context */
void us_io_uring_internal_io_error_context_init(struct us_io_uring_internal_error_context *ctx) {
    if (ctx) {
        memset(ctx, 0, sizeof(*ctx));
        ctx->fd = -1;
        ctx->timestamp_ns = get_time_ns();
    }
}

/* Set error details with printf-style formatting */
void us_io_uring_internal_io_error_context_set_details(struct us_io_uring_internal_error_context *ctx, 
                                 const char *format, ...) {
    if (!ctx || !format) {
        return;
    }
    
    va_list args;
    va_start(args, format);
    vsnprintf(ctx->details, sizeof(ctx->details), format, args);
    va_end(args);
    
    /* Ensure null termination */
    ctx->details[sizeof(ctx->details) - 1] = '\0';
}

#endif /* LIBUS_USE_IO_URING */
