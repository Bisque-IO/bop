/*
 * Core io_uring context management for uSockets
 * Handles initialization, feature detection, and resource management
 */

#ifdef LIBUS_USE_IO_URING

#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <stdio.h>
#include <sys/utsname.h>
#include <sys/resource.h>
#include <sys/mman.h>
#include <sched.h>
#include <pthread.h>
#include "liburing.h"
#include "io_uring_context.h"
#include "io_uring_buffer_pool.h"

/* Minimum kernel version for full io_uring support */
#define MIN_KERNEL_MAJOR 5
#define MIN_KERNEL_MINOR 11

/* Default ring parameters */
#define DEFAULT_RING_SIZE 4096
#define DEFAULT_CQ_ENTRIES 8192
#define DEFAULT_SQ_THREAD_IDLE 10000 /* 10ms */


/* Global context instance */
struct us_io_uring_internal_context *g_uring_context = NULL;
static pthread_mutex_t g_context_mutex = PTHREAD_MUTEX_INITIALIZER;

/* Check kernel version for io_uring support */
static int check_kernel_version(void) {
    struct utsname info;
    if (uname(&info) != 0) {
        return -1;
    }
    
    int major, minor;
    if (sscanf(info.release, "%d.%d", &major, &minor) != 2) {
        return -1;
    }
    
    if (major < MIN_KERNEL_MAJOR || 
        (major == MIN_KERNEL_MAJOR && minor < MIN_KERNEL_MINOR)) {
        return -1;
    }
    
    return 0;
}

/* Detect available io_uring features */
static uint32_t detect_features(void) {
    uint32_t features = 0;
    struct io_uring ring;
    struct io_uring_params params;
    
    memset(&params, 0, sizeof(params));
    
    /* Try to initialize with advanced features */
    params.flags = IORING_SETUP_SQPOLL | IORING_SETUP_SQ_AFF;
    if (io_uring_queue_init_params(32, &ring, &params) == 0) {
        features |= URING_FEAT_SQPOLL | URING_FEAT_SQ_AFF;
        io_uring_queue_exit(&ring);
    }
    
    memset(&params, 0, sizeof(params));
    params.flags = IORING_SETUP_COOP_TASKRUN | IORING_SETUP_SINGLE_ISSUER;
    if (io_uring_queue_init_params(32, &ring, &params) == 0) {
        features |= URING_FEAT_COOP_TASKRUN | URING_FEAT_SINGLE_ISSUER;
        
        /* Test for multishot accept support */
        struct io_uring_probe *probe = io_uring_get_probe_ring(&ring);
        if (probe) {
            if (io_uring_opcode_supported(probe, IORING_OP_ACCEPT)) {
                features |= URING_FEAT_MULTISHOT_ACCEPT;
            }
            if (io_uring_opcode_supported(probe, IORING_OP_PROVIDE_BUFFERS)) {
                features |= URING_FEAT_BUFFER_SELECT;
            }
            io_uring_free_probe(probe);
        }
        
        /* Test for registered file descriptors */
        if (io_uring_register_files_sparse(&ring, 16) == 0) {
            features |= URING_FEAT_REGISTERED_FDS;
            io_uring_unregister_files(&ring);
        }
        
        io_uring_queue_exit(&ring);
    }
    
    return features;
}

/* Set CPU affinity for SQ polling thread */
static int set_sq_affinity(struct us_io_uring_internal_context *ctx, int cpu) {
    if (!(ctx->features & URING_FEAT_SQ_AFF)) {
        return -ENOTSUP;
    }
    
    cpu_set_t cpuset;
    CPU_ZERO(&cpuset);
    CPU_SET(cpu, &cpuset);
    
    /* This would be set in the params during init */
    ctx->sq_cpu = cpu;
    return 0;
}

/* Initialize file descriptor table for registered FDs */
static int init_fd_table(struct us_io_uring_internal_context *ctx, uint32_t size) {
    ctx->fd_table.capacity = size;
    ctx->fd_table.registered_fds = calloc(size, sizeof(int));
    ctx->fd_table.fd_map = calloc(65536, sizeof(uint32_t)); /* Max FD value */
    ctx->fd_table.free_slots = us_io_uring_internal_bitmap_create(size);
    
    if (!ctx->fd_table.registered_fds || !ctx->fd_table.fd_map || 
        !ctx->fd_table.free_slots) {
        free(ctx->fd_table.registered_fds);
        free(ctx->fd_table.fd_map);
        us_io_uring_internal_bitmap_destroy(ctx->fd_table.free_slots);
        return -ENOMEM;
    }
    
    /* Initialize all slots as free */
    for (uint32_t i = 0; i < size; i++) {
        ctx->fd_table.registered_fds[i] = -1;
        us_io_uring_internal_bitmap_set(ctx->fd_table.free_slots, i);
    }
    
    /* Register sparse files with io_uring */
    if (io_uring_register_files_sparse(&ctx->ring, size) < 0) {
        free(ctx->fd_table.registered_fds);
        free(ctx->fd_table.fd_map);
        us_io_uring_internal_bitmap_destroy(ctx->fd_table.free_slots);
        return -errno;
    }
    
    return 0;
}

