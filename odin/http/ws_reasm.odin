package http_ws

import "core:mem"

WsMsgKind :: enum { Text, Binary }
WsMsgEvent_Kind :: enum { MessageStart, MessageData, MessageEnd, ProtocolError }

WsMsgEvent :: struct {
    kind:      WsMsgEvent_Kind,
    msg_kind:  WsMsgKind,
    data:      []u8,
    err_msg:   string,
}

WsReassembler :: struct {
    assembling:  bool,
    kind:        WsMsgKind,
    buf:         []u8,
}

ws_msg_reset :: proc(r: ^WsReassembler) {
    if r.buf != nil { delete(r.buf, context.allocator) }
    r^ = WsReassembler{}
}

ws_msg_on_frame_events :: proc(r: ^WsReassembler, evs: []WsEvent, out: ^[]WsMsgEvent, allocator := context.allocator) {
    for ev in evs {
        switch ev.kind {
        case .FrameStart:
            if !r.assembling {
                if ev.opcode == .Text {
                    r.kind = .Text
                    r.assembling = true
                    out^ = append(out^, WsMsgEvent{kind = .MessageStart, msg_kind = .Text})
                } else if ev.opcode == .Binary {
                    r.kind = .Binary
                    r.assembling = true
                    out^ = append(out^, WsMsgEvent{kind = .MessageStart, msg_kind = .Binary})
                } else if ev.opcode == .Continuation {
                    out^ = append(out^, WsMsgEvent{kind = .ProtocolError, err_msg = "continuation without message start"})
                }
            } else {
                if ev.opcode != .Continuation && !ev.fin {
                    out^ = append(out^, WsMsgEvent{kind = .ProtocolError, err_msg = "unexpected new data frame while assembling message"})
                }
            }

        case .Data:
            if r.assembling && (ev.opcode == .Text || ev.opcode == .Binary || ev.opcode == .Continuation) {
                if r.buf == nil { r.buf = make([]u8, 0, allocator) }
                old := r.buf.len
                r.buf = mem.resize(r.buf, old + ev.data.len, allocator)
                mem.copy(r.buf[old : old+ev.data.len], ev.data)
                out^ = append(out^, WsMsgEvent{kind = .MessageData, msg_kind = r.kind, data = ev.data})
            }

        case .FrameEnd:
            if r.assembling && ev.fin {
                complete := make([]u8, r.buf.len, allocator)
                mem.copy(complete, r.buf)
                out^ = append(out^, WsMsgEvent{kind = .MessageEnd, msg_kind = r.kind, data = complete})
                delete(r.buf, allocator)
                r.buf = nil
                r.assembling = false
            }

        case .ProtocolError:
            out^ = append(out^, WsMsgEvent{kind = .ProtocolError, err_msg = ev.err_msg})
        }
    }
}
