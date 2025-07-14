#ifndef BOP_MPMC_H
#define BOP_MPMC_H

#include "./lib.h"

#ifdef __cplusplus
extern "C" {
#endif

struct bop_mpmc;
struct bop_mpmc_blocking;

BOP_API struct bop_mpmc *bop_mpmc_create();

BOP_API void bop_mpmc_destroy(struct bop_mpmc *queue);

BOP_API size_t bop_mpmc_size_approx(struct bop_mpmc *queue);

BOP_API bool bop_mpmc_enqueue(struct bop_mpmc *queue, void* item);

BOP_API bool bop_mpmc_dequeue(struct bop_mpmc *queue, void **item);

BOP_API int64_t bop_mpmc_dequeue_bulk(struct bop_mpmc *queue, void *items[], size_t max_size);

BOP_API struct bop_mpmc_blocking *bop_mpmc_blocking_create();

BOP_API void bop_mpmc_blocking_destroy(struct bop_mpmc_blocking *queue);

BOP_API size_t bop_mpmc_blocking_size_approx(struct bop_mpmc_blocking *queue);

BOP_API bool bop_mpmc_blocking_enqueue(struct bop_mpmc_blocking *queue, void* item);

BOP_API bool bop_mpmc_blocking_dequeue(struct bop_mpmc_blocking *queue, void **item);

BOP_API bool bop_mpmc_blocking_dequeue_wait(struct bop_mpmc_blocking *queue, void **item, int64_t timeout_micros);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk(struct bop_mpmc_blocking *queue, void *items[], size_t max_size);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait(struct bop_mpmc_blocking *queue, void *items[], size_t max_size, int64_t timeout_micros);

#ifdef __cplusplus
}
#endif

#endif
