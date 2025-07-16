#+build windows
package fs

import "base:runtime"
import win32 "core:sys/windows"
import "core:os/os2"
import "core:slice"
import "core:strings"
import "core:io"

@(init)
init_module :: proc() {
    info: win32.SYSTEM_INFO
    win32.GetSystemInfo(&info)
    PAGE_SIZE = int(info.dwPageSize)
}

FD             :: win32.HANDLE
INVALID_HANDLE :: win32.INVALID_HANDLE_VALUE

is_valid_handle :: proc "contextless" (fd: FD) -> bool {
    return fd != INVALID_HANDLE
}

is_invalid_handle :: proc "contextless" (fd: FD) -> bool {
    return fd == INVALID_HANDLE
}

S_IWRITE :: 0o200
_ERROR_BAD_NETPATH :: 53
MAX_RW :: 1<<30

_Platform_Error :: win32.System_Error

_error_string :: proc(errno: i32) -> string {
    e := win32.DWORD(errno)
    if e == 0 {
        return ""
    }

    err := runtime.Type_Info_Enum_Value(e)

    ti := &runtime.type_info_base(type_info_of(win32.System_Error)).variant.(runtime.Type_Info_Enum)
    if idx, ok := slice.binary_search(ti.values, err); ok {
        return ti.names[idx]
    }
    return "<unknown platform error>"
}

_get_platform_error :: proc() -> Error {
    err := win32.GetLastError()
    if err == 0 {
        return nil
    }
    switch err {
    case win32.ERROR_ACCESS_DENIED, win32.ERROR_SHARING_VIOLATION:
        return .Permission_Denied

    case win32.ERROR_FILE_EXISTS, win32.ERROR_ALREADY_EXISTS:
        return .Exist

    case win32.ERROR_FILE_NOT_FOUND, win32.ERROR_PATH_NOT_FOUND:
        return .Not_Exist

    case win32.ERROR_NO_DATA:
        return .Closed

    case win32.ERROR_TIMEOUT, win32.WAIT_TIMEOUT:
        return .Timeout

    case win32.ERROR_NOT_SUPPORTED:
        return .Unsupported

    case win32.ERROR_HANDLE_EOF:
        return .EOF

    case win32.ERROR_INVALID_HANDLE:
        return .Invalid_File

    case win32.ERROR_NEGATIVE_SEEK:
        return .Invalid_Offset

    case win32.ERROR_BROKEN_PIPE:
        return .Broken_Pipe

    case
    win32.ERROR_BAD_ARGUMENTS,
    win32.ERROR_INVALID_PARAMETER,
    win32.ERROR_NOT_ENOUGH_MEMORY,
    win32.ERROR_NO_MORE_FILES,
    win32.ERROR_LOCK_VIOLATION,
    win32.ERROR_CALL_NOT_IMPLEMENTED,
    win32.ERROR_INSUFFICIENT_BUFFER,
    win32.ERROR_INVALID_NAME,
    win32.ERROR_LOCK_FAILED,
    win32.ERROR_ENVVAR_NOT_FOUND,
    win32.ERROR_OPERATION_ABORTED,
    win32.ERROR_IO_PENDING,
    win32.ERROR_NO_UNICODE_TRANSLATION:
    // fallthrough
    }
    return Platform_Error(err)
}



int64_high :: proc "contextless" (n: i64) -> win32.DWORD {
    return win32.DWORD(n >> 32)
}

int64_low :: proc "contextless" (n: i64) -> win32.DWORD {
    return win32.DWORD(n & 0xffffffff)
}

//to_wstring :: proc(str: string, allocator := context.allocator) -> win32.wstring {
//    return win32.utf8_to_wstring_alloc(str, allocator)
//}

