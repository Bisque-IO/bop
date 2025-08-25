package http_ws

import "core:strings"

// Events
ChunkedEvent_Kind :: enum { Data, End, ProtocolError }

ChunkedEvent :: struct {
    kind:     ChunkedEvent_Kind,
    data:     []u8,
    err_msg:  string,
}

// State
ChunkedState :: struct {
    reading_size:      bool,
    reading_size_hex:  u64,
    size_done:         bool,
    chunk_remaining:   u64,
    in_trailers:       bool,
    need_lf_after_cr:  bool,
    trailers_crlf_run: int,
    done:              bool,
}

chunked_reset :: proc(s: ^ChunkedState) { s^ = ChunkedState{ reading_size = true } }

chunked_parse :: proc(s: ^ChunkedState, buf: []u8, out: ^[]ChunkedEvent, allocator := context.allocator) -> int {
    if s.done || buf.len == 0 { return 0 }
    consumed := 0

    parse_error := proc(msg: string) {
        out^ = append(out^, ChunkedEvent{kind = .ProtocolError, err_msg = strings.clone(msg, allocator)})
    }

    for consumed < buf.len && !s.done {
        if s.in_trailers {
            b := buf[consumed]; consumed += 1
            switch b {
            case '\r', '\n':
                s.trailers_crlf_run += 1
                if s.trailers_crlf_run >= 4 {
                    out^ = append(out^, ChunkedEvent{kind = .End})
                    s.done = true
                }
            case:
                s.trailers_crlf_run = 0
            }
            continue
        }

        if s.reading_size {
            for consumed < buf.len && s.reading_size && !s.size_done {
                c := buf[consumed]
                is_hex := (('0' <= c && c <= '9') || ('a' <= c && c <= 'f') || ('A' <= c && c <= 'F'))
                if is_hex {
                    v: u8
                    if c <= '9' { v = c - '0' }
                    else if c <= 'F' { v = 10 + (c - 'A') }
                    else { v = 10 + (c - 'a') }
                    s.reading_size_hex = (s.reading_size_hex << 4) | u64(v)
                    consumed += 1
                    continue
                }
                if c == ';' {
                    consumed += 1
                    for consumed < buf.len {
                        d := buf[consumed]; consumed += 1
                        if d == '\r' { s.need_lf_after_cr = true; break }
                        if d == '\n' { s.size_done = true; break }
                    }
                    if s.need_lf_after_cr || s.size_done { break }
                    continue
                }
                if c == '\r' { s.need_lf_after_cr = true; consumed += 1; break }
                if c == '\n' { s.size_done = true; consumed += 1; break }

                parse_error("invalid char in chunk size line")
                s.done = true
                return consumed
            }

            if s.need_lf_after_cr {
                if consumed >= buf.len { break }
                if buf[consumed] != '\n' {
                    parse_error("expected LF after CR in chunk size")
                    s.done = true
                    return consumed
                }
                consumed += 1
                s.size_done = true
                s.need_lf_after_cr = false
            }

            if !s.size_done { break }

            s.chunk_remaining = s.reading_size_hex
            s.reading_size = false
            s.size_done = false
            s.reading_size_hex = 0

            if s.chunk_remaining == 0 {
                s.in_trailers = true
                s.trailers_crlf_run = 0
                continue
            }
        }

        if s.chunk_remaining > 0 {
            have := buf.len - consumed
            if have == 0 { break }
            take := have
            if u64(take) > s.chunk_remaining { take = int(s.chunk_remaining) }

            if take > 0 {
                append(out, ChunkedEvent{kind = .Data, data = buf[consumed:consumed+take]})
                consumed += take
                s.chunk_remaining -= u64(take)
            }
            if s.chunk_remaining > 0 { continue }
        }

        if consumed+2 <= buf.len {
            if buf[consumed] != '\r' || buf[consumed+1] != '\n' {
                parse_error("missing CRLF after chunk data")
                s.done = true
                return consumed
            }
            consumed += 2
        } else {
            break
        }
        s.reading_size = true
    }
    return consumed
}
