#ifndef BOP_ALLOC_H
#define BOP_ALLOC_H

#ifdef __cplusplus
extern "C" {
#endif

#include "./lib.h"

BOP_API void *bop_alloc(size_t size);

BOP_API void *bop_zalloc(size_t size);

BOP_API void *bop_calloc(size_t element_size, size_t count);

BOP_API void *bop_alloc_aligned(size_t alignment, size_t size);

BOP_API void *bop_zalloc_aligned(size_t alignment, size_t size);

BOP_API void *bop_realloc(void *p, size_t new_size);

BOP_API void bop_dealloc(void *p);

BOP_API void bop_dealloc_sized(void *p, size_t size);

BOP_API void bop_heap_access(void *data, size_t size);

BOP_API size_t bop_malloc_usable_size(const void *data);

#ifdef __cplusplus
}
#endif

#endif
