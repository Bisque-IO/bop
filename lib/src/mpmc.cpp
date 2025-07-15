#include "./mpmc.h"

#include <queue>

#include "./queue/blockingconcurrentqueue.h"
#include "./queue/concurrentqueue.h"

struct bop_mpmc {
    moodycamel::ConcurrentQueue<uint64_t> queue{};
};

struct bop_mpmc_blocking {
    moodycamel::BlockingConcurrentQueue<uint64_t> queue{};
};

struct bop_mpmc_producer_token {
    moodycamel::ConcurrentQueue<uint64_t> *queue;
    moodycamel::ProducerToken token;

    explicit bop_mpmc_producer_token(moodycamel::ConcurrentQueue<uint64_t> *queue)
        : queue(queue), token(*queue) {
    }
};

struct bop_mpmc_consumer_token {
    moodycamel::ConcurrentQueue<uint64_t> *queue;
    moodycamel::ConsumerToken token;

    explicit bop_mpmc_consumer_token(moodycamel::ConcurrentQueue<uint64_t> *queue)
        : queue(queue), token(*queue) {
    }
};

struct bop_mpmc_blocking_producer_token {
    moodycamel::BlockingConcurrentQueue<uint64_t> *queue;
    moodycamel::ProducerToken token;

    explicit bop_mpmc_blocking_producer_token(moodycamel::BlockingConcurrentQueue<uint64_t> *queue)
        : queue(queue), token(*queue) {
    }
};

struct bop_mpmc_blocking_consumer_token {
    moodycamel::BlockingConcurrentQueue<uint64_t> *queue;
    moodycamel::ConsumerToken token;

    explicit bop_mpmc_blocking_consumer_token(moodycamel::BlockingConcurrentQueue<uint64_t> *queue)
        : queue(queue), token(*queue) {
    }
};

