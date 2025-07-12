package usockets_test

import "base:runtime"
import "core:fmt"
import "core:strings"
import "core:time"

import c "core:c/libc"

import bop "../"

SSL :: 1

HTTP_Socket :: struct {
    ctx: runtime.Context,
    offset: c.int,
}

HTTP_Context :: struct {
    response: string,
}

on_wakeup :: proc "c" (loop: ^bop.US_Loop) {

}

on_pre :: proc "c" (loop: ^bop.US_Loop) {

}

on_post :: proc "c" (loop: ^bop.US_Loop) {

}

on_http_socket_writeable :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
    http_socket := cast(^HTTP_Socket)bop.us_socket_ext(SSL, s)
    http_context := cast(^HTTP_Context)bop.us_socket_context_ext(SSL, bop.us_socket_context(SSL, s))

    // Stream whatever is remaining of the response
    http_socket.offset += bop.us_socket_write(
        SSL, s,
        raw_data(http_context.response[http_socket.offset:]),
        c.int(int(len(http_context.response)) - int(http_socket.offset)),
        0,
    )

    return s
}

on_http_socket_close :: proc "c" (s: ^bop.US_Socket, code: c.int, reason: rawptr) -> ^bop.US_Socket {
    context = runtime.default_context()
    fmt.println("client disconnected")
    return s
}

on_http_socket_end :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
    // HTTP does not support half-closed sockets
    bop.us_socket_shutdown(SSL, s)
    return bop.us_socket_close(SSL, s, 0, nil)
}

on_http_socket_data :: proc "c" (s: ^bop.US_Socket, data: [^]byte, length: c.int) -> ^bop.US_Socket {
    http_socket := cast(^HTTP_Socket)bop.us_socket_ext(SSL, s)
    http_context := cast(^HTTP_Context)bop.us_socket_context_ext(SSL, bop.us_socket_context(SSL, s))

    /* We treat all data events as a request */
    http_socket.offset = bop.us_socket_write(SSL, s, raw_data(http_context.response), c.int(len(http_context.response)), 0)

    /* Reset idle timer */
    bop.us_socket_timeout(SSL, s, 30)

    return s
}

on_http_socket_open :: proc "c" (
    s: ^bop.US_Socket,
    is_client: c.int,
    ip: [^]byte,
    ip_length: c.int,
) -> ^bop.US_Socket {
    http_socket := cast(^HTTP_Socket)bop.us_socket_ext(SSL, s)

    /* Reset offset */
    http_socket.offset = 0

    /* Timeout idle HTTP connections */
    bop.us_socket_timeout(SSL, s, 30)

    context = runtime.default_context()

    fmt.println("client connected")

    return s
}

on_http_socket_timeout :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
    /* Close idle HTTP sockets */
    return bop.us_socket_close(SSL, s, 0, nil)
}

main :: proc() {
    using bop

    fmt.println("hi")

    loop := us_create_loop(nil, on_wakeup, on_pre, on_post, 0)

    options := US_Socket_Context_Options{}
    options.key_file_name = "key.pem"
    options.cert_file_name = "cert.pem"
    options.passphrase = "1234"

    http_context := us_create_socket_context(SSL, loop, c.int(size_of(HTTP_Context)), options)

    if http_context == nil {
        fmt.println("could not load SSL cert/key")
        return
    }

    body := "<html><body><h1>Why hello there!</h1></body></html>"

    http_context_ext := cast(^HTTP_Context)us_socket_context_ext(SSL, http_context)
    http_context_ext.response = fmt.aprintf("HTTP/1.1 200 OK\r\nContent-Length: %ld\r\n\r\n%s", len(body), body)

    us_socket_context_on_open(SSL, http_context, on_http_socket_open)
    us_socket_context_on_data(SSL, http_context, on_http_socket_data)
    us_socket_context_on_writable(SSL, http_context, on_http_socket_writeable)
    us_socket_context_on_close(SSL, http_context, on_http_socket_close)
    us_socket_context_on_timeout(SSL, http_context, on_http_socket_timeout)
    us_socket_context_on_end(SSL, http_context, on_http_socket_end)

    listen_socket := us_socket_context_listen(SSL, http_context, nil, 3000, 0, c.int(size_of(HTTP_Socket)))

    if listen_socket != nil {
        fmt.println("listening on port 3000...")
        us_loop_run(loop)
    } else {
        fmt.println("failed to listen on port 3000")
    }
}
