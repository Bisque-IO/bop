package http_ws

import "core:fmt"
import "core:strings"
import "core:crypto/sha1"
import "core:encoding/base64"

// ── picohttpparser FFI ───────────────────────────────────────────────────
// foreign import pico {
//     #include "picohttpparser.h"
// }

phr_header :: struct {
    name: ^u8,
    name_len: uintptr,
    value: ^u8,
    value_len: uintptr,
}

foreign pico {
    phr_parse_request :: proc "c" (
        buf: ^u8, len: uintptr,
        method: ^^u8, method_len: ^uintptr,
        path: ^^u8, path_len: ^uintptr,
        minor_version: ^int,
        headers: ^phr_header, num_headers: ^uintptr,
        last_len: uintptr,
    ) -> int ---
}

// ── Public HTTP types ────────────────────────────────────────────────────
Header :: struct {
    name:  string,
    value: string,
}

HttpHeaders :: struct {
    method:        string,
    path:          string,
    minor_version: int,
    headers:       []Header,
}

HttpParseStatus :: enum { Ok, NeedMore, Error }

HttpParseResult :: struct {
    status:   HttpParseStatus,
    consumed: int,
    headers:  HttpHeaders,
    err_msg:  string,
}

// ── Parsing ──────────────────────────────────────────────────────────────
parse_http_request :: proc(
    buf: []u8,
    last_len: int,
    max_headers: int = 64,
    allocator := context.allocator,
) -> HttpParseResult {
    res: HttpParseResult;
    if buf.len == 0 {
        res.status = .NeedMore
        return res
    }

    tmp_headers := make([]phr_header, max_headers, allocator)
    defer delete(tmp_headers, allocator)

    method_ptr: ^u8
    method_len: uintptr
    path_ptr:   ^u8
    path_len:   uintptr
    minor: int
    num_headers := cast(uintptr) tmp_headers.len

    rc := phr_parse_request(
        &buf[0], cast(uintptr) buf.len,
        &method_ptr, &method_len,
        &path_ptr,   &path_len,
        &minor,
        &tmp_headers[0], &num_headers,
        cast(uintptr) last_len,
    )

    switch rc {
    case -2:
        res.status = .NeedMore
        return res
    case -1:
        res.status = .Error
        res.err_msg = "malformed HTTP request line or headers"
        return res
    case:
        res.status = .Ok
        res.consumed = rc
    }

    hdrs := make([]Header, int(num_headers), allocator)
    for i in 0..<int(num_headers) {
        n := tmp_headers[i].name_len
        v := tmp_headers[i].value_len
        hdrs[i].name  = strings.string_from_bytes(tmp_headers[i].name, int(n))
        hdrs[i].value = strings.string_from_bytes(tmp_headers[i].value, int(v))
    }

    res.headers.method        = strings.string_from_bytes(method_ptr, int(method_len))
    res.headers.path          = strings.string_from_bytes(path_ptr,   int(path_len))
    res.headers.minor_version = minor
    res.headers.headers       = hdrs
    return res
}

// ── Header helpers ───────────────────────────────────────────────────────
http_get_header :: proc(h: ^HttpHeaders, key_ci: string) -> (ok: bool, value: string) {
    for hdr in h.headers {
        if strings.equal_fold(hdr.name, key_ci) {
            return true, hdr.value
        }
    }
    return false, ""
}

token_list_contains_ci :: proc(list: string, needle: string) -> bool {
    i := 0
    for i < list.len {
        for i < list.len && (list[i] == ' ' || list[i] == '\t') do i += 1
        start := i
        for i < list.len && list[i] != ',' do i += 1
        tok := strings.trim_space(list[start:i])
        if strings.equal_fold(tok, needle) {
            return true
        }
        if i < list.len && list[i] == ',' do i += 1
    }
    return false
}

http_is_websocket_upgrade :: proc(h: ^HttpHeaders) -> bool {
    ok, up := http_get_header(h, "Upgrade")
    if !ok || !strings.equal_fold(strings.trim_space(up), "websocket") {
        return false
    }
    ok, conn := http_get_header(h, "Connection")
    if !ok || !token_list_contains_ci(conn, "upgrade") {
        return false
    }
    ok, _ = http_get_header(h, "Sec-WebSocket-Key")
    return ok
}

// Compute Sec-WebSocket-Accept
websocket_accept_key :: proc(client_key: string, allocator := context.allocator) -> string {
    guid := "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
    combined := fmt.tprintf("%s%s", client_key, guid, allocator)
    defer delete(combined, allocator)

    sum := sha1.sum([]u8(combined))
    enc := base64.encode_string(sum[:], allocator)
    return enc
}

// Minimal 101 response string (write to socket then switch to WS framing)
make_ws_101_response :: proc(sec_websocket_key: string, allocator := context.allocator) -> string {
    accept := websocket_accept_key(sec_websocket_key, allocator)
    resp := fmt.tprintf(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: %s\r\n\r\n",
        accept, allocator,
    )
    delete(accept, allocator)
    return resp
}
