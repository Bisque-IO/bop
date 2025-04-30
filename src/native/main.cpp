#include <snmalloc/snmalloc.h>
#include <snmalloc/pal/pal_consts.h>
#include <snmalloc/mem/sizeclasstable.h>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <memory>

extern "C" {
//    #define LIBMDBX_API BOP_API
//    #define LIBMDBX_EXPORTS BOP_API
//    #define SQLITE_API BOP_API
#include <lmdb.h>
#include <mdbx.h>
#include <sqlite3.h>

#ifdef _WIN32
#define BOP_API __declspec(dllexport)
#else
#define BOP_API
#endif
BOP_API void bop_hello() {}

BOP_API void *bop_alloc(size_t size) {
    return snmalloc::libc::malloc(size);
}

BOP_API void *bop_zalloc(size_t size) {
    return snmalloc::ThreadAlloc::get().alloc<snmalloc::ZeroMem::YesZero>(size);
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

BOP_API void *bop_realloc(void* p, size_t new_size) {
    return snmalloc::libc::realloc(p, new_size);
}

BOP_API void bop_dealloc(void *p) {
    snmalloc::libc::free(p);
}

BOP_API void bop_dealloc_sized(void *p, size_t size) {
    snmalloc::libc::free_sized(p, size);
}

BOP_API int bop_mdbx_env_open(MDBX_env* env, const char* pathname, MDBX_env_flags_t flags, int mode) {
    return mdbx_env_open(env, pathname, flags, (mdbx_mode_t)mode);
}

BOP_API int bop_mdbx_get(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    void* key,
    size_t key_size,
    void** data,
    size_t *data_size
) {
    MDBX_val k{key, key_size};
    MDBX_val v{nullptr, 0};
    int err = mdbx_get(txn, dbi, &k, &v);
    *data = v.iov_base;
    *data_size = v.iov_len;
    return err;
}

BOP_API int bop_mdbx_cursor_get(
    MDBX_cursor* cur,
    void* key,
    size_t key_size,
    void** data,
    size_t *data_size,
    MDBX_cursor_op op
) {
    MDBX_val k{key, key_size};
    MDBX_val v{nullptr, 0};
    int err = mdbx_cursor_get(cur, &k, &v, op);
    *data = v.iov_base;
    *data_size = v.iov_len;
    return err;
}

BOP_API int bop_mdbx_put(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    void* key,
    size_t key_size,
    void** data,
    size_t* data_size,
    MDBX_put_flags_t flags
) {
    MDBX_val k{key, key_size};
    MDBX_val v{*data, *data_size};
    int err = mdbx_put(txn, dbi, &k, &v, flags);
    *data = v.iov_base;
    *data_size = v.iov_len;
    return err;
}

BOP_API int bop_mdbx_cursor_put(
    MDBX_cursor* cur,
    void* key,
    size_t key_size,
    void** value,
    size_t* value_size,
    MDBX_put_flags_t flags
) {
    MDBX_val k{key, key_size};
    MDBX_val v{*value, *value_size};
    int err = mdbx_cursor_put(cur, &k, &v, flags);
    *value = v.iov_base;
    *value_size = v.iov_len;
    return err;
}

BOP_API int bop_mdbx_del(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    void* key,
    size_t key_size,
    void** data,
    size_t* data_size
) {
    MDBX_val k{key, key_size};
    MDBX_val v{nullptr, 0};
    int err = mdbx_del(txn, dbi, &k, &v);
    *data = v.iov_base;
    *data_size = v.iov_len;
    return err;
}


BOP_API size_t bop_MDBX_val_iov_base_offset() { return offsetof(MDBX_val, iov_base); }
BOP_API size_t bop_MDBX_val_iov_len_offset() { return offsetof(MDBX_val, iov_len); }
BOP_API size_t bop_MDBX_val_size() { return sizeof(MDBX_val); }
BOP_API int bop_ENODATA() { return MDBX_ENODATA; }
BOP_API int bop_EINVAL() { return MDBX_EINVAL; }
BOP_API int bop_EACCESS() { return MDBX_EACCESS; }
BOP_API int bop_ENOMEM() { return MDBX_ENOMEM; }
BOP_API int bop_EROFS() { return MDBX_EROFS; }
BOP_API int bop_ENOSYS() { return MDBX_ENOSYS; }
BOP_API int bop_EIO() { return MDBX_EIO; }
BOP_API int bop_EPERM() { return MDBX_EPERM; }
BOP_API int bop_EINTR() { return MDBX_EINTR; }
BOP_API int bop_ENOFILE() { return MDBX_ENOFILE; }
BOP_API int bop_EREMOTE() { return MDBX_EREMOTE; }
BOP_API int bop_EDEADLK() { return MDBX_EDEADLK; }
}