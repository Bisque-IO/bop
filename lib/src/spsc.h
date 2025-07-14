#ifndef BOP_SPSC_H
#define BOP_SPSC_H

#include "./lib.h"

#ifdef __cplusplus
extern "C" {
#endif

    struct bop_spsc;
    struct bop_spsc_blocking;

    BOP_API struct bop_spsc *bop_spsc_create();

    BOP_API void bop_spsc_destroy(struct bop_spsc *queue);

    BOP_API size_t bop_spsc_size_approx(struct bop_spsc *queue);

    BOP_API size_t bop_spsc_max_capacity(struct bop_spsc *queue);

    BOP_API bool bop_spsc_enqueue(struct bop_spsc *queue, void* item);

    BOP_API bool bop_spsc_dequeue(struct bop_spsc *queue, void **item);

    BOP_API struct bop_spsc_blocking *bop_spsc_blocking_create();

    BOP_API void bop_spsc_blocking_destroy(struct bop_spsc_blocking *queue);

    BOP_API size_t bop_spsc_blocking_size_approx(struct bop_spsc_blocking *queue);

    BOP_API size_t bop_spsc_blocking_max_capacity(struct bop_spsc_blocking *queue);

    BOP_API bool bop_spsc_blocking_enqueue(struct bop_spsc_blocking *queue, void* item);

    BOP_API bool bop_spsc_blocking_dequeue(struct bop_spsc_blocking *queue, void **item);

    BOP_API bool bop_spsc_blocking_dequeue_wait(struct bop_spsc_blocking *queue, void **item, int64_t timeout_micros);

#ifdef __cplusplus
}
#endif

#endif
