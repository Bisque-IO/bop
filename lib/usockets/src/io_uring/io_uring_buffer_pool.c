/*
 * High-performance buffer pool implementation for io_uring
 * 
 * This provides efficient buffer management with multiple pools
 * for different size classes to minimize memory waste and improve
 * cache locality.
 */

#ifdef LIBUS_USE_IO_URING

#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <stdatomic.h>
#include <sys/mman.h>
#include <unistd.h>
#include <sched.h>
#include <pthread.h>
#include <errno.h>
#include "liburing.h"
#include "io_uring_buffer_pool.h"

#define ALIGN_UP(x, align) (((x) + (align) - 1) & ~((align) - 1))
#define CACHE_LINE_SIZE 64

/* Buffer pool size classes */
#define NUM_SIZE_CLASSES 5

static const size_t BUFFER_SIZES[NUM_SIZE_CLASSES] = {
    512,    // Small messages
    2048,   // Medium messages  
    4096,   // Standard page size
    16384,  // Large messages
    65536   // Jumbo messages
};

/* Per-CPU buffer cache for reduced contention */
struct us_io_uring_internal_per_cpu_cache {
    struct us_io_uring_internal_buffer_node *head;
    uint32_t count;
    uint32_t max_count;
    char padding[CACHE_LINE_SIZE - sizeof(void*) - sizeof(uint32_t)*2];
} __attribute__((aligned(CACHE_LINE_SIZE)));

struct us_io_uring_internal_buffer_node {
    struct us_io_uring_internal_buffer_node *next;
    uint16_t bid;  // Buffer ID for io_uring
    uint16_t pool_idx;
    uint32_t size;
    void *data;
};

struct us_io_uring_internal_buffer_pool {
    struct io_uring_buf_ring *buf_ring;
    void *base_addr;
    size_t buffer_size;
    uint32_t num_buffers;
    uint32_t gid;  // Buffer group ID
    
    /* Statistics */
    _Atomic(uint64_t) allocations;
    _Atomic(uint64_t) deallocations;
    _Atomic(uint64_t) failed_allocations;
    
    /* Free list management */
    struct us_io_uring_internal_buffer_node *free_list;
    pthread_spinlock_t free_lock;
    
    /* Per-CPU caches */
    struct us_io_uring_internal_per_cpu_cache *cpu_caches;
    int num_cpus;
    
    /* Memory mapped region info */
    void *mmap_addr;
    size_t mmap_size;
    
    /* Reference counting for cleanup */
    atomic_int refs;
};

struct us_io_uring_internal_buffer_pool_manager {
    struct us_io_uring_internal_buffer_pool pools[NUM_SIZE_CLASSES];
    struct io_uring *ring;
    int initialized;
    pthread_mutex_t init_lock;
};

static struct us_io_uring_internal_buffer_pool_manager g_pool_manager = {
    .initialized = 0,
    .init_lock = PTHREAD_MUTEX_INITIALIZER
};

/* Forward declarations */
static void us_io_uring_internal_buffer_pool_destroy(struct us_io_uring_internal_buffer_pool *pool);

/* Get the appropriate buffer pool for a given size */
static inline struct us_io_uring_internal_buffer_pool *get_pool_for_size(size_t size) {
    for (size_t i = 0; i < NUM_SIZE_CLASSES; i++) {
        if (size <= BUFFER_SIZES[i]) {
            return &g_pool_manager.pools[i];
        }
    }
    return NULL;  // Size too large
}

