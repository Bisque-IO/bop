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

BOP_API void bop_heap_access(void* data, size_t size) {
    *static_cast<uint8_t*>(data) = 5;
}

BOP_API int bop_mdbx_env_open(MDBX_env* env, const char* pathname, MDBX_env_flags_t flags, int mode) {
    return mdbx_env_open(env, pathname, flags, static_cast<mdbx_mode_t>(mode));
}


////////////////////////////////////////////////////////////////////////////////////
/// Environment GET operations
////////////////////////////////////////////////////////////////////////////////////

BOP_API int bop_mdbx_get(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* found_key,
    MDBX_val* data
) {
    MDBX_val k{(key+key_offset), key_size};
    int err = mdbx_get(txn, dbi, &k, data);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

BOP_API int bop_mdbx_get_int(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    MDBX_val* found_key,
    MDBX_val* data
) {
    MDBX_val k{static_cast<void*>(&key), 8};
    int err = mdbx_get(txn, dbi, &k, data);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

BOP_API int bop_mdbx_get_greater_or_equal(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* found_key,
    MDBX_val* data
) {
    MDBX_val k{(key+key_offset), key_size};
    int err = mdbx_get_equal_or_great(txn, dbi, &k, data);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

BOP_API int bop_mdbx_get_greater_or_equal_int(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    MDBX_val* found_key,
    MDBX_val* data
) {
    MDBX_val k{static_cast<void*>(&key), 8};
    int err = mdbx_get_equal_or_great(txn, dbi, &k, data);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

////////////////////////////////////////////////////////////////////////////////////
/// Cursor GET operations
////////////////////////////////////////////////////////////////////////////////////

BOP_API int bop_mdbx_cursor_get(
    MDBX_cursor* cur,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* found_key,
    MDBX_val* data,
    MDBX_cursor_op op
) {
    MDBX_val k{(key+key_offset), key_size};
    int err = mdbx_cursor_get(cur, &k, data, op);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

BOP_API int bop_mdbx_cursor_get_int(
    MDBX_cursor* cur,
    uint64_t key,
    MDBX_val* found_key,
    MDBX_val* data,
    MDBX_cursor_op op
) {
    MDBX_val k{static_cast<void*>(&key), 8};
    int err = mdbx_cursor_get(cur, &k, data, op);
    if (err != MDBX_SUCCESS) return err;
    *found_key = k;
    return err;
}

////////////////////////////////////////////////////////////////////////////////////
/// Environment PUT operations
////////////////////////////////////////////////////////////////////////////////////

BOP_API int bop_mdbx_put(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{key+key_offset, key_size};
    MDBX_val v{data+data_offset, data_size};
    int err = mdbx_put(txn, dbi, &k, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_put2(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{(key+key_offset), key_size};
    return mdbx_put(txn, dbi, &k, data, flags);
}

BOP_API int bop_mdbx_put3(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    MDBX_val* key,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val v{data+data_offset, data_size};
    int err = mdbx_put(txn, dbi, key, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_put_int(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{&key, 8};
    MDBX_val v{data+data_offset, data_size};
    int err = mdbx_put(txn, dbi, &k, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_put_int2(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    MDBX_val* data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{&key, 8};
    return mdbx_put(txn, dbi, &k, data, flags);
}

////////////////////////////////////////////////////////////////////////////////////
/// Environment DEL operations
////////////////////////////////////////////////////////////////////////////////////

BOP_API int bop_mdbx_del(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    void* data,
    size_t data_size
) {
    MDBX_val k{key+key_offset, key_size};
    MDBX_val v{data, data_size};
    return mdbx_del(txn, dbi, &k, data == nullptr ? nullptr : &v);
}

BOP_API int bop_mdbx_del2(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* data
) {
    MDBX_val k{key+key_offset, key_size};
    return mdbx_del(txn, dbi, &k, data);
}

BOP_API int bop_mdbx_del3(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    MDBX_val* key,
    uint8_t* data,
    size_t data_offset,
    size_t data_size
) {
    MDBX_CURRENT
    if (data == nullptr) {
        return mdbx_del(txn, dbi, key, nullptr);
    } else {
        MDBX_val v{data+data_offset, data_size};
        return mdbx_del(txn, dbi, key, &v);
    }
}

BOP_API int bop_mdbx_del_int(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    void* data,
    size_t data_size
) {
    MDBX_val k{&key, 8};
    MDBX_val v{data, data_size};
    return mdbx_del(txn, dbi, &k, data == nullptr ? nullptr : &v);
}

BOP_API int bop_mdbx_del_int2(
    MDBX_txn* txn,
    MDBX_dbi dbi,
    uint64_t key,
    MDBX_val* data
) {
    MDBX_val k{&key, 8};
    return mdbx_del(txn, dbi, &k, data);
}

////////////////////////////////////////////////////////////////////////////////////
/// Cursor PUT operations
////////////////////////////////////////////////////////////////////////////////////

BOP_API int bop_mdbx_cursor_put(
    MDBX_cursor* cur,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{key + key_offset, key_size};
    MDBX_val v{data == nullptr ? nullptr : data + data_offset, data_size};
    int err = mdbx_cursor_put(cur, &k, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_cursor_put2(
    MDBX_cursor* cur,
    uint8_t* key,
    size_t key_offset,
    size_t key_size,
    MDBX_val* data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{key + key_offset, key_size};
    return mdbx_cursor_put(cur, &k, data, flags);
}

BOP_API int bop_mdbx_cursor_put3(
    MDBX_cursor* cur,
    MDBX_val* key,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val v{data == nullptr ? nullptr : data + data_offset, data_size};
    int err = mdbx_cursor_put(cur, key, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_cursor_put_int(
    MDBX_cursor* cur,
    uint64_t key,
    uint8_t* data,
    size_t data_offset,
    size_t data_size,
    MDBX_val* prev_data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{&key, 8};
    MDBX_val v{data == nullptr ? nullptr : data + data_offset, data_size};
    int err = mdbx_cursor_put(cur, &k, &v, flags);
    *prev_data = v;
    return err;
}

BOP_API int bop_mdbx_cursor_put_int2(
    MDBX_cursor* cur,
    uint64_t key,
    MDBX_val* data,
    MDBX_put_flags_t flags
) {
    MDBX_val k{&key, 8};
    return mdbx_cursor_put(cur, &k, data, flags);
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