extern "C" {
BOP_API bop_mpmc *bop_mpmc_create() {
    return new bop_mpmc{};
}

BOP_API void bop_mpmc_destroy(bop_mpmc *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API bop_mpmc_producer_token *bop_mpmc_create_producer_token(bop_mpmc *queue) {
    return new bop_mpmc_producer_token{&queue->queue};
}

BOP_API void bop_mpmc_destroy_producer_token(bop_mpmc_producer_token *token) {
    delete token;
}

BOP_API bop_mpmc_consumer_token *bop_mpmc_create_consumer_token(bop_mpmc *queue) {
    return new bop_mpmc_consumer_token{&queue->queue};
}

BOP_API void bop_mpmc_destroy_consumer_token(bop_mpmc_consumer_token *token) {
    delete token;
}

BOP_API size_t bop_mpmc_size_approx(bop_mpmc *queue) {
    return queue->queue.size_approx();
}

BOP_API bool bop_mpmc_enqueue(bop_mpmc *queue, uint64_t item) {
    return queue->queue.enqueue(item);
}

BOP_API bool bop_mpmc_enqueue_token(struct bop_mpmc_producer_token *token, uint64_t item) {
    return token->queue->enqueue(token->token, item);
}

BOP_API bool bop_mpmc_enqueue_bulk(struct bop_mpmc *token, uint64_t items[], size_t size) {
    return token->queue.enqueue_bulk(items, size);
}

BOP_API bool bop_mpmc_enqueue_bulk_token(struct bop_mpmc_producer_token *token, uint64_t items[], size_t size) {
    return token->queue->enqueue_bulk(token->token, items, size);
}

BOP_API bool bop_mpmc_try_enqueue_bulk(struct bop_mpmc *queue, uint64_t items[], size_t size) {
    return queue->queue.try_enqueue_bulk(items, size);
}

BOP_API bool bop_mpmc_try_enqueue_bulk_token(struct bop_mpmc_producer_token *token, uint64_t items[], size_t size) {
    return token->queue->try_enqueue_bulk(token->token, items, size);
}

BOP_API bool bop_mpmc_dequeue(bop_mpmc *queue, uint64_t *item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API bool bop_mpmc_dequeue_token(bop_mpmc_consumer_token *token, uint64_t *item) {
    return token->queue->try_dequeue(token->token, *item);
}

BOP_API int64_t bop_mpmc_dequeue_bulk(bop_mpmc *queue, size_t items[], uint64_t max_size) {
    return queue->queue.try_dequeue_bulk(items, max_size);
}

BOP_API int64_t bop_mpmc_dequeue_bulk_token(bop_mpmc_consumer_token *token, size_t items[], uint64_t max_size) {
    return token->queue->try_dequeue_bulk(token->token, items, max_size);
}

BOP_API bop_mpmc_blocking *bop_mpmc_blocking_create() {
    return new bop_mpmc_blocking{};
}

BOP_API void bop_mpmc_blocking_destroy(bop_mpmc_blocking *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API bop_mpmc_blocking_producer_token *bop_mpmc_blocking_create_producer_token(bop_mpmc_blocking *queue) {
    return new bop_mpmc_blocking_producer_token{&queue->queue};
}

BOP_API void bop_mpmc_blocking_destroy_producer_token(bop_mpmc_blocking_producer_token *token) {
    delete token;
}

BOP_API bop_mpmc_blocking_consumer_token *bop_mpmc_blocking_create_consumer_token(bop_mpmc_blocking *queue) {
    return new bop_mpmc_blocking_consumer_token{&queue->queue};
}

BOP_API void bop_mpmc_blocking_destroy_consumer_token(bop_mpmc_blocking_consumer_token *token) {
    delete token;
}

BOP_API size_t bop_mpmc_blocking_size_approx(bop_mpmc_blocking *queue) {
    return queue->queue.size_approx();
}

BOP_API bool bop_mpmc_blocking_enqueue(bop_mpmc_blocking *queue, uint64_t item) {
    return queue->queue.enqueue(item);
}

BOP_API bool bop_mpmc_blocking_enqueue_token(bop_mpmc_blocking_producer_token *queue, uint64_t item) {
    return queue->queue->enqueue(item);
}

BOP_API bool bop_mpmc_blocking_enqueue_bulk(bop_mpmc_blocking *queue, uint64_t items[], size_t size) {
    return queue->queue.enqueue_bulk(items, size);
}

BOP_API bool bop_mpmc_blocking_enqueue_bulk_token(bop_mpmc_blocking_producer_token *token, uint64_t items[],
                                                  size_t size) {
    return token->queue->enqueue_bulk(token->token, items, size);
}

BOP_API bool bop_mpmc_blocking_try_enqueue_bulk(bop_mpmc_blocking *queue, uint64_t items[], size_t size) {
    return queue->queue.try_enqueue_bulk(items, size);
}

BOP_API bool bop_mpmc_blocking_try_enqueue_bulk_token(bop_mpmc_blocking_producer_token *token, uint64_t items[],
                                                      size_t size) {
    return token->queue->try_enqueue_bulk(token->token, items, size);
}

BOP_API bool bop_mpmc_blocking_dequeue(bop_mpmc_blocking *queue, uint64_t *item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API bool bop_mpmc_blocking_dequeue_token(bop_mpmc_blocking_consumer_token *token, uint64_t *item) {
    return token->queue->try_dequeue(token->token, *item);
}

BOP_API bool bop_mpmc_blocking_dequeue_wait(bop_mpmc_blocking *queue, uint64_t *item, int64_t timeout_micros) {
    return queue->queue.wait_dequeue_timed(*item, timeout_micros);
}

BOP_API bool bop_mpmc_blocking_dequeue_wait_token(bop_mpmc_blocking_consumer_token *token, uint64_t *item,
                                                  int64_t timeout_micros) {
    return token->queue->wait_dequeue_timed(token->token, *item, timeout_micros);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk(
    bop_mpmc_blocking *queue,
    uint64_t items[],
    size_t max_size
) {
    return queue->queue.try_dequeue_bulk(items, max_size);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_token(
    bop_mpmc_blocking_consumer_token *token,
    uint64_t items[],
    size_t max_size
) {
    return token->queue->try_dequeue_bulk(token->token, items, max_size);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait(
    bop_mpmc_blocking *queue,
    uint64_t items[],
    size_t max_size,
    int64_t timeout_micros
) {
    return queue->queue.wait_dequeue_bulk_timed(items, max_size, timeout_micros);
}

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait_token(
    bop_mpmc_blocking_consumer_token *token,
    uint64_t items[],
    size_t max_size,
    int64_t timeout_micros
) {
    return token->queue->wait_dequeue_bulk_timed(token->token, items, max_size, timeout_micros);
}
}
