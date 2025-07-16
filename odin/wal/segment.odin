package wal

import "base:intrinsics"
import "base:runtime"
import "../fs"

Segment :: struct {
    allocator: runtime.Allocator,
    name:         string,
    mmap:         fs.MMAP,
    lsn:          u64le,
    gsn:          u64le,
    cnt:          u32,
    size:         u32,
    durable_size: u32,
}

segment_append :: proc(
    gsn: u64le,
    data: []byte
) -> (lsn: u64le, offset: u32le, ok: bool) {
    return
}

import "core:testing"
import "core:fmt"

@test
test_segment :: proc(t: ^testing.T) {

}