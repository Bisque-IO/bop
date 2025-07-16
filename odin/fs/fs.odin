package fs

import "base:runtime"
import c "core:c/libc"
import "core:io"
import "core:os/os2"
import "core:path/filepath"
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

O_RDONLY  :: os2.O_RDONLY
O_WRONLY  :: os2.O_WRONLY
O_RDWR    :: os2.O_RDWR
O_APPEND  :: os2.O_APPEND
O_CREATE  :: os2.O_CREATE
O_EXCL    :: os2.O_EXCL
O_SYNC    :: os2.O_SYNC
O_TRUNC   :: os2.O_TRUNC
O_SPARSE  :: os2.O_SPARSE

Access_Mode :: enum {
    Read,
    Read_Write,
}

MMAP :: struct {
    allocator:   runtime.Allocator,
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

open :: proc(name: string, flags: File_Flags, perm: int) -> (fd: FD, err: Error) {
    temp_allocator := TEMP_ALLOCATOR_GUARD({})
    return _open_file(name, flags, perm, temp_allocator)
}

mmap :: proc(
    path: string,
    size: int,
    mode: Access_Mode,
    perm: int = 0o777,
    pre_allocate: bool = false,
    allocator := context.allocator,
) -> (
    m: MMAP,
    err: Error
) {
    if len(path) == 0 {
        return m, General_Error.Invalid_Path
    }
    if size < PAGE_SIZE {
        return m, IO_Error.Short_Buffer
    }

    fd: FD
    created := false

    defer if err != nil {
        if is_valid_handle(fd) {
            close(&m)
        }
    }

    temp_allocator := TEMP_ALLOCATOR_GUARD({})

    info: File_Info
    info, err = os2.stat(path, temp_allocator)

    offset_to_zero := 0
    size_to_zero   := 0

    if err != nil {
        err = nil
        if mode == .Read {
            return m, os2.General_Error.Not_Exist
        }

        // ensure all directories are created
        dir := filepath.dir(path, temp_allocator)
        os2.make_directory_all(dir, perm) or_return

        // create file
        fd = _open_file(path, O_RDWR | O_CREATE, perm, temp_allocator) or_return

        if is_invalid_handle(fd) {
            return m, General_Error.Invalid_File
        }

        // allocate file
        _truncate(fd, i64(size)) or_return

        created = true
        size_to_zero = size
    } else {
        // must be a regular file
        if info.type != .Regular {
            return m, General_Error.Invalid_Dir
        }

        fd = _open_file(path, O_RDWR if mode == .Read_Write else O_RDONLY, perm, temp_allocator) or_return

        if is_invalid_handle(fd) {
            return m, General_Error.Invalid_File
        }

        if int(info.size) != size {
            if int(info.size) < size {
                offset_to_zero = int(info.size)
                size_to_zero = size - int(info.size)
            }
            _truncate(fd, i64(size)) or_return
        }
    }

    m = _mmap(fd, 0, size, mode) or_return
    m.created = created

    if size_to_zero > 0 {
        if pre_allocate {
            // force allocation by zeroing entire file
            runtime.mem_zero(rawptr(&m.data[offset_to_zero]), size_to_zero)
        }

        // flush mmap
        _msync(&m)   or_return
        // flush file
        _fsync(m.fd) or_return
    }

    return m, nil
}

msync :: proc(m: ^MMAP) -> Error {
    return _msync(m)
}

fsync :: proc(fd: FD) -> Error {
    return _fsync(fd)
}

fdatasync :: proc(fd: FD) -> Error {
    return _fdatasync(fd)
}

truncate :: proc(fd: FD, size: i64) -> Error {
    return _truncate(fd, size)
}

close :: proc{
    close_fd,
    close_mmap,
}

close_fd :: proc(fd: FD) -> Error {
    return _close_fd(fd)
}

close_mmap :: proc(m: ^MMAP) -> (err: Error) {
    return _close_mmap(m)
}
