package concurrent

import "base:intrinsics"
import "base:runtime"
import "core:sync"

import "../snmalloc"
import time "core:time"

main :: proc() {}

_Map_Shard :: struct($K: typeid, $V: typeid) {
    allocator: runtime.Allocator,
    mu:        sync.Mutex,
    m:         map[K]V
}

Map :: struct($K: typeid, $V: typeid, $SHARDS: int) {
    allocator: runtime.Allocator,
    hasher: proc "contextless" (data: rawptr, seed: uintptr) -> uintptr,
    seed: uintptr,
    shards: [SHARDS]_Map_Shard(K, V),
}

_map_shard :: #force_inline proc "contextless" (self: ^Map($K, $V, $S), key: K) -> ^_Map_Shard(K, V) {
    MASK :: S - 1
    key := key
    return &self.shards[uint(self.hasher(rawptr(&key), self.seed)) & uint(MASK)]
}

map_replace :: proc(self: ^Map($K, $V, $S), key: K, value: V) -> (prev_val: V, has_prev: bool) {
    shard := _map_shard(self, key)
    sync.mutex_guard(&shard.mu)
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
    sync.mutex_guard(&shard.mu)
    if shard.m == nil {
        shard.m = make(map[K]V, self.allocator)
    }
    shard.m[key] = value
}

map_remove :: proc(self: ^Map($K, $V, $S), key: K) -> (prev_val: V, has_prev: bool) {
    shard := _map_shard(self, key)
    sync.mutex_guard(&shard.mu)
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
    sync.mutex_guard(&shard.mu)
    if shard.m == nil {
        return
    }
    value, ok = shard.m[key]
    return
}

map_destroy :: proc(self: ^Map($K, $V, $S)) {
    for &shard in self.shards {
        sync.mutex_guard(&shard.mu)
        delete_map(shard.m)
        shard.m = nil
    }
}

import "core:testing"
import "core:fmt"

@test
test_concurrent_map :: proc(t: ^testing.T) {
    time.sleep(time.Millisecond*1)
    cm := Map(u64, u64, 32){
        allocator = context.allocator,
        hasher = intrinsics.type_hasher_proc(u64),
    }
    defer map_destroy(&cm)
    map_put(&cm, 1, 2)
    v, ok := map_get(&cm, 1)
    fmt.println(v, ok)
    v, ok = map_remove(&cm, 1)
    fmt.println(v, ok)
    v, ok = map_get(&cm, 1)
    fmt.println(v, ok)
    map_put(&cm, 1, 2)
}