/* Initialize submission queue batching */
static int init_sqe_batch(struct us_io_uring_internal_context *ctx) {
    ctx->batch.sqes = calloc(MAX_BATCH_SIZE, sizeof(struct io_uring_sqe *));
    if (!ctx->batch.sqes) {
        return -ENOMEM;
    }
    
    ctx->batch.count = 0;
    ctx->batch.submit_ts = 0;
    ctx->batch_timeout_ns = 100000; /* 100 microseconds default */
    
    return 0;
}

/* Initialize statistics tracking */
static void init_statistics(struct us_io_uring_internal_context *ctx) {
    memset(&ctx->stats, 0, sizeof(ctx->stats));
    
    atomic_init(&ctx->stats.sqes_submitted, 0);
    atomic_init(&ctx->stats.sqes_dropped, 0);
    atomic_init(&ctx->stats.batch_count, 0);
    atomic_init(&ctx->stats.cqes_processed, 0);
    atomic_init(&ctx->stats.cqes_overflow, 0);
    atomic_init(&ctx->stats.total_latency_ns, 0);
    atomic_init(&ctx->stats.max_latency_ns, 0);
    atomic_init(&ctx->stats.buffer_exhaustion, 0);
    atomic_init(&ctx->stats.zero_copy_sends, 0);
    
    for (int i = 0; i < US_IO_URING_INTERNAL_ERROR_MAX; i++) {
        atomic_init(&ctx->stats.errors[i], 0);
    }
}

/* Create and initialize io_uring context */
struct us_io_uring_internal_context *us_io_uring_internal_context_create(struct us_io_uring_internal_config *config) {
    pthread_mutex_lock(&g_context_mutex);
    
    /* Return existing context if already initialized */
    if (g_uring_context) {
        atomic_fetch_add(&g_uring_context->refs, 1);
        pthread_mutex_unlock(&g_context_mutex);
        return g_uring_context;
    }
    
    /* Check kernel support */
    if (check_kernel_version() < 0) {
        pthread_mutex_unlock(&g_context_mutex);
        errno = ENOTSUP;
        return NULL;
    }
    
    struct us_io_uring_internal_context *ctx = calloc(1, sizeof(struct us_io_uring_internal_context));
    if (!ctx) {
        pthread_mutex_unlock(&g_context_mutex);
        return NULL;
    }
    
    /* Detect available features */
    ctx->features = detect_features();
    
    /* Set up ring parameters */
    struct io_uring_params params;
    memset(&params, 0, sizeof(params));
    
    uint32_t ring_size = config ? config->ring_size : DEFAULT_RING_SIZE;
    uint32_t cq_entries = config ? config->cq_entries : DEFAULT_CQ_ENTRIES;
    
    /* Enable advanced features if available */
    if (ctx->features & URING_FEAT_COOP_TASKRUN) {
        params.flags |= IORING_SETUP_COOP_TASKRUN;
    }
    if (ctx->features & URING_FEAT_SINGLE_ISSUER) {
        params.flags |= IORING_SETUP_SINGLE_ISSUER;
    }
    
    /* Enable SQPOLL if requested and available */
    if (config && config->enable_sqpoll && (ctx->features & URING_FEAT_SQPOLL)) {
        params.flags |= IORING_SETUP_SQPOLL;
        params.sq_thread_idle = config->sq_thread_idle ?: DEFAULT_SQ_THREAD_IDLE;
        
        if (config->sq_cpu >= 0 && (ctx->features & URING_FEAT_SQ_AFF)) {
            params.flags |= IORING_SETUP_SQ_AFF;
            params.sq_thread_cpu = config->sq_cpu;
            ctx->sq_cpu = config->sq_cpu;
        }
    }
    
    params.cq_entries = cq_entries;
    
