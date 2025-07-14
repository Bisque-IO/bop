#include "./mpmc.h"
#include "./queue/blockingconcurrentqueue.h"
#include "./queue/concurrentqueue.h"

struct bop_mpmc {
    moodycamel::ConcurrentQueue<void *> queue{};
    moodycamel::ProducerToken tok{queue};
    moodycamel::ConsumerToken ctok{queue};
};

struct bop_mpmc_blocking {
    moodycamel::BlockingConcurrentQueue<void *> queue{};
};

extern "C" {
BOP_API bop_mpmc *bop_mpmc_create() {
    return new bop_mpmc{};
}

BOP_API void bop_mpmc_destroy(bop_mpmc *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API size_t bop_mpmc_size_approx(bop_mpmc *queue) {
    return queue->queue.size_approx();
}

BOP_API bool bop_mpmc_enqueue(bop_mpmc *queue, void *item) {
    // return queue->queue.enqueue(item);
    void* items[1];
    items[0] = item;
    return queue->queue.try_enqueue_bulk(items, 1);
}

BOP_API bool bop_mpmc_dequeue(bop_mpmc *queue, void **item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API int64_t bop_mpmc_dequeue_bulk(bop_mpmc *queue, void *items[], size_t max_size) {
    return queue->queue.try_dequeue_bulk(items, max_size);
}

BOP_API bop_mpmc_blocking *bop_mpmc_blocking_create() {
    return new bop_mpmc_blocking{};
}

BOP_API void bop_mpmc_blocking_destroy(bop_mpmc_blocking *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API size_t bop_mpmc_blocking_size_approx(bop_mpmc_blocking *queue) {
    return queue->queue.size_approx();
}

BOP_API bool bop_mpmc_blocking_enqueue(bop_mpmc_blocking *queue, void *item) {
    return queue->queue.enqueue(item);
}

BOP_API bool bop_mpmc_blocking_dequeue(bop_mpmc_blocking *queue, void **item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API bool bop_mpmc_blocking_dequeue_wait(bop_mpmc_blocking *queue, void **item, int64_t timeout_micros) {
    return queue->queue.wait_dequeue_timed(*item, timeout_micros);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk(
    bop_mpmc_blocking *queue,
    void *items[],
    size_t max_size
) {
    return queue->queue.try_dequeue_bulk(items, max_size);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait(
    bop_mpmc_blocking *queue,
    void *items[],
    size_t max_size,
    int64_t timeout_micros
) {
    return queue->queue.wait_dequeue_bulk_timed(items, max_size, timeout_micros);
}
}
