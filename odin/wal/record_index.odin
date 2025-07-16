package wal

import "base:intrinsics"
import "base:runtime"
import "../fs"


Record_Index_Page :: struct {
    ids:     []u64le,
    offsets: []Record_Index_Record,
}

Record_Index_Record :: struct {
    offset: u32le,
    flags:  u32le,
}


Record_Index_Builder :: struct {
    allocator: runtime.Allocator,
    page_size: int,
    root:      Record_Index_Page,
    data:      [dynamic]Record_Index_Page,
}

binary_search_u64 :: proc(slice: []u64, target: u64) -> int {
    count := len(slice);
    low   := 0;
    high  := count - 1;

    for low <= high {
        mid := low + (high - low) / 2;
        val := slice[mid];

        if val == target {
            return mid;
        } else if val < target {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    return -1; // Not found
}

import "core:testing"
import "core:fmt"

@test
test_record_index :: proc(t: ^testing.T) {
    b := Record_Index_Builder{}
    b.page_size = 1024

    fmt.println(b)
}