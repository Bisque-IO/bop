package http_ws

import "core:fmt"
import "core:os"
import "core:strings"
import "core:time"

// Options to control static serving behavior
StaticOptions :: struct {
    root:            string,          // filesystem root to serve from (absolute recommended)
    index_files:     []string,        // e.g., ["index.html", "index.htm"]
    cache_control:   string,          // e.g., "public, max-age=3600" ("" to omit)
    extra_headers:   []Header,        // appended to response headers
    allow_dotfiles:  bool,            // default false: 404 on dotfiles
}

// Outcome for the HTTP layer
StaticResult :: struct {
    status_code: int,
    status_text: string,
    headers:     []Header,
    // If file_path != "", stream its contents after headers for GET (not for HEAD).
    file_path:   string,
    // If body != nil, write it after headers (error bodies etc.)
    body:        []u8,
}

// Main handler: build headers + file path to stream
static_handle_request :: proc(req: ^HttpHeaders, opt: ^StaticOptions, allocator := context.allocator) -> StaticResult {
    res: StaticResult
    // Only GET/HEAD
    if !strings.equal_fold(req.method, "GET") && !strings.equal_fold(req.method, "HEAD") {
        res.status_code = 405
        res.status_text = "Method Not Allowed"
        res.headers = add_common_headers(nil, "text/plain; charset=utf-8", 0, "", allocator)
        res.body = []u8("Method Not Allowed")
        return res
    }

    // Normalize path → filesystem path
    path := sanitize_path(req.path, allocator)
    defer delete(path, allocator)

    if path.len == 0 || path[0] != '/' {
        res.status_code = 400
        res.status_text = "Bad Request"
        res.headers = add_common_headers(nil, "text/plain; charset=utf-8", 0, "", allocator)
        res.body = []u8("Bad Request")
        return res
    }

    // Reject dotfiles unless allowed
    if !opt.allow_dotfiles && has_dotfile_component(path) {
        notfound(&res, allocator)
        return res
    }

    // Resolve filesystem path under root
    fs_path := fmt.tprintf("%s%s", opt.root, path, allocator)
    defer delete(fs_path, allocator)

    st, ok := os.stat(fs_path)
    if ok && st.is_dir {
        // Try index files
        resolved := false
        for name in opt.index_files {
            try_path := fmt.tprintf("%s/%s", fs_path, name, allocator)
            st2, ok2 := os.stat(try_path)
            if ok2 && st2.is_file {
                fs_path = strings.clone(try_path, allocator)
                st = st2
                resolved = true
                delete(try_path, allocator)
                break
            }
            delete(try_path, allocator)
        }
        if !resolved {
            notfound(&res, allocator)
            return res
        }
    } else if !(ok && st.is_file) {
        notfound(&res, allocator)
        return res
    }

    // Build headers
    ctype := mime_from_ext(file_ext(fs_path))
    lm := http_date_from_unix(st.mod_time_unix, allocator)
    length := cast(u64) st.size

    hdrs := add_common_headers(opt.extra_headers, ctype, length, lm, allocator)
    if opt.cache_control.len > 0 {
        hdrs = append(hdrs, Header{name = "Cache-Control", value = strings.clone(opt.cache_control, allocator)})
    }

    res.status_code = 200
    res.status_text = "OK"
    res.headers = hdrs
    res.file_path = strings.clone(fs_path, allocator)
    // No default body for GET (you stream file). For HEAD, you simply don’t send the body.
    return res
}

// Build an HTTP response head (status line + headers + CRLF)
static_make_response_head :: proc(res: ^StaticResult, minor_version: int, allocator := context.allocator) -> string {
    if minor_version != 0 && minor_version != 1 { minor_version = 1 }
    head := fmt.tprintf("HTTP/1.%d %d %s\r\n", minor_version, res.status_code, res.status_text, allocator)
    for h in res.headers {
        line := fmt.tprintf("%s: %s\r\n", h.name, h.value, allocator)
        head = fmt.tprintf("%s%s", head, line, allocator)
        delete(line, allocator)
    }
    // end of headers
    head = fmt.tprintf("%s\r\n", head, allocator)
    return head
}

// ── Helpers ─────────────────────────────────────────────────────────────
notfound :: proc(res: ^StaticResult, allocator := context.allocator) {
    res.status_code = 404
    res.status_text = "Not Found"
    res.headers = add_common_headers(nil, "text/plain; charset=utf-8", 9, "", allocator)
    res.body = []u8("Not Found")
}