    /* Initialize io_uring */
    int ret = io_uring_queue_init_params(ring_size, &ctx->ring, &params);
    if (ret < 0) {
        free(ctx);
        pthread_mutex_unlock(&g_context_mutex);
        errno = -ret;
        return NULL;
    }
    
    /* Register ring fd for faster access */
    io_uring_register_ring_fd(&ctx->ring);
    
    /* Initialize buffer pools */
    ret = us_io_uring_internal_buffer_pool_init(&ctx->ring);
    if (ret < 0) {
        io_uring_queue_exit(&ctx->ring);
        free(ctx);
        pthread_mutex_unlock(&g_context_mutex);
        errno = -ret;
        return NULL;
    }
    
    /* Initialize FD table if supported */
    if (ctx->features & URING_FEAT_REGISTERED_FDS) {
        uint32_t fd_table_size = config ? config->max_fds : 4096;
        init_fd_table(ctx, fd_table_size);
    }
    
    /* Initialize submission batching */
    init_sqe_batch(ctx);
    
    /* Initialize statistics */
    init_statistics(ctx);
    
    /* Initialize synchronization */
    pthread_mutex_init(&ctx->mutex, NULL);
    pthread_spin_init(&ctx->cqe_lock, PTHREAD_PROCESS_PRIVATE);
    atomic_init(&ctx->refs, 1);
    
    /* Store configuration */
    if (config) {
        memcpy(&ctx->config, config, sizeof(struct us_io_uring_internal_config));
    }
    
    g_uring_context = ctx;
    pthread_mutex_unlock(&g_context_mutex);
    
    return ctx;
}

/* Increment reference count */
void us_io_uring_internal_context_ref(struct us_io_uring_internal_context *ctx) {
    if (ctx) {
        atomic_fetch_add(&ctx->refs, 1);
    }
}

/* Decrement reference count and destroy if zero */
void us_io_uring_internal_context_unref(struct us_io_uring_internal_context *ctx) {
    if (!ctx) {
        return;
    }
    
    if (atomic_fetch_sub(&ctx->refs, 1) == 1) {
        us_io_uring_internal_context_destroy(ctx);
    }
}

/* Destroy io_uring context */
void us_io_uring_internal_context_destroy(struct us_io_uring_internal_context *ctx) {
    if (!ctx) {
        return;
    }
    
    pthread_mutex_lock(&g_context_mutex);
    
    /* Wait for all references */
    while (atomic_load(&ctx->refs) > 0) {
        pthread_mutex_unlock(&g_context_mutex);
        usleep(1000);
        pthread_mutex_lock(&g_context_mutex);
    }
    
    /* Clean up buffer pools */
    us_io_uring_internal_buffer_pool_cleanup();
    
    /* Clean up FD table */
    if (ctx->fd_table.registered_fds) {
        io_uring_unregister_files(&ctx->ring);
        free(ctx->fd_table.registered_fds);
        free(ctx->fd_table.fd_map);
        us_io_uring_internal_bitmap_destroy(ctx->fd_table.free_slots);
    }
    
    /* Clean up submission batch */
    free(ctx->batch.sqes);
    
    /* Clean up io_uring */
    io_uring_queue_exit(&ctx->ring);
    
    /* Clean up synchronization */
    pthread_mutex_destroy(&ctx->mutex);
    pthread_spin_destroy(&ctx->cqe_lock);
    
    if (g_uring_context == ctx) {
        g_uring_context = NULL;
    }
    
    free(ctx);
    pthread_mutex_unlock(&g_context_mutex);
}

/* Register a file descriptor for zero-copy operations */
int us_io_uring_internal_register_fd(struct us_io_uring_internal_context *ctx, int fd) {
    if (!ctx || !(ctx->features & URING_FEAT_REGISTERED_FDS)) {
        return fd; /* Return original FD if registration not supported */
    }
    
    pthread_mutex_lock(&ctx->mutex);
    
    /* Check if already registered */
    if (ctx->fd_table.fd_map[fd] > 0) {
        pthread_mutex_unlock(&ctx->mutex);
        return ctx->fd_table.fd_map[fd] - 1;
    }
    
    /* Find a free slot */
    int slot = us_io_uring_internal_bitmap_find_first_set(ctx->fd_table.free_slots, ctx->fd_table.capacity);
    if (slot < 0) {
        pthread_mutex_unlock(&ctx->mutex);
        return fd; /* Table full, use regular FD */
    }
    
    /* Register the FD */
    int ret = io_uring_register_files_update(&ctx->ring, slot, &fd, 1);
    if (ret < 0) {
        pthread_mutex_unlock(&ctx->mutex);
        return fd;
    }
    
    /* Update tables */
    ctx->fd_table.registered_fds[slot] = fd;
    ctx->fd_table.fd_map[fd] = slot + 1; /* +1 to distinguish from unregistered */
    us_io_uring_internal_bitmap_clear(ctx->fd_table.free_slots, slot);
    
    pthread_mutex_unlock(&ctx->mutex);
    return slot;
}

