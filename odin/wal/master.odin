package wal

import "../mdbx"
import "core:sync"

Master :: struct {
	env: ^mdbx.Env,
}

Sharded_Map :: struct($K: typeid, $V: typeid, $SHARDS: int) {
	shards: [SHARDS]struct {
		allocator: runtime.Allocator,
		mu:        sync.Mutex,
		m:         map[K]V,
	},
}
