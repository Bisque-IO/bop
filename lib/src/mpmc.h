#ifndef BOP_MPMC_H
#define BOP_MPMC_H

#include "./lib.h"

#ifdef __cplusplus
extern "C" {
#endif

struct bop_mpmc;
struct bop_mpmc_producer_token;
struct bop_mpmc_consumer_token;
struct bop_mpmc_blocking;
struct bop_mpmc_blocking_producer_token;
struct bop_mpmc_blocking_consumer_token;

BOP_API struct bop_mpmc *bop_mpmc_create();

BOP_API void bop_mpmc_destroy(struct bop_mpmc *queue);

BOP_API bop_mpmc_producer_token *bop_mpmc_create_producer_token(bop_mpmc* queue);

BOP_API void bop_mpmc_destroy_producer_token(bop_mpmc_producer_token* token);

BOP_API bop_mpmc_consumer_token *bop_mpmc_create_consumer_token(bop_mpmc* queue);

BOP_API void bop_mpmc_destroy_consumer_token(bop_mpmc_consumer_token* token);

BOP_API size_t bop_mpmc_size_approx(struct bop_mpmc *queue);

BOP_API bool bop_mpmc_enqueue(struct bop_mpmc *queue, uint64_t item);

BOP_API bool bop_mpmc_enqueue_token(struct bop_mpmc_producer_token* token, uint64_t item);

BOP_API bool bop_mpmc_enqueue_bulk(struct bop_mpmc *token, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_enqueue_bulk_token(struct bop_mpmc_producer_token *queue, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_try_enqueue_bulk(struct bop_mpmc *queue, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_try_enqueue_bulk_token(struct bop_mpmc_producer_token *queue, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_dequeue(struct bop_mpmc *queue, uint64_t *item);

BOP_API bool bop_mpmc_dequeue_token(struct bop_mpmc_consumer_token *queue, uint64_t *item);

BOP_API int64_t bop_mpmc_dequeue_bulk(struct bop_mpmc *queue, uint64_t items[], size_t max_size);

BOP_API int64_t bop_mpmc_dequeue_bulk_token(struct bop_mpmc_consumer_token *queue, uint64_t items[], size_t max_size);



BOP_API struct bop_mpmc_blocking *bop_mpmc_blocking_create();

BOP_API void bop_mpmc_blocking_destroy(struct bop_mpmc_blocking *queue);

BOP_API bop_mpmc_blocking_producer_token *bop_mpmc_blocking_create_producer_token(bop_mpmc_blocking* queue);

BOP_API void bop_mpmc_blocking_destroy_producer_token(bop_mpmc_blocking_producer_token* token);

BOP_API bop_mpmc_blocking_consumer_token *bop_mpmc_blocking_create_consumer_token(bop_mpmc_blocking* queue);

BOP_API void bop_mpmc_blocking_destroy_consumer_token(bop_mpmc_blocking_consumer_token* token);

BOP_API size_t bop_mpmc_blocking_size_approx(struct bop_mpmc_blocking *queue);

BOP_API bool bop_mpmc_blocking_enqueue(struct bop_mpmc_blocking *queue, uint64_t item);

BOP_API bool bop_mpmc_blocking_enqueue_token(struct bop_mpmc_blocking_producer_token *token, uint64_t item);

BOP_API bool bop_mpmc_blocking_enqueue_bulk(struct bop_mpmc_blocking *queue, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_blocking_enqueue_bulk_token(struct bop_mpmc_blocking_producer_token *token, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_blocking_try_enqueue_bulk(struct bop_mpmc_blocking *queue, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_blocking_try_enqueue_bulk_token(struct bop_mpmc_blocking_producer_token *token, uint64_t items[], size_t size);

BOP_API bool bop_mpmc_blocking_dequeue(struct bop_mpmc_blocking *queue, uint64_t *item);

BOP_API bool bop_mpmc_blocking_dequeue_token(struct bop_mpmc_blocking_consumer_token *token, uint64_t *item);

BOP_API bool bop_mpmc_blocking_dequeue_wait(struct bop_mpmc_blocking *queue, uint64_t *item, int64_t timeout_micros);

BOP_API bool bop_mpmc_blocking_dequeue_wait_token(struct bop_mpmc_blocking_consumer_token *token, uint64_t *item, int64_t timeout_micros);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk(struct bop_mpmc_blocking *queue, uint64_t items[], size_t max_size);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_token(struct bop_mpmc_blocking_consumer_token *token, uint64_t items[], size_t max_size);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait(
    struct bop_mpmc_blocking *queue, uint64_t items[], size_t max_size, int64_t timeout_micros);

BOP_API int64_t bop_mpmc_blocking_dequeue_bulk_wait_token(
    struct bop_mpmc_blocking_consumer_token *token, uint64_t items[], size_t max_size, int64_t timeout_micros);

#ifdef __cplusplus
}
#endif

#endif