/* Unregister a file descriptor */
void us_io_uring_internal_unregister_fd(struct us_io_uring_internal_context *ctx, int fd) {
    if (!ctx || !(ctx->features & URING_FEAT_REGISTERED_FDS)) {
        return;
    }
    
    pthread_mutex_lock(&ctx->mutex);
    
    uint32_t slot_plus_one = ctx->fd_table.fd_map[fd];
    if (slot_plus_one == 0) {
        pthread_mutex_unlock(&ctx->mutex);
        return; /* Not registered */
    }
    
    int slot = slot_plus_one - 1;
    
    /* Unregister from io_uring */
    int unreg_fd = -1;
    io_uring_register_files_update(&ctx->ring, slot, &unreg_fd, 1);
    
    /* Update tables */
    ctx->fd_table.registered_fds[slot] = -1;
    ctx->fd_table.fd_map[fd] = 0;
    us_io_uring_internal_bitmap_set(ctx->fd_table.free_slots, slot);
    
    pthread_mutex_unlock(&ctx->mutex);
}

/* Get statistics */
void us_io_uring_internal_context_get_stats(struct us_io_uring_internal_context *ctx, struct us_io_uring_internal_stats *stats) {
    if (!ctx || !stats) {
        return;
    }
    
    stats->sqes_submitted = atomic_load(&ctx->stats.sqes_submitted);
    stats->sqes_dropped = atomic_load(&ctx->stats.sqes_dropped);
    stats->batch_count = atomic_load(&ctx->stats.batch_count);
    stats->cqes_processed = atomic_load(&ctx->stats.cqes_processed);
    stats->cqes_overflow = atomic_load(&ctx->stats.cqes_overflow);
    stats->total_latency_ns = atomic_load(&ctx->stats.total_latency_ns);
    stats->max_latency_ns = atomic_load(&ctx->stats.max_latency_ns);
    stats->buffer_exhaustion = atomic_load(&ctx->stats.buffer_exhaustion);
    stats->zero_copy_sends = atomic_load(&ctx->stats.zero_copy_sends);
    
    for (int i = 0; i < US_IO_URING_INTERNAL_ERROR_MAX; i++) {
        stats->errors[i] = atomic_load(&ctx->stats.errors[i]);
    }
    
    /* Get buffer pool stats */
    us_io_uring_internal_buffer_pool_get_stats(&stats->buffer_stats);
}

/* Simple bitmap implementation */
struct us_io_uring_internal_bitmap *us_io_uring_internal_bitmap_create(uint32_t size) {
    struct us_io_uring_internal_bitmap *bm = malloc(sizeof(struct us_io_uring_internal_bitmap));
    if (!bm) return NULL;
    
    bm->size = size;
    bm->words = (size + 63) / 64;
    bm->data = calloc(bm->words, sizeof(uint64_t));
    if (!bm->data) {
        free(bm);
        return NULL;
    }
    
    return bm;
}

void us_io_uring_internal_bitmap_destroy(struct us_io_uring_internal_bitmap *bm) {
    if (bm) {
        free(bm->data);
        free(bm);
    }
}

void us_io_uring_internal_bitmap_set(struct us_io_uring_internal_bitmap *bm, uint32_t bit) {
    if (bm && bit < bm->size) {
        bm->data[bit / 64] |= (1ULL << (bit % 64));
    }
}

void us_io_uring_internal_bitmap_clear(struct us_io_uring_internal_bitmap *bm, uint32_t bit) {
    if (bm && bit < bm->size) {
        bm->data[bit / 64] &= ~(1ULL << (bit % 64));
    }
}

int us_io_uring_internal_bitmap_find_first_set(struct us_io_uring_internal_bitmap *bm, uint32_t max) {
    if (!bm) return -1;
    
    uint32_t limit = (max < bm->size) ? max : bm->size;
    for (uint32_t i = 0; i < limit; i++) {
        if (bm->data[i / 64] & (1ULL << (i % 64))) {
            return i;
        }
    }
    return -1;
}

#endif /* LIBUS_USE_IO_URING */
