package fs

import "base:runtime"
import "core:fmt"
import "core:testing"

@test
test_mmap :: proc(t: ^testing.T) {
    fmt.println(PAGE_SIZE)

//    defer runtime.free_all(context.temp_allocator)

    m, err := mmap("odin/fs/test-bin/data.txt", PAGE_SIZE*8, .Read_Write)
    ensure(err == nil)

    m.data[0] = '2'

    err = msync(&m)
    err = fsync(m.fd)

    close(&m)
}
