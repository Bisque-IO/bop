package uws

import "base:intrinsics"
import "base:runtime"
import c "core:c/libc"
import "core:slice"
import "core:strings"
import bop "../libbop"

App :: bop.uws_app_t
Response :: bop.uws_res_t
Request :: bop.uws_req_t
Socket :: bop.uws_socket_t

SSL_Options :: bop.uws_ssl_options_t

HTTP_Handler :: bop.uws_http_handler_t
Opcode :: bop.uws_opcode_t

WS_Upgrade_Handler :: bop.uws_ws_upgrade_handler_t
WS_Create_User_Data_Handler :: bop.uws_ws_create_user_data_t
WS_Open_Handler :: bop.uws_ws_open_handler_t
WS_Message_Handler :: bop.uws_ws_message_handler_t
WS_Drain_Handler :: bop.uws_ws_drain_handler_t
WS_Error_Handler :: bop.uws_ws_drain_handler_t
WS_Ping_Handler :: bop.uws_ws_drain_handler_t
WS_Pong_Handler :: bop.uws_ws_drain_handler_t
WS_Close_Handler :: bop.uws_ws_drain_handler_t
WS_Subscribe_Handler :: bop.uws_ws_drain_handler_t
WS_Destroy_User_Data_Handler :: bop.uws_ws_drain_handler_t

WS_Behavior :: bop.uws_ws_behavior_t

Listen_Handler :: bop.uws_listen_handler_t

app_create :: proc () -> App {
    return bop.uws_create_app()
}

app_ssl_create :: proc (options: SSL_Options) -> App {
    return bop.uws_create_ssl_app(options)
}

app_destroy :: proc (app: App) {
    bop.uws_app_destroy(app)
}

app_get :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_get(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_post :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_post(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_put :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_put(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_del :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_del(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_patch :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_patch(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_options :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_options(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_any :: proc(user_data: rawptr, app: App, pattern: string, handler: HTTP_Handler) {
    bop.uws_app_any(user_data, app, raw_data(pattern), len(pattern), handler)
}

app_ws :: proc(user_data: rawptr, app: App, pattern: string, behavior: WS_Behavior) {
    bop.uws_app_ws(app, raw_data(pattern), len(pattern), behavior)
}

app_listen :: proc (user_data: rawptr, app: App, port: i32, handler: Listen_Handler) {
    bop.uws_app_listen(user_data, app, port, handler)
}

app_run :: proc (app: App) {
    bop.uws_app_run(app)
}

response_write_status :: proc(res: Response, status: string) {
    bop.uws_res_write_status(res, raw_data(status), c.size_t(len(status)))
}

response_write_header :: proc(res: Response, key: string, value: string) {
    bop.uws_res_write_header(
        res,
        raw_data(key),
        c.size_t(len(key)),
        raw_data(value),
        c.size_t(len(value)),
    )
}

response_write :: proc(res: Response, data: string) {
    bop.uws_res_write(res, raw_data(data), c.size_t(len(data)))
}

response_end :: proc(res: Response, data: string, close_connection := false) {
    bop.uws_res_end(res, raw_data(data), c.size_t(len(data)), close_connection)
}

request_get_method :: proc(req: Request) -> string {
    length: c.size_t = 0
    url := bop.uws_req_get_method(req, &length)
    return strings.string_from_ptr(url, int(length))
}

request_get_query :: proc(req: Request) -> string {
    length: c.size_t = 0
    url := bop.uws_req_get_query(req, &length)
    return strings.string_from_ptr(url, int(length))
}

request_get_url :: proc(req: Request) -> string {
    length: c.size_t = 0
    url := bop.uws_req_get_url(req, &length)
    return strings.string_from_ptr(url, int(length))
}