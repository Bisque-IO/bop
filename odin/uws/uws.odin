package uws

import "core:text/regex/optimizer"
import bop "../libbop"
import "base:intrinsics"
import "base:runtime"
import c "core:c/libc"
import "core:mem"
import "core:slice"
import "core:strings"

App                          :: bop.uws_app_t
Loop                         :: bop.uws_loop_t
Timer                        :: ^bop.us_timer_t
Response                     :: bop.uws_res_t
Request                      :: bop.uws_req_t
WS                           :: bop.uws_web_socket_t
SSL_Options                  :: bop.uws_ssl_options_t
HTTP_Handler                 :: bop.uws_http_handler_t
Listen_Handler               :: bop.uws_listen_handler_t
HTTP_Header                  :: bop.uws_http_header_t
Opcode                       :: bop.uws_opcode_t
Loop_Defer_Handler           :: bop.uws_loop_defer_handler_t
Response_Defer_Handler       :: bop.uws_res_defer_handler_t
Response_Cork_Handler        :: bop.uws_res_cork_handler_t
Response_Writable_Handler    :: bop.uws_res_on_writable_handler_t
Response_Aborted_Handler     :: bop.uws_res_on_aborted_handler_t
Response_Data_Handler        :: bop.uws_res_on_data_handler_t
WS_Cork_Handler              :: bop.uws_ws_cork_handler_t
WS_Upgrade_Handler           :: bop.uws_ws_upgrade_handler_t
WS_Create_User_Data_Handler  :: bop.uws_ws_create_user_data_t
WS_Open_Handler              :: bop.uws_ws_open_handler_t
WS_Message_Handler           :: bop.uws_ws_message_handler_t
WS_Drain_Handler             :: bop.uws_ws_drain_handler_t
WS_Ping_Handler              :: bop.uws_ws_ping_handler_t
WS_Pong_Handler              :: bop.uws_ws_pong_handler_t
WS_Close_Handler             :: bop.uws_ws_close_handler_t
WS_Subscribe_Handler         :: bop.uws_ws_subscription_handler_t
WS_Destroy_User_Data_Handler :: bop.uws_ws_destroy_user_data_t
WS_Behavior                  :: bop.uws_ws_behavior_t
WS_Send_Status               :: bop.uws_ws_send_status_t

string_from_ptr :: proc "contextless" (ptr: ^byte, len: int) -> (res: string) {
	return transmute(string)mem.Raw_String{ptr, len}
}

app_create :: proc "c" () -> App {
	return bop.uws_create_app()
}

app_ssl_create :: proc "c" (options: SSL_Options) -> App {
	return bop.uws_create_ssl_app(options)
}

app_destroy :: proc "c" (app: App) {
	bop.uws_app_destroy(app)
}

app_is_ssl :: proc "c" (app: App) -> bool {
    return bop.uws_app_is_ssl(app)
}

