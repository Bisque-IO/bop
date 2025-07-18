package concurrent

import "base:intrinsics"
import "base:runtime"
import "core:sync"

import time "core:time"

_Map_Shard :: struct($K: typeid, $V: typeid) {
    mu:        sync.RW_Mutex,
    m:         map[K]V
}

Map :: struct($K: typeid, $V: typeid, $SHARDS: uintptr) {
    allocator: runtime.Allocator,
    hasher: proc "contextless" (data: rawptr, seed: uintptr) -> uintptr,
    seed: uintptr,
    shards: [SHARDS]_Map_Shard(K, V),
}

map_create :: proc($K: typeid, $V: typeid, $S: uintptr, allocator := context.allocator) -> (
    m: ^Map(K, V, S),
    err: runtime.Allocator_Error,
) {
    m = new(Map(K, V, S), allocator) or_return
    m.allocator = allocator
    m.hasher = intrinsics.type_hasher_proc(K)
    m.seed = 0
    return
}

map_destroy :: proc(self: ^Map($K, $V, $S)) {
    for &shard in self.shards {
        sync.rw_mutex_guard(&shard.mu)
        delete_map(shard.m)
        shard.m = nil
    }
    free(self, self.allocator)
}

_map_shard :: #force_inline proc "contextless" (self: ^Map($K, $V, $S), key: K) -> ^_Map_Shard(K, V) {
    MASK :: S - 1
    key := key
    return &self.shards[self.hasher(rawptr(&key), self.seed) & MASK]
}

map_replace :: proc(self: ^Map($K, $V, $S), key: K, value: V) -> (prev_val: V, has_prev: bool) {
    shard := _map_shard(self, key)
    sync.rw_mutex_guard(&shard.mu)
    if shard.m == nil {
        shard.m = make(map[K]V, self.allocator)
        shard.m[key] = value
    } else {
        if key in shard.m {
            prev_val = shard.m[key]
            has_prev = true
        }
        shard.m[key] = value
    }
}

map_put :: proc(self: ^Map($K, $V, $S), key: K, value: V) {
    shard := _map_shard(self, key)
    sync.rw_mutex_guard(&shard.mu)
    if shard.m == nil {
        shard.m = make(map[K]V, self.allocator)
    }
    shard.m[key] = value
}

map_remove :: proc(self: ^Map($K, $V, $S), key: K) -> (prev_val: V, has_prev: bool) {
    shard := _map_shard(self, key)
    sync.rw_mutex_guard(&shard.mu)
    if shard.m == nil {
        return
    }
    if key in shard.m {
        prev_val = shard.m[key]
        has_prev = true
        delete_key(&shard.m, key)
    }
    return
}

map_get :: proc(self: ^Map($K, $V, $S), key: K) -> (value: V, ok: bool) {
    shard := _map_shard(self, key)
    sync.rw_mutex_shared_guard(&shard.mu)
    if shard.m == nil {
        return
    }
    value, ok = shard.m[key]
    return
}

map_contains :: proc(self: ^Map($K, $V, $S), key: K) -> bool {
    shard := _map_shard(self, key)
    sync.rw_mutex_shared_guard(&shard.mu)
    if shard.m == nil {
        return false
    }
    return key in shard.m
}
