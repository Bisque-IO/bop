package http_ws

import "core:mem"
import "core:strings"

// ── Opcodes/Events ───────────────────────────────────────────────────────
WsOpcode :: enum u8 {
    Continuation = 0x0,
    Text         = 0x1,
    Binary       = 0x2,
    Close        = 0x8,
    Ping         = 0x9,
    Pong         = 0xA,
}

WsEvent_Kind :: enum { FrameStart, Data, FrameEnd, ProtocolError }

WsEvent :: struct {
    kind:        WsEvent_Kind,
    fin:         bool,
    opcode:      WsOpcode,
    payload_len: u64,
    data:        []u8,
    err_msg:     string,
}

WsParser :: struct {
    fin:           bool,
    opcode:        WsOpcode,
    masked:        bool,
    need_ext_len:  int,
    len_field:     u64,
    mask_key:      [4]u8,
    mask_index:    u32,
    in_frame:      bool,
    payload_read:  u64,
}

ws_reset :: proc(p: ^WsParser) { p^ = WsParser{} }

append_protocol_error :: proc(out: ^[]WsEvent, msg: string, allocator := context.allocator) {
    out^ = append(out^, WsEvent{ kind = .ProtocolError, err_msg = strings.clone(msg, allocator) })
}

// Streaming parse; emits unmasked Data chunks
ws_parse :: proc(p: ^WsParser, buf: []u8, out: ^[]WsEvent, allocator := context.allocator) -> int {
    consumed := 0
    for consumed < buf.len {
        if !p.in_frame {
            if (buf.len - consumed) < 2 { break }
            b0 := buf[consumed+0]
            b1 := buf[consumed+1]

            p.fin    = (b0 & 0x80) != 0
            if (b0 & 0x70) != 0 {
                append_protocol_error(out, "RSV bits set", allocator)
                return consumed
            }
            p.opcode = WsOpcode(b0 & 0x0F)

            p.masked = (b1 & 0x80) != 0
            base_len := u64(b1 & 0x7F)
            header_need := 2

            if base_len == 126 { header_need += 2 }
            else if base_len == 127 { header_need += 8 }
            if p.masked { header_need += 4 } else {
                append_protocol_error(out, "client frame not masked", allocator)
                return consumed
            }

            if (buf.len - consumed) < header_need { break }

            idx := consumed + 2
            if base_len < 126 {
                p.len_field = base_len
            } else if base_len == 126 {
                p.len_field = (u64(buf[idx]) << 8) | u64(buf[idx+1])
                idx += 2
            } else {
                lf: u64 = 0
                for j in 0..<8 do lf = (lf << 8) | u64(buf[idx+j])
                p.len_field = lf
                idx += 8
            }

            p.mask_key[0] = buf[idx+0]
            p.mask_key[1] = buf[idx+1]
            p.mask_key[2] = buf[idx+2]
            p.mask_key[3] = buf[idx+3]
            idx += 4

            consumed += header_need
            p.in_frame = true
            p.payload_read = 0
            p.mask_index   = 0

            out^ = append(out^, WsEvent{
                kind = .FrameStart,
                fin = p.fin,
                opcode = p.opcode,
                payload_len = p.len_field,
            })
        }

        remaining := int(p.len_field - p.payload_read)
        if remaining == 0 {
            out^ = append(out^, WsEvent{kind = .FrameEnd, fin = p.fin, opcode = p.opcode})
            p.in_frame = false
            continue
        }

        have := buf.len - consumed
        if have == 0 { break }
        take := have
        if take > remaining { take = remaining }

        tmp := make([]u8, take, allocator)
        for i in 0..<take {
            tmp[i] = buf[consumed+i] ^ p.mask_key[(p.mask_index + u32(i)) & 3]
        }
        p.mask_index = (p.mask_index + u32(take)) & 3
        out^ = append(out^, WsEvent{ kind = .Data, opcode = p.opcode, fin = p.fin, data = tmp })

        consumed += take
        p.payload_read += u64(take)
    }
    return consumed
}

// ── Writers (server->client frames are unmasked) ─────────────────────────
ws_frame_size :: proc(payload_len: u64) -> int {
    base := 2
    ext := 0
    if payload_len < 126     { ext = 0 }
    else if payload_len <= 0xFFFF { ext = 2 }
    else                     { ext = 8 }
    return base + ext + int(payload_len)
}

ws_write_frame :: proc(op: WsOpcode, fin: bool, payload: []u8, out: []u8) -> (n: int, need: int, ok: bool) {
    need = ws_frame_size(u64(payload.len))
    if out.len < need { return 0, need, false }

    i := 0
    b0: u8 = u8(op) & 0x0F
    if fin { b0 |= 0x80 }
    out[i] = b0; i += 1

    plen := payload.len
    if plen < 126 {
        out[i] = u8(plen); i += 1
    } else if plen <= 0xFFFF {
        out[i] = 126; i += 1
        out[i+0] = u8((plen >> 8) & 0xFF)
        out[i+1] = u8(plen & 0xFF)
        i += 2
    } else {
        out[i] = 127; i += 1
        p := u64(plen)
        for j in 0..<8 {
            shift := 56 - 8*j
            out[i+j] = u8((p >> shift) & 0xFF)
        }
        i += 8
    }

    if plen > 0 {
        mem.copy(out[i : i+plen], payload)
        i += plen
    }
    return i, need, true
}

ws_write_text   :: proc(data: []u8, out: []u8) -> (n:int, need:int, ok:bool) { return ws_write_frame(.Text, true, data, out) }
ws_write_binary :: proc(data: []u8, out: []u8) -> (n:int, need:int, ok:bool) { return ws_write_frame(.Binary, true, data, out) }

ws_write_ping :: proc(payload: []u8, out: []u8) -> (n:int, need:int, ok:bool) {
    if payload.len > 125 { return 0, 0, false }
    return ws_write_frame(.Ping, true, payload, out)
}
ws_write_pong :: proc(payload: []u8, out: []u8) -> (n:int, need:int, ok:bool) {
    if payload.len > 125 { return 0, 0, false }
    return ws_write_frame(.Pong, true, payload, out)
}
ws_write_close :: proc(code: u16, reason: []u8, out: []u8, scratch: []u8) -> (n:int, need:int, ok:bool) {
    need_payload := 2 + reason.len
    if scratch.len < need_payload { return 0, need_payload, false }
    scratch[0] = u8((code >> 8) & 0xFF)
    scratch[1] = u8(code & 0xFF)
    mem.copy(scratch[2 : 2+reason.len], reason)
    return ws_write_frame(.Close, true, scratch[:need_payload], out)
}