//open_file_helper :: proc(path: string, mode: Access_Mode) -> FD {
//    return win32.CreateFileW(
//        s_2_ws(path),
//        win32.GENERIC_READ if mode == .Read else win32.GENERIC_READ | win32.GENERIC_WRITE,
//        win32.FILE_SHARE_READ | win32.FILE_SHARE_WRITE,
//        0,
//        win32.OPEN_EXISTING,
//        win32.FILE_ATTRIBUTE_NORMAL,
//        0
//    )
//}
//
//open_file :: proc(path: string, mode: Access_Mode) -> (fd: FD, err: Error) {
//    if len(path) == 0 {
//        return 0, .Empty_Path
//    }
//
//    fd = open_file_helper(path, mode, context.temp_allocator)
//    werr := win32.GetLastError()
//    if werr != win32.ERROR_SUCCESS {
//        err = to_error(werr)
//        return
//    }
//
//    return fd, .Success
//}

//query_file_size := proc(fd: FD) -> (err: Error) {
//    file_size: win32.LARGE_INTEGER
//    if win32.GetFileSizeEx(fd, &file_size) == 0 {
//        err = to_error(win32.GetLastError())
//        return err
//    }
//    return i64(file_size.QuadPart)
//}

_open_file :: proc(
    name: string,
    flags: File_Flags,
    perm: int,
    temp_allocator: runtime.Allocator,
) -> (
    fd: FD,
    err: Error
) {
    if len(name) == 0 {
        err = General_Error.Not_Exist
        return
    }

    path := _fix_long_path(name, temp_allocator) or_return

    access: u32
    switch flags & {.Read, .Write} {
    case {.Read}:         access = win32.FILE_GENERIC_READ
    case {.Write}:        access = win32.FILE_GENERIC_WRITE
    case {.Read, .Write}: access = win32.FILE_GENERIC_READ | win32.FILE_GENERIC_WRITE
    }

    if .Create in flags {
        access |= win32.FILE_GENERIC_WRITE
    }
    if .Append in flags {
        access &~= win32.FILE_GENERIC_WRITE
        access |= win32.FILE_APPEND_DATA
    }
    share_mode := u32(win32.FILE_SHARE_READ | win32.FILE_SHARE_WRITE)
    sa := win32.SECURITY_ATTRIBUTES {
        nLength = size_of(win32.SECURITY_ATTRIBUTES),
        bInheritHandle = .Inheritable in flags,
    }

    create_mode: u32 = win32.OPEN_EXISTING
    switch {
    case flags & {.Create, .Excl} == {.Create, .Excl}:
        create_mode = win32.CREATE_NEW
    case flags & {.Create, .Trunc} == {.Create, .Trunc}:
        create_mode = win32.CREATE_ALWAYS
    case flags & {.Create} == {.Create}:
        create_mode = win32.OPEN_ALWAYS
    case flags & {.Trunc} == {.Trunc}:
        create_mode = win32.TRUNCATE_EXISTING
    }

    attrs: u32 = win32.FILE_ATTRIBUTE_NORMAL|win32.FILE_FLAG_BACKUP_SEMANTICS
    if perm & S_IWRITE == 0 {
        attrs = win32.FILE_ATTRIBUTE_READONLY
        if create_mode == win32.CREATE_ALWAYS {
            // NOTE(bill): Open has just asked to create a file in read-only mode.
            // If the file already exists, to make it akin to a *nix open call,
            // the call preserves the existing permissions.
            h := win32.CreateFileW(path, access, share_mode, &sa, win32.TRUNCATE_EXISTING, win32.FILE_ATTRIBUTE_NORMAL, nil)
            if h == win32.INVALID_HANDLE {
                switch e := win32.GetLastError(); e {
                case win32.ERROR_FILE_NOT_FOUND, _ERROR_BAD_NETPATH, win32.ERROR_PATH_NOT_FOUND:
                // file does not exist, create the file
                case 0:
                    return FD(h), nil
                case:
                    return nil, _get_platform_error()
                }
            }
        }
    }

    h := win32.CreateFileW(path, access, share_mode, &sa, create_mode, attrs, nil)
    if h == win32.INVALID_HANDLE {
        return nil, _get_platform_error()
    }
    return FD(h), nil
}

