package mmap

import "base:runtime"
import "core:fmt"
import "core:testing"

@test
test_mmap :: proc(t: ^testing.T) {
    fmt.println(PAGE_SIZE)

    defer runtime.free_all(context.temp_allocator)

    m, err := open("odin/mmap/test-bin/data.txt", PAGE_SIZE*8, .Read_Write)
    ensure(err == nil)

    m.data[0] = '1'

    sync(&m)

    close(&m)
}
