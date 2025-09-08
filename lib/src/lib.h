#ifndef BOP_LIB_H
#define BOP_LIB_H

#ifndef BOP_API
#  ifdef __cplusplus
#    define BOP_EXTERN extern "C"
#  else
#    define BOP_EXTERN
#  endif
#  ifdef _WIN32
#    define BOP_API BOP_EXTERN __declspec(dllexport)
#  else
#    define BOP_API BOP_EXTERN
#  endif
#endif

// #ifdef _WIN32
// #define BOP_API __declspec(dllexport)
// #else
// #define BOP_API
// #endif

#ifdef __cplusplus
extern "C" {
#endif

#include <inttypes.h>
#include <stddef.h>

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

BOP_API size_t bop_size_of_shared_ptr();

#ifdef __cplusplus
}
#endif

#endif
