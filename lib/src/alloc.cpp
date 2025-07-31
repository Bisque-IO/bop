#include "./alloc.h"

#include <snmalloc/snmalloc.h>
#include <snmalloc/pal/pal_consts.h>
#include <snmalloc/mem/sizeclasstable.h>
#include <snmalloc/global/libc.h>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <memory>
#include <cstddef>

////////////////////////////////////////////////////////////////////////////////////
/// snmalloc C API
////////////////////////////////////////////////////////////////////////////////////

#ifdef __cplusplus
extern "C" {
#endif
BOP_API void *bop_alloc(size_t size) {
    return snmalloc::libc::malloc(size);
    // return snmalloc::ThreadAlloc::get().alloc<snmalloc::Uninit>(size);
}

BOP_API void *bop_zalloc(size_t size) {
    return snmalloc::libc::calloc(size, 1);
    // return snmalloc::ThreadAlloc::get().alloc<snmalloc::Zero>(size);
}

BOP_API void *bop_calloc(size_t element_size, size_t count) {
    return snmalloc::libc::calloc(element_size, count);
}

BOP_API void *bop_alloc_aligned(size_t alignment, size_t size) {
    return snmalloc::libc::aligned_alloc(alignment, size);
}

BOP_API void *bop_zalloc_aligned(size_t alignment, size_t size) {
    return bop_zalloc(snmalloc::aligned_size(alignment, size));
}

BOP_API void *bop_realloc(void *p, size_t new_size) {
    return snmalloc::libc::realloc(p, new_size);
}

BOP_API void bop_dealloc(void *p) {
    snmalloc::libc::free(p);
}

BOP_API void bop_dealloc_sized(void *p, size_t size) {
    snmalloc::libc::free_sized(p, size);
}

BOP_API void bop_heap_access(void *data, size_t size) {
    *static_cast<uint8_t *>(data) = 5;
}

BOP_API size_t bop_malloc_usable_size(const void *data) {
    return snmalloc::libc::malloc_usable_size(data);
}

struct SP {
    std::shared_ptr<void *> p;
};

BOP_API size_t bop_size_of_shared_ptr() {
    return sizeof(SP);
}
#ifdef __cplusplus
}
#endif