_mmap :: proc(file_handle: FD, offset: int, length: int, mode: Access_Mode) -> (m: MMAP, err: Error) {
    aligned_offset := make_offset_page_aligned(offset)
    length_to_map := offset - aligned_offset + length

    max_file_size := i64(offset + length)
    file_mapping_handle := win32.CreateFileMappingW(
        file_handle,
        nil,
        win32.PAGE_READONLY if mode == .Read else win32.PAGE_READWRITE,
        int64_high(i64(max_file_size)),
        int64_low(i64(max_file_size)),
        nil,
    )
    if file_mapping_handle == INVALID_HANDLE {
        return m, _get_platform_error()
    }

    mapping_start := win32.MapViewOfFile(
        file_mapping_handle,
        win32.FILE_MAP_READ if mode == .Read else win32.FILE_MAP_WRITE,
        int64_high(i64(aligned_offset)),
        int64_low(i64(aligned_offset)),
        win32.SIZE_T(length_to_map),
    )
    if mapping_start == nil {
        err = _get_platform_error()
        win32.CloseHandle(file_mapping_handle)
        return
    }

    m.fd = file_handle
    m.access = mode
    m.data = cast([^]byte)mapping_start
    m.size = length
    m.mapped_size = length_to_map
    m.is_internal = true
    m.mapping_fd = file_mapping_handle

    return m, nil
}

_msync :: proc(m: ^MMAP) -> (err: Error) {
    if m.data == nil {
        return os2.General_Error.Invalid_File
    }
    if win32.FlushViewOfFile(win32.LPCVOID(rawptr(m.data)), win32.SIZE_T(m.mapped_size)) == win32.FALSE {
        return _get_platform_error()
    }
    return nil
}

_fsync :: proc(fd: FD) -> (err: Error) {
    if win32.FlushFileBuffers(fd) == win32.FALSE {
        return _get_platform_error()
    }
    return nil
}

_fdatasync :: proc(fd: FD) -> (err: Error) {
    return _fsync(fd)
}

_seek :: proc(handle: FD, offset: i64, whence: io.Seek_From) -> (ret: i64, err: Error) {
    if handle == win32.INVALID_HANDLE {
        return 0, .Invalid_File
    }

    w: u32
    switch whence {
    case .Start:   w = win32.FILE_BEGIN
    case .Current: w = win32.FILE_CURRENT
    case .End:     w = win32.FILE_END
    case:
        return 0, .Invalid_Whence
    }
    hi := i32(offset>>32)
    lo := i32(offset)

    dw_ptr := win32.SetFilePointer(handle, lo, &hi, w)
    if dw_ptr == win32.INVALID_SET_FILE_POINTER {
        return 0, _get_platform_error()
    }
    return i64(hi)<<32 + i64(dw_ptr), nil
}

_truncate :: proc(fd: FD, size: i64) -> Error {
    if fd == INVALID_HANDLE {
        return nil
    }
    curr_off := _seek(fd, 0, .Current) or_return
    defer _seek(fd, curr_off, .Start)
    _seek(fd, size, .Start) or_return
    if !win32.SetEndOfFile(fd) {
        return _get_platform_error()
    }
    return nil
}

_close_fd :: proc(fd: FD) -> Error {
    if !win32.CloseHandle(fd) {
        return _get_platform_error()
    }
    return nil
}

_close_mmap :: proc(m: ^MMAP) -> (err: Error) {
    if m == nil || m.data == nil {
        return nil
    }

    data := m.data

    if data != nil {
        win32.UnmapViewOfFile(data)
        m.data = nil
        m.size = 0
        m.mapped_size = 0
    }

    if m.mapping_fd != INVALID_HANDLE {
        win32.CloseHandle(m.mapping_fd)
        m.mapping_fd = INVALID_HANDLE
    }

    if m.fd != INVALID_HANDLE && m.is_internal {
        win32.CloseHandle(m.fd)
        m.fd = INVALID_HANDLE
    }

    return nil
}



