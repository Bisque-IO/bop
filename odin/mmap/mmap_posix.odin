#+build darwin, freebsd, openbsd, netbsd
package mmap

import "core:sys/posix"
import c "core:c/libc"

FD             :: posix.FD
INVALID_HANDLE :: posix.FD(0)

@(init)
init_module :: proc() {
    PAGE_SIZE = int(posix.sysconf(posix.PAGE_SIZE))
}

_Platform_Error :: posix.Errno

_error_string :: proc(errno: i32) -> string {
    return string(posix.strerror(posix.Errno(errno)))
}

_get_platform_error_from_errno :: proc() -> Error {
    return _get_platform_error_existing(posix.errno())
}

_get_platform_error_existing :: proc(errno: posix.Errno) -> Error {
    #partial switch errno {
    case .EPERM:
        return .Permission_Denied
    case .EEXIST:
        return .Exist
    case .ENOENT:
        return .Not_Exist
    case .ETIMEDOUT:
        return .Timeout
    case .EPIPE:
        return .Broken_Pipe
    case .EBADF:
        return .Invalid_File
    case .ENOMEM:
        return .Out_Of_Memory
    case .ENOSYS:
        return .Unsupported
    case:
        return Platform_Error(errno)
    }
}

_get_platform_error :: proc{
    _get_platform_error_existing,
    _get_platform_error_from_errno,
}

_open :: proc(name: string, flags: File_Flags, perm: int) -> (fd: FD, err: Error) {
    if name == "" {
        err = General_Error.Invalid_Path
        return
    }

    sys_flags := posix.O_Flags{.NOCTTY, .CLOEXEC}

    if .Write in flags {
        if .Read in flags {
            sys_flags += {.RDWR}
        } else {
            sys_flags += {.WRONLY}
        }
    }

    if .Append      in flags { sys_flags += {.APPEND} }
    if .Create      in flags { sys_flags += {.CREAT} }
    if .Excl        in flags { sys_flags += {.EXCL} }
    if .Sync        in flags { sys_flags += {.DSYNC} }
    if .Trunc       in flags { sys_flags += {.TRUNC} }
    if .Inheritable in flags { sys_flags -= {.CLOEXEC} }

    temp_allocator := TEMP_ALLOCATOR_GUARD({})
    cname := clone_to_cstring(name, temp_allocator) or_return

    fd := posix.open(cname, sys_flags, transmute(posix.mode_t)posix._mode_t(perm))
    if fd < 0 {
        err = _get_platform_error()
        return
    }

    return FD(fd), nil
}

_map_with_fd :: proc(
    file_handle: FD,
    offset: int,
    length: int,
    mode: Access_Mode,
) -> (m: Mapping, err: Error) {
    aligned_offset := make_offset_page_aligned(offset)
    length_to_map := offset - aligned_offset + length

    max_file_size := i64(offset + length)
    mapping_start := posix.mmap(
        nil,
        c.size_t(length_to_map),
        posix.PROT_READ if mode == .Read else posix.PROT_WRITE,
        posix.MAP_SHARED,
        file_handle,
        aligned_offset,
    )

    if mapping_start == nil || mapping_start == posix.MAP_FAILED {
        return m, General_Error.Invalid_Command
    }

    m.file = file
    m.fd = file_handle
    m.access = mode
    m.data = cast([^]byte)mapping_start
    m.size = length
    m.mapped_size = length_to_map
    m.is_internal = true
    m.mapping_fd = FD(0)

    return m, nil
}

_sync :: proc(m: ^Mapping) -> (err: Error) {
    if m.data == nil {
        return General_Error.Invalid_File
    }
    if posix.msync(rawptr(m.data), c.size_t(m.mapped_size), posix.MS_SYNC) == posix.result.OK {
        return nil
    } else {
        return IO_Error.Invalid_Write
    }
}

_fsync :: proc(fd: FD) -> Error {
    if posix.fsync(fd) == posix.result.OK {
        return nil
    } else {
        return IO_Error.Invalid_Write
    }
}

_fdatasync :: proc(fd: FD) -> Error {
    if posix.fdatasync(fd) == posix.result.OK {
        return nil
    } else {
        return IO_Error.Invalid_Write
    }
}

_truncate :: proc(fd: FD, size: i64) -> Error {
    if posix.ftruncate(fd, posix.off_t(size)) == posix.result.OK {
        return nil
    } else {
        return IO_Error.Invalid_Write
    }
}

_close :: proc(m: ^Mapping) -> (err: Error) {
    if m == nil {
        return nil
    }

    data := m.data
    if data != nil {
        posix.munmap(data, c.size_t(m.mapped_size))
        m.data = nil
    }

    if m.fd != -1 {
        posix.close(m.fd)
        m.fd = -1
    }

    return nil
}
