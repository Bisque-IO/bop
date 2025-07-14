#include "./spsc.h"
#include "./queue/readerwriterqueue.h"

struct bop_spsc {
    moodycamel::ReaderWriterQueue<void *> queue{};
};

struct bop_spsc_blocking {
    moodycamel::BlockingReaderWriterQueue<void *> queue{};
};

extern "C" {
BOP_API bop_spsc *bop_spsc_create() {
    return new bop_spsc{};
}

BOP_API void bop_spsc_destroy(bop_spsc *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API size_t bop_spsc_size_approx(bop_spsc *queue) {
    return queue->queue.size_approx();
}

BOP_API size_t bop_spsc_max_capacity(struct bop_spsc *queue) {
    return queue->queue.max_capacity();
}

BOP_API bool bop_spsc_enqueue(bop_spsc *queue, void *item) {
    return queue->queue.enqueue(item);
}

BOP_API bool bop_spsc_dequeue(bop_spsc *queue, void **item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API bop_spsc_blocking *bop_spsc_blocking_create() {
    return new bop_spsc_blocking{};
}

BOP_API void bop_spsc_blocking_destroy(bop_spsc_blocking *queue) {
    if (!queue) return;
    delete queue;
}

BOP_API size_t bop_spsc_blocking_size_approx(bop_spsc_blocking *queue) {
    return queue->queue.size_approx();
}

BOP_API size_t bop_spsc_blocking_max_capacity(struct bop_spsc_blocking *queue) {
    return queue->queue.max_capacity();
}

BOP_API bool bop_spsc_blocking_enqueue(bop_spsc_blocking *queue, void *item) {
    return queue->queue.enqueue(item);
}

BOP_API bool bop_spsc_blocking_dequeue(bop_spsc_blocking *queue, void **item) {
    return queue->queue.try_dequeue(*item);
}

BOP_API bool bop_spsc_blocking_dequeue_wait(bop_spsc_blocking *queue, void **item, int64_t timeout_micros) {
    return queue->queue.wait_dequeue_timed(*item, timeout_micros);
}
}