add_common_headers :: proc(base: []Header, content_type: string, content_length: u64, last_modified: string, allocator := context.allocator) -> []Header {
    hdrs := make([]Header, 0, allocator)
    if base != nil {
        for h in base {
            hdrs = append(hdrs, Header{name = strings.clone(h.name, allocator), value = strings.clone(h.value, allocator)})
        }
    }
    date := http_date_now(allocator)
    hdrs = append(hdrs, Header{name = "Date", value = date})
    hdrs = append(hdrs, Header{name = "Server", value = strings.clone("odin-static", allocator)})
    if content_type.len > 0 {
        hdrs = append(hdrs, Header{name = "Content-Type", value = strings.clone(content_type, allocator)})
    }
    hdrs = append(hdrs, Header{name = "Content-Length", value = fmt.tprintf("%v", content_length, allocator)})
    if last_modified.len > 0 {
        hdrs = append(hdrs, Header{name = "Last-Modified", value = strings.clone(last_modified, allocator)})
    }
    return hdrs
}

// Prevent ../ traversal; collapse //; keep leading slash; no URL-decode here.
sanitize_path :: proc(url_path: string, allocator := context.allocator) -> string {
    // Strip query/fragment if present (very simple)
    end := url_path.len
    for i in 0..<url_path.len {
        if url_path[i] == '?' || url_path[i] == '#' { end = i; break }
    }
    p := strings.clone(url_path[:end], allocator)

    // ensure leading slash
    if p.len == 0 || p[0] != '/' {
        p = fmt.tprintf("/%s", p, allocator)
    }

    // split, handle . and .., collapse empties
    out := make([]u8, 0, allocator)
    out = append(out, '/')
    i := 1
    comps: []string = nil
    for i < p.len {
        j := i
        for j < p.len && p[j] != '/' do j += 1
        seg := p[i:j]
        if seg.len == 0 || seg == "." {
            // ignore
        } else if seg == ".." {
            if comps.len > 0 { comps = comps[:comps.len-1] }
        } else {
            comps = append(comps, strings.clone(seg, allocator))
        }
        i = j + 1
    }
    // rebuild
    for k, s in comps {
        append(out, []u8(s))
        if k+1 < comps.len { out = append(out, '/') }
        delete(s, allocator)
    }
    delete(p, allocator)
    return string(out)
}

has_dotfile_component :: proc(path: string) -> bool {
    i := 1 // skip leading slash
    for i < path.len {
        j := i
        for j < path.len && path[j] != '/' do j += 1
        seg := path[i:j]
        if seg.len > 0 && seg[0] == '.' { return true }
        i = j + 1
    }
    return false
}

file_ext :: proc(p: string) -> string {
    dot := -1
    for i in 0..<p.len {
        if p[i] == '.' { dot = i }
        if p[i] == '/' { dot = -1 }
    }
    if dot >= 0 && dot+1 < p.len { return p[dot+1:] }
    return ""
}

mime_from_ext :: proc(ext: string) -> string {
    // very small map; extend as needed
    if ext == "html" || ext == "htm"  { return "text/html; charset=utf-8" }
    if ext == "css"                   { return "text/css; charset=utf-8" }
    if ext == "js"                    { return "application/javascript" }
    if ext == "json"                  { return "application/json" }
    if ext == "png"                   { return "image/png" }
    if ext == "jpg" || ext == "jpeg"  { return "image/jpeg" }
    if ext == "gif"                   { return "image/gif" }
    if ext == "svg"                   { return "image/svg+xml" }
    if ext == "txt"                   { return "text/plain; charset=utf-8" }
    if ext == "wasm"                  { return "application/wasm" }
    return "application/octet-stream"
}

http_date_now :: proc(allocator := context.allocator) -> string {
    t := time.now_utc()
    return http_date_from_unix(t.unix, allocator)
}

http_date_from_unix :: proc(unix_sec: i64, allocator := context.allocator) -> string {
    // Format: Sun, 06 Nov 1994 08:49:37 GMT
    t := time.unix_to_time(unix_sec)
    wd := [7]string{"Sun","Mon","Tue","Wed","Thu","Fri","Sat"}
    mon := [12]string{"Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"}
    return fmt.tprintf("%s, %02d %s %04d %02d:%02d:%02d GMT",
        wd[int(t.weekday)],
        t.day, mon[int(t.month)-1], t.year,
        t.hour, t.minute, t.second,
        allocator,
    )
}