app_get :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_get(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_post :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_post(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_put :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_put(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_del :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_del(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_patch :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_patch(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_options :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_options(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_any :: proc "c" (app: App, user_data: rawptr, pattern: string, handler: HTTP_Handler) {
	bop.uws_app_any(app, user_data, raw_data(pattern), len(pattern), handler)
}

app_ws :: proc "c" (app: App, pattern: string, behavior: WS_Behavior) {
	bop.uws_app_ws(app, raw_data(pattern), len(pattern), behavior)
}

app_listen :: proc "c" (
    app: App,
    user_data: rawptr,
    host: string = "",
    port: i32 = 0,
    options: i32 = 0,
    handler: Listen_Handler = nil,
) {
    if len(host) == 0 {
        bop.uws_app_listen(app, user_data, nil, 0, port, options, handler)
    } else {
        bop.uws_app_listen(app, user_data, raw_data(host), c.size_t(len(host)), port, options, handler)
    }
}

app_listen_unix :: proc "c" (
    app: App,
    user_data: rawptr,
    path: string,
    options: i32 = 0,
    handler: Listen_Handler = nil,
) {
    bop.uws_app_listen_unix(app, user_data, raw_data(path), c.size_t(len(path)), options, handler)
}

app_loop :: proc "c" (app: App) -> Loop {
    return bop.uws_app_loop(app)
}

loop_defer :: proc "c" (loop: Loop, user_data: rawptr, handler: Loop_Defer_Handler) {
    bop.uws_loop_defer(loop, user_data, handler)
}

app_run :: proc "c" (app: App) {
	bop.uws_app_run(app)
}

/*
Manually upgrade to WebSocket. Typically called in upgrade handler. Immediately calls open handler.
NOTE: Will invalidate 'this' as socket might change location in memory. Throw away after use.
*/
response_upgrade :: proc "c" (
    res: Response,
    user_data: rawptr,
    sec_web_socket_key: string,
    sec_web_socket_protocol: string,
    sec_web_socket_extensions: string,
    web_socket_context: ^bop.us_socket_context_t,
) {
    bop.uws_res_upgrade(
        res,
        user_data,
        raw_data(sec_web_socket_key),
        c.size_t(len(sec_web_socket_key)),
        raw_data(sec_web_socket_protocol),
        c.size_t(len(sec_web_socket_protocol)),
        raw_data(sec_web_socket_extensions),
        c.size_t(len(sec_web_socket_extensions)),
        web_socket_context,
    )
}

response_loop :: proc "c" (res: Response) -> Loop {
    return bop.uws_res_loop(res)
}

response_defer :: proc "c" (res: Response, user_data: rawptr, handler: Response_Defer_Handler) {
    bop.uws_res_defer(res, user_data, handler)
}

response_pause :: proc "c" (res: Response) {
    bop.uws_res_pause(res)
}

response_resume :: proc "c" (res: Response) {
    bop.uws_res_resume(res)
}

/* Immediately close socket */
response_close :: proc "c" (res: Response) {
    bop.uws_res_close(res)
}

/* Returns SSL pointer or FD as pointer */
response_native_handle :: proc "c" (res: Response) -> rawptr {
    return bop.uws_res_native_handle(res)
}

/*
Returns the remote IP address or empty string on failure.
Returned value is borrowed and attached to current thread:
    static thread_local char buf[16];
*/
response_remote_address :: proc "c" (res: Response) -> string {
    length: c.size_t
    result := bop.uws_res_remote_address(res, &length)
    return string_from_ptr(result, int(length))
}

/* Write 100 Continue, can be done any amount of times */
response_write_continue :: proc "c" (res: Response, status: string) {
	bop.uws_res_write_continue(res)
}

/* Write the HTTP status */
response_write_status :: proc "c" (res: Response, status: string) {
	bop.uws_res_write_status(res, raw_data(status), c.size_t(len(status)))
}

/* Write an HTTP header with string value */
response_write_header :: proc "c" (res: Response, key: string, value: string) {
	bop.uws_res_write_header(res, raw_data(key), c.size_t(len(key)), raw_data(value), c.size_t(len(value)))
}

/* Write an HTTP header with unsigned int value */
response_write_header_int :: proc "c" (res: Response, key: string, value: u64) {
	bop.uws_res_write_header_int(res, raw_data(key), c.size_t(len(key)), value)
}

/* Write parts of the response in chunking fashion. Starts timeout if failed. */
response_write :: proc "c" (res: Response, data: string) {
	bop.uws_res_write(res, raw_data(data), c.size_t(len(data)))
}

/* End without a body (no content-length) or end with a spoofed content-length. */
respond_end_without_body :: proc "c" (
    res: Response,
    reported_content_length: int = 0,
    reported_content_length_is_set: bool = false,
    close_connection: bool = false,
) {
    bop.uws_res_end_without_body(
        res,
        c.size_t(reported_content_length),
        c.bool(reported_content_length_is_set),
        c.bool(close_connection),
    )
}

/* End the response with an optional data chunk. Always starts a timeout. */
response_end :: proc "c" (res: Response, data: string, close_connection: bool = false) {
	bop.uws_res_end(res, raw_data(data), c.size_t(len(data)), close_connection)
}

/*
Try and end the response. Returns [true, true] on success.
Starts a timeout in some cases. Returns [ok, hasResponded]
*/
response_try_end :: proc "c" (
    res: Response,
    total_size: int = 0,
    close_connection: bool = false,
) -> (ok: bool, has_responded: bool) {
    ok = bop.uws_res_try_end(res, c.size_t(total_size), c.bool(close_connection), &has_responded)
    return
}

/* Get the current byte write offset for this Http response */
response_write_offset :: proc "c" (res: Response) -> int {
    return int(bop.uws_res_write_offset(res))
}

/* If you are messing around with sendfile you might want to override the offset. */
response_override_write_offset :: proc "c" (res: Response, offset: int) {
    bop.uws_res_override_write_offset(res, c.size_t(offset))
}

/* Checking if we have fully responded and are ready for another request */
response_has_responded :: proc "c" (res: Response) -> bool {
    return bop.uws_res_has_responded(res)
}

/* Corks the response if possible. Leaves already corked socket be. */
response_cork :: proc "c" (res: Response, user_data: rawptr, handler: Response_Cork_Handler) {
    bop.uws_res_cork(res, user_data, handler)
}

/* Attach handler for writable HTTP response */
response_on_writable :: proc "c" (res: Response, user_data: rawptr, handler: Response_Writable_Handler) {
    bop.uws_res_on_writable(res, user_data, handler)
}

/* Attach handler for aborted HTTP request */
response_on_aborted :: proc "c" (res: Response, user_data: rawptr, handler: Response_Aborted_Handler) {
    bop.uws_res_on_aborted(res, user_data, handler)
}

/* Attach a read handler for data sent. Will be called with FIN set true if last segment. */
response_on_data :: proc "c" (res: Response, user_data: rawptr, handler: Response_Data_Handler) {
    bop.uws_res_on_data(res, user_data, handler)
}

request_get_method :: proc "c" (req: Request) -> string {
	length: c.size_t = 0
	url := bop.uws_req_get_method(req, &length)
	return string_from_ptr(url, int(length))
}

request_get_query :: proc "c" (req: Request) -> string {
	length: c.size_t = 0
	url := bop.uws_req_get_query(req, &length)
	return string_from_ptr(url, int(length))
}

request_get_url :: proc "c" (req: Request) -> string {
	length: c.size_t = 0
	url := bop.uws_req_get_url(req, &length)
	return string_from_ptr(url, int(length))
}

request_get_header :: proc "c" (req: Request, key: string) -> string {
	length: c.size_t = len(key)
	value := bop.uws_req_get_header(req, raw_data(key), &length)
	return string_from_ptr(value, int(length))
}

request_get_header_count :: proc "c" (req: Request) -> int {
	return int(bop.uws_req_get_header_count(req))
}

request_get_header_at :: proc "c" (req: Request, index: int, header: ^HTTP_Header) {
	bop.uws_req_get_header_at(req, c.size_t(index), header)
}

request_get_headers :: proc "c" (req: Request, headers: []HTTP_Header) -> []HTTP_Header {
	count := bop.uws_req_get_headers(req, raw_data(headers), c.size_t(len(headers)))
	return headers[0:int(count)]
}

ws_send :: proc "c" (ws: WS, message: string, opcode: Opcode, compress: bool, fin: bool) -> bool {
	return bop.uws_ws_send(ws, raw_data(message), c.size_t(len(message)), opcode, compress, fin)
}

ws_send_first_fragment :: proc "c" (ws: WS, message: string, opcode: Opcode, compress: bool) -> WS_Send_Status {
	return bop.uws_ws_send_first_fragment(ws, raw_data(message), c.size_t(len(message)), opcode, compress)
}

ws_send_fragment :: proc "c" (ws: WS, message: string, compress: bool) -> WS_Send_Status {
	return bop.uws_ws_send_fragment(ws, raw_data(message), c.size_t(len(message)), compress)
}

ws_send_last_fragment :: proc "c" (ws: WS, message: string, compress: bool) -> WS_Send_Status {
	return bop.uws_ws_send_last_fragment(ws, raw_data(message), c.size_t(len(message)), compress)
}

ws_has_negotiated_compression :: proc "c" (ws: WS) -> bool {
	return bop.uws_ws_has_negotiated_compression(ws)
}

ws_end :: proc "c" (ws: WS, code: i32, message: string) {
	bop.uws_ws_end(ws, c.int(code), raw_data(message), c.size_t(len(message)))
}

ws_close :: proc "c" (ws: WS, code: i32, message: string) {
	bop.uws_ws_close(ws, c.int(code), raw_data(message), c.size_t(len(message)))
}

ws_cork :: proc "c" (ws: WS, user_data: rawptr, handler: WS_Cork_Handler) {
    bop.uws_ws_cork(ws, user_data, handler)
}

ws_subscribe :: proc "c" (ws: WS, topic: string) -> bool {
	return bop.uws_ws_subscribe(ws, raw_data(topic), c.size_t(len(topic)))
}

ws_unsubscribe :: proc "c" (ws: WS, topic: string) -> bool {
	return bop.uws_ws_unsubscribe(ws, raw_data(topic), c.size_t(len(topic)))
}

ws_is_subscribed :: proc "c" (ws: WS, topic: string) -> bool {
	return bop.uws_ws_is_subscribed(ws, raw_data(topic), c.size_t(len(topic)))
}

ws_publish :: proc "c" (ws: WS, topic: string, message: string, opcode: Opcode = .BINARY, compress: bool = false) -> bool {
	return bop.uws_ws_publish(
		ws,
		raw_data(topic),
		c.size_t(len(topic)),
		raw_data(message),
		c.size_t(len(message)),
		opcode,
		compress,
	)
}

ws_get_buffered_amount :: proc "c" (ws: WS) -> u32 {
	return bop.uws_ws_get_buffered_amount(ws)
}

ws_get_remote_address :: proc "c" (ws: WS) -> string {
	length: c.size_t = 0
	addr := bop.uws_ws_get_remote_address(ws, &length)
	return string_from_ptr(addr, int(length))
}

app_publish :: proc "c" (app: App, topic: string, message: string, opcode: Opcode = .BINARY, compress: bool = false) -> bool {
	return bop.uws_app_publish(
		app,
		raw_data(topic),
		c.size_t(len(topic)),
		raw_data(message),
		c.size_t(len(message)),
		opcode,
		compress,
	)
}
