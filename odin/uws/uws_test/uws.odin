package uws_test

import "base:runtime"

import c "core:c/libc"
import "core:fmt"
import "core:strings"

import ".."

main :: proc() {
    app := uws.app_create()
    fmt.println(uws.app_loop(app))

    uws.app_ws(app, "/ws", uws.WS_Behavior{
        open = proc "c" (ws: uws.WS, user_data: rawptr) {
            context = runtime.default_context()
            fmt.println("WebSocket connected")
        },
        message = proc "c" (
            ws: uws.WS,
            user_data: rawptr,
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
            ws: uws.WS,
            user_data: rawptr,
            message: [^]u8,
            length: c.size_t,
        ) {
            context = runtime.default_context()
            fmt.println("WebSocket disconnected")
        },
    })

    uws.app_any(app, app, "/", proc "c" (user_data: rawptr, res: uws.Response, req: uws.Request) {
        context = runtime.default_context()
        
        num_headers := uws.request_get_header_count(req)

        loop := uws.app_loop(transmute(uws.App)user_data)

        uws.response_on_aborted(res, res, proc "c" (user_data: rawptr, res: uws.Response) {
            context = runtime.default_context()
            fmt.println("aborted")
        })

        uws.response_defer(res, nil, proc "c" (loop: uws.Loop, res: uws.Response, user_data: rawptr) {
            context = runtime.default_context()
            fmt.println("response_defer")

            uws.response_end(res, "Hello World", false)
        })

        // uws.loop_defer(loop, res, proc "c" (loop: uws.Loop, user_data: rawptr) {
        //     context = runtime.default_context()
        //     fmt.println("deferred!")

        //     uws.response_end(transmute(uws.Response)user_data, "Hello World", false)
        // })

        fmt.println("Loop", uintptr(loop))

        fmt.println("Received ", uws.request_get_method(req), uws.request_get_url(req), uws.request_get_query(req), " ", "headers:", num_headers)
        
        header: uws.HTTP_Header = {}
        for i in 0..<num_headers {
            uws.request_get_header_at(req, i, &header)
            // fmt.println("\t", header.key, header.value)
        }

        // uws.response_end(res, "Hello World", false)
    })

    uws.app_listen(app, nil, port = 3000, handler = proc "c" (user_data: rawptr, listen_socket: rawptr) -> i32 {
        context = runtime.default_context()
        fmt.println("Listening on port 3000")
        return 0
    })

    uws.app_run(app)
}
