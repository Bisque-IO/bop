package wal

import "base:intrinsics"
import "base:runtime"

import "core:sync"

import "../fs"
import "../mdbx"

/*
Segment management for a single WAL.
*/
Segment_Store :: struct {
	wal:          ^WAL,
	env:          ^mdbx.Env,
	archive_path: string,
}

segment_store_create :: proc(wal: ^WAL) {

}

import "core:fmt"
import "core:testing"

@(test)
test_segment_store :: proc(t: ^testing.T) {
	ss := Segment_Store{}

	fmt.println(ss)
}