can_use_long_paths: bool

@(init)
init_long_path_support :: proc() {
    can_use_long_paths = false

    key: win32.HKEY
    res := win32.RegOpenKeyExW(
        win32.HKEY_LOCAL_MACHINE,
        win32.L(`SYSTEM\CurrentControlSet\Control\FileSystem`),
        0,
        win32.KEY_READ,
        &key,
    )
    defer win32.RegCloseKey(key)
    if res != 0 {
        return
    }

    value: u32
    size := u32(size_of(value))
    res = win32.RegGetValueW(
        key,
        nil,
        win32.L("LongPathsEnabled"),
        win32.RRF_RT_ANY,
        nil,
        &value,
        &size,
    )
    if res != 0 {
        return
    }
    if value == 1 {
        can_use_long_paths = true
    }
}

@(private="package", require_results)
win32_utf8_to_wstring :: proc(s: string, allocator: runtime.Allocator) -> (ws: [^]u16, err: runtime.Allocator_Error) {
    ws = raw_data(win32_utf8_to_utf16(s, allocator) or_return)
    return
}

@(private="package", require_results)
win32_utf8_to_utf16 :: proc(s: string, allocator: runtime.Allocator) -> (ws: []u16, err: runtime.Allocator_Error) {
    if len(s) < 1 {
        return
    }

    b := transmute([]byte)s
    cstr := raw_data(b)
    n := win32.MultiByteToWideChar(win32.CP_UTF8, win32.MB_ERR_INVALID_CHARS, cstr, i32(len(s)), nil, 0)
    if n == 0 {
        return nil, nil
    }

    text := make([]u16, n+1, allocator) or_return

    n1 := win32.MultiByteToWideChar(win32.CP_UTF8, win32.MB_ERR_INVALID_CHARS, cstr, i32(len(s)), raw_data(text), n)
    if n1 == 0 {
        delete(text, allocator)
        return
    }

    text[n] = 0
    for n >= 1 && text[n-1] == 0 {
        n -= 1
    }
    ws = text[:n]
    return
}

@(require_results)
_fix_long_path :: proc(path: string, allocator: runtime.Allocator) -> (win32.wstring, runtime.Allocator_Error) {
    return win32_utf8_to_wstring(_fix_long_path_internal(path), allocator)
}

@(require_results)
_fix_long_path_internal :: proc(path: string) -> string {
    if can_use_long_paths {
        return path
    }

    // When using win32 to create a directory, the path
    // cannot be too long that you cannot append an 8.3
    // file name, because MAX_PATH is 260, 260-12 = 248
    if len(path) < 248 {
        return path
    }

    // UNC paths do not need to be modified
    if len(path) >= 2 && path[:2] == `\\` {
        return path
    }

    if !is_absolute_path(path) { // relative path
        return path
    }

    temp_allocator, _ := runtime.DEFAULT_TEMP_ALLOCATOR_TEMP_GUARD()

    PREFIX :: `\\?`
    path_buf := runtime.make_slice([]byte, len(PREFIX)+len(path)+1, context.temp_allocator)
    copy(path_buf, PREFIX)
    n := len(path)
    r, w := 0, len(PREFIX)
    for r < n {
        switch {
        case is_path_separator(path[r]):
            r += 1
        case path[r] == '.' && (r+1 == n || is_path_separator(path[r+1])):
            // \.\
            r += 1
        case r+1 < n && path[r] == '.' && path[r+1] == '.' && (r+2 == n || is_path_separator(path[r+2])):
            // Skip \..\ paths
            return path
        case:
            path_buf[w] = '\\'
            w += 1
            for r < n && !is_path_separator(path[r]) {
                path_buf[w] = path[r]
                r += 1
                w += 1
            }
        }
    }

    // Root directories require a trailing \
    if w == len(`\\?\c:`) {
        path_buf[w] = '\\'
        w += 1
    }

    return string(path_buf[:w])
}
