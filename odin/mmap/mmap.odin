package mmap

import "base:runtime"
import c "core:c/libc"
import "core:io"
import "core:os/os2"
import "core:time"

main :: proc() {}

is_path_separator :: os2.is_path_separator
is_absolute_path :: os2.is_absolute_path
get_absolute_path :: os2.get_absolute_path

PAGE_SIZE : int = 0

Error :: os2.Error

File_Type :: os2.File_Type

File_Flag :: os2.File_Flag
File_Flags :: os2.File_Flags

General_Error :: os2.General_Error

IO_Error :: io.Error

Platform_Error :: os2.Platform_Error

Allocator_Error :: runtime.Allocator_Error


Access_Mode :: enum {
    Read,
    Read_Write,
}

Mapping :: struct {
    fd:          FD,
    access:      Access_Mode,
    data:        [^]byte,
    size:        int,
    mapped_size: int,
    mapping_fd:  FD,
    is_internal: bool,
    created:     bool,
}

File_Info :: os2.File_Info

make_offset_page_aligned :: proc(offset: int) -> int {
    page_size := PAGE_SIZE
    return offset / page_size * page_size
}

open :: proc(
    path: string,
    size: int,
    mode: Access_Mode,
    perm: int = 0o777,
    allocator := context.allocator,
) -> (
    m: Mapping,
    err: Error
) {
    file: FD
    created := false

    defer if err != nil {
        if file != nil {
            close(&m)
        }
    }

    info: File_Info
    info, err = os2.stat(path, context.temp_allocator)

    if err != nil {
        if mode == .Read {
            return m, os2.General_Error.Not_Exist
        }

        // create file
        file = _open_file(path, os2.O_RDWR | os2.O_CREATE, perm) or_return

        // allocate file
        _truncate(file, i64(size)) or_return

        created = true
    } else {
        file = _open_file(path, (os2.O_RDWR) if mode == .Read_Write else os2.O_RDONLY, perm) or_return

        if int(info.size) != size {
            _truncate(file, i64(size)) or_return
        }
    }

    m = _map_with_fd(file, 0, size, mode) or_return
    m.created = created
    return m, nil
}

sync :: proc(m: ^Mapping) -> (err: Error) {
    return _sync(m)
}

close :: proc(m: ^Mapping) -> (err: Error) {
    return _close(m)
}