/* Initialize a single buffer pool */
static int init_buffer_pool(struct us_io_uring_internal_buffer_pool *pool, size_t buffer_size, 
                           uint32_t num_buffers, uint32_t gid,
                           struct io_uring *ring) {
    memset(pool, 0, sizeof(*pool));
    
    pool->buffer_size = buffer_size;
    pool->num_buffers = num_buffers;
    pool->gid = gid;
    
    /* Calculate total memory needed */
    size_t total_buffer_size = buffer_size * num_buffers;
    size_t ring_size = sizeof(struct io_uring_buf) * num_buffers;
    pool->mmap_size = ALIGN_UP(total_buffer_size + ring_size, sysconf(_SC_PAGESIZE));
    
    /* Allocate memory using huge pages if available */
    int mmap_flags = MAP_PRIVATE | MAP_ANONYMOUS | MAP_POPULATE;
    #ifdef MAP_HUGETLB
    if (pool->mmap_size >= (2 * 1024 * 1024)) {
        mmap_flags |= MAP_HUGETLB;
    }
    #endif
    
    pool->mmap_addr = mmap(NULL, pool->mmap_size, PROT_READ | PROT_WRITE,
                           mmap_flags, -1, 0);
    if (pool->mmap_addr == MAP_FAILED) {
        /* Fall back to regular pages */
        mmap_flags &= ~MAP_HUGETLB;
        pool->mmap_addr = mmap(NULL, pool->mmap_size, PROT_READ | PROT_WRITE,
                               mmap_flags, -1, 0);
        if (pool->mmap_addr == MAP_FAILED) {
            return -ENOMEM;
        }
    }
    
    /* Set up buffer ring */
    pool->base_addr = pool->mmap_addr;
    struct io_uring_buf_reg reg = {
        .ring_addr = (unsigned long)pool->base_addr,
        .ring_entries = num_buffers,
        .bgid = gid
    };
    
    if (io_uring_register_buf_ring(ring, &reg, 0) < 0) {
        munmap(pool->mmap_addr, pool->mmap_size);
        return -errno;
    }
    
    pool->buf_ring = (struct io_uring_buf_ring *)pool->base_addr;
    io_uring_buf_ring_init(pool->buf_ring);
    
    /* Initialize free list */
    pthread_spin_init(&pool->free_lock, PTHREAD_PROCESS_PRIVATE);
    pool->free_list = NULL;
    
    /* Set up per-CPU caches */
    pool->num_cpus = sysconf(_SC_NPROCESSORS_ONLN);
    pool->cpu_caches = calloc(pool->num_cpus, sizeof(struct us_io_uring_internal_per_cpu_cache));
    if (!pool->cpu_caches) {
        munmap(pool->mmap_addr, pool->mmap_size);
        return -ENOMEM;
    }
    
    /* Pre-populate per-CPU caches */
    for (int i = 0; i < pool->num_cpus; i++) {
        pool->cpu_caches[i].max_count = 32;  // Cache up to 32 buffers per CPU
    }
    
    /* Add all buffers to io_uring buffer ring */
    uint8_t *buf_addr = (uint8_t *)pool->base_addr + ring_size;
    for (uint32_t i = 0; i < num_buffers; i++) {
        io_uring_buf_ring_add(pool->buf_ring, 
                             buf_addr + (i * buffer_size),
                             buffer_size, i,
                             io_uring_buf_ring_mask(num_buffers), i);
    }
    io_uring_buf_ring_advance(pool->buf_ring, num_buffers);
    
    atomic_init(&pool->refs, 1);
    
    return 0;
}

/* Initialize the buffer pool manager */
int us_io_uring_internal_buffer_pool_init(struct io_uring *ring) {
    pthread_mutex_lock(&g_pool_manager.init_lock);
    
    if (g_pool_manager.initialized) {
        pthread_mutex_unlock(&g_pool_manager.init_lock);
        return 0;
    }
    
    g_pool_manager.ring = ring;
    
    /* Initialize pools for each size class */
    for (size_t i = 0; i < NUM_SIZE_CLASSES; i++) {
        size_t buffer_size = BUFFER_SIZES[i];
        uint32_t num_buffers = 1024;  // Start with 1024 buffers per pool
        
        /* Adjust number of buffers based on size */
        if (buffer_size >= 16384) {
            num_buffers = 256;  // Fewer large buffers
        } else if (buffer_size >= 4096) {
            num_buffers = 512;
        }
        
        uint32_t gid = 1000 + i;  // Buffer group IDs start at 1000
        
        int ret = init_buffer_pool(&g_pool_manager.pools[i], 
                                   buffer_size, num_buffers, gid, ring);
        if (ret < 0) {
            /* Clean up already initialized pools */
            for (size_t j = 0; j < i; j++) {
                us_io_uring_internal_buffer_pool_destroy(&g_pool_manager.pools[j]);
            }
            pthread_mutex_unlock(&g_pool_manager.init_lock);
            return ret;
        }
    }
    
    g_pool_manager.initialized = 1;
    pthread_mutex_unlock(&g_pool_manager.init_lock);
    
    return 0;
}

/* Allocate a buffer from the pool */
struct us_io_uring_internal_buffer_handle *us_io_uring_internal_buffer_pool_alloc(size_t size) {
    struct us_io_uring_internal_buffer_pool *pool = get_pool_for_size(size);
    if (!pool) {
        return NULL;
    }
    
    atomic_fetch_add(&pool->allocations, 1);
    
    /* Try per-CPU cache first */
    int cpu = sched_getcpu();
    if (cpu >= 0 && cpu < pool->num_cpus) {
        struct us_io_uring_internal_per_cpu_cache *cache = &pool->cpu_caches[cpu];
        if (cache->head) {
            struct us_io_uring_internal_buffer_node *node = cache->head;
            cache->head = node->next;
            cache->count--;
            
            struct us_io_uring_internal_buffer_handle *handle = malloc(sizeof(*handle));
            if (handle) {
                handle->data = node->data;
                handle->size = pool->buffer_size;
                handle->bid = node->bid;
                handle->pool = pool;
                atomic_fetch_add(&pool->refs, 1);
            }
            free(node);
            return handle;
        }
    }
    
    /* Fall back to global free list */
    pthread_spin_lock(&pool->free_lock);
    if (pool->free_list) {
        struct us_io_uring_internal_buffer_node *node = pool->free_list;
        pool->free_list = node->next;
        pthread_spin_unlock(&pool->free_lock);
        
        struct us_io_uring_internal_buffer_handle *handle = malloc(sizeof(*handle));
        if (handle) {
            handle->data = node->data;
            handle->size = pool->buffer_size;
            handle->bid = node->bid;
            handle->pool = pool;
            atomic_fetch_add(&pool->refs, 1);
        }
        free(node);
        return handle;
    }
    pthread_spin_unlock(&pool->free_lock);
    
