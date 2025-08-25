package uws_test

import "base:runtime"

import c "core:c/libc"
import "core:fmt"

import ".."

main :: proc() {
    app := uws.app_create()

    uws.app_get(nil, app, "/*", proc "c" (user_data: rawptr, res: uws.Response, req: uws.Request) {
        context = runtime.default_context()
        fmt.println("Received GET ", uws.request_get_url(req), uws.request_get_query(req))
        uws.response_end(res, "Hello World")
    })

    uws.app_ws(nil, app, "/ws", uws.WS_Behavior{
        open = proc "c" (user_data: rawptr, ws: uws.Socket) {
            context = runtime.default_context()
            fmt.println("WebSocket connected")
        },
        message = proc "c" (
            user_data: rawptr,
            ws: uws.Socket,
            message: [^]u8,
            length: c.size_t,
            opcode: uws.Opcode,
        ) {
            context = runtime.default_context()
            msg := string(message[0:length])
            fmt.println("Received message:", msg)

            // uws.ws_send(ws, msg, uws.TEXT, true)
        },
        close = proc "c" (
            user_data: rawptr,
            ws: uws.Socket,
            message: [^]u8,
            length: c.size_t,
        ) {
            context = runtime.default_context()
            fmt.println("WebSocket disconnected")
        },
    })

    uws.app_listen(nil, app, 3000, proc "c" (user_data: rawptr, listen_socket: rawptr) -> i32 {
        context = runtime.default_context()
        fmt.println("Listening on port 3000")
        return 0
    })

    uws.app_run(app)
}