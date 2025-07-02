#include "./hash.h"

#include <snmalloc/snmalloc.h>
#include <snmalloc/pal/pal_consts.h>
#include <snmalloc/mem/sizeclasstable.h>
#include <cstdlib>
#include <cstring>
#include <cstdio>
#include <memory>
#include <cstddef>

#include "../hash/rapidhash.h"
#include "../hash/xxh3.h"

BOP_API uint64_t bop_rapidhash(const uint8_t *data, size_t len) {
    return rapidhash(data, len);
}

BOP_API uint64_t bop_rapidhash_segment(const uint8_t *data, size_t offset, size_t len) {
    return rapidhash(data + offset, len);
}

BOP_API uint64_t bop_xxh3(const uint8_t *data, size_t len) {
    return XXH3_64bits(data, len);
}

BOP_API uint64_t bop_xxh3_segment(const uint8_t *data, size_t offset, size_t len) {
    return XXH3_64bits(data + offset, len);
}

//////////////////////////////////////////////////////////////////////////////////////////
/// XXH3 streaming
//////////////////////////////////////////////////////////////////////////////////////////

BOP_API uint64_t bop_xxh3_alloc() {
    auto state = XXH3_createState();
    XXH3_64bits_reset(state);
    return reinterpret_cast<uint64_t>(state);
}

BOP_API void bop_xxh3_dealloc(void *state) {
    XXH3_freeState(static_cast<XXH3_state_t *>(state));
}

BOP_API void bop_xxh3_update(void *state, const uint8_t *data, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data, len);
}

BOP_API void bop_xxh3_update_segment(void *state, const uint8_t *data, size_t offset, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data + offset, len);
}

BOP_API uint64_t bop_xxh3_update_final(void *state, const uint8_t *data, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data, len);
    return XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
}

BOP_API uint64_t bop_xxh3_update_final_segment(void *state, const uint8_t *data, size_t offset, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data + offset, len);
    return XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
}

BOP_API uint64_t bop_xxh3_update_final_reset(void *state, const uint8_t *data, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data, len);
    uint64_t result = XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
    XXH3_64bits_reset(static_cast<XXH3_state_t *>(state));
    return result;
}

BOP_API uint64_t bop_xxh3_update_final_reset_segment(void *state, const uint8_t *data, size_t offset, size_t len) {
    XXH3_64bits_update(static_cast<XXH3_state_t *>(state), data + offset, len);
    uint64_t result = XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
    XXH3_64bits_reset(static_cast<XXH3_state_t *>(state));
    return result;
}

BOP_API uint64_t bop_xxh3_digest(void *state) {
    return XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
}

BOP_API uint64_t bop_xxh3_digest_reset(void *state) {
    uint64_t result = XXH3_64bits_digest(static_cast<XXH3_state_t *>(state));
    XXH3_64bits_reset(static_cast<XXH3_state_t *>(state));
    return result;
}

BOP_API void bop_xxh3_reset(void *state) {
    XXH3_64bits_reset(static_cast<XXH3_state_t *>(state));
}