    atomic_fetch_add(&pool->failed_allocations, 1);
    return NULL;
}

/* Return a buffer to the pool */
void us_io_uring_internal_buffer_pool_free(struct us_io_uring_internal_buffer_handle *handle) {
    if (!handle || !handle->pool) {
        return;
    }
    
    struct us_io_uring_internal_buffer_pool *pool = handle->pool;
    atomic_fetch_add(&pool->deallocations, 1);
    
    /* Try to return to per-CPU cache */
    int cpu = sched_getcpu();
    if (cpu >= 0 && cpu < pool->num_cpus) {
        struct us_io_uring_internal_per_cpu_cache *cache = &pool->cpu_caches[cpu];
        if (cache->count < cache->max_count) {
            struct us_io_uring_internal_buffer_node *node = malloc(sizeof(*node));
            if (node) {
                node->data = handle->data;
                node->bid = handle->bid;
                node->pool_idx = handle->pool - g_pool_manager.pools;
                node->size = handle->size;
                node->next = cache->head;
                cache->head = node;
                cache->count++;
                
                atomic_fetch_sub(&pool->refs, 1);
                free(handle);
                return;
            }
        }
    }
    
    /* Return to global free list */
    struct us_io_uring_internal_buffer_node *node = malloc(sizeof(*node));
    if (node) {
        node->data = handle->data;
        node->bid = handle->bid;
        node->pool_idx = handle->pool - g_pool_manager.pools;
        node->size = handle->size;
        
        pthread_spin_lock(&pool->free_lock);
        node->next = pool->free_list;
        pool->free_list = node;
        pthread_spin_unlock(&pool->free_lock);
    }
    
    /* Return buffer to io_uring */
    io_uring_buf_ring_add(pool->buf_ring, handle->data, handle->size,
                         handle->bid, io_uring_buf_ring_mask(pool->num_buffers), 0);
    io_uring_buf_ring_advance(pool->buf_ring, 1);
    
    atomic_fetch_sub(&pool->refs, 1);
    free(handle);
}

/* Destroy a buffer pool */
static void us_io_uring_internal_buffer_pool_destroy(struct us_io_uring_internal_buffer_pool *pool) {
    if (!pool || !pool->mmap_addr) {
        return;
    }
    
    /* Wait for all references to be released */
    while (atomic_load(&pool->refs) > 1) {
        usleep(1000);  // Wait 1ms
    }
    
    /* Free per-CPU caches */
    if (pool->cpu_caches) {
        for (int i = 0; i < pool->num_cpus; i++) {
            struct us_io_uring_internal_buffer_node *node = pool->cpu_caches[i].head;
            while (node) {
                struct us_io_uring_internal_buffer_node *next = node->next;
                free(node);
                node = next;
            }
        }
        free(pool->cpu_caches);
    }
    
    /* Free global free list */
    struct us_io_uring_internal_buffer_node *node = pool->free_list;
    while (node) {
        struct us_io_uring_internal_buffer_node *next = node->next;
        free(node);
        node = next;
    }
    
    pthread_spin_destroy(&pool->free_lock);
    
    /* Unregister from io_uring */
    if (g_pool_manager.ring) {
        io_uring_unregister_buf_ring(g_pool_manager.ring, pool->gid);
    }
    
    /* Unmap memory */
    munmap(pool->mmap_addr, pool->mmap_size);
    
    memset(pool, 0, sizeof(*pool));
}

/* Get statistics for monitoring */
void us_io_uring_internal_buffer_pool_get_stats(struct us_io_uring_internal_buffer_pool_stats *stats) {
    if (!stats) {
        return;
    }
    
    memset(stats, 0, sizeof(*stats));
    
    for (size_t i = 0; i < NUM_SIZE_CLASSES; i++) {
        struct us_io_uring_internal_buffer_pool *pool = &g_pool_manager.pools[i];
        stats->total_allocations += atomic_load(&pool->allocations);
        stats->total_deallocations += atomic_load(&pool->deallocations);
        stats->total_failed_allocations += atomic_load(&pool->failed_allocations);
        stats->total_buffers += pool->num_buffers;
        stats->total_memory += pool->mmap_size;
    }
}

/* Cleanup all buffer pools */
void us_io_uring_internal_buffer_pool_cleanup(void) {
    pthread_mutex_lock(&g_pool_manager.init_lock);
    
    if (!g_pool_manager.initialized) {
        pthread_mutex_unlock(&g_pool_manager.init_lock);
        return;
    }
    
    for (size_t i = 0; i < NUM_SIZE_CLASSES; i++) {
        us_io_uring_internal_buffer_pool_destroy(&g_pool_manager.pools[i]);
    }
    
    g_pool_manager.initialized = 0;
    pthread_mutex_unlock(&g_pool_manager.init_lock);
}

#endif /* LIBUS_USE_IO_URING */
