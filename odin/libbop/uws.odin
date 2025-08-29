package libbop

import "base:runtime"
import c "core:c/libc"
import "core:fmt"
import "core:mem"
import "core:os"
import "core:time"

uws_app_t :: rawptr
uws_res_t :: rawptr
uws_req_t :: rawptr
uws_web_socket_t :: rawptr

uws_http_header_t :: struct {
	key, value: string,
}

// This can be transmute(US_Loop)
uws_loop_t :: rawptr

uws_loop_defer_handler_t :: #type proc "c" (loop: uws_loop_t, user_data: rawptr)
uws_res_defer_handler_t :: #type proc "c" (loop: uws_loop_t, res: uws_res_t, user_data: rawptr)

uws_ssl_options_t :: struct {
	key_file:                    string,
	cert_file:                   string,
	passphrase:                  string,
	dh_params_file:              string,
	ca_file:                     string,
	ssl_prefer_low_memory_usage: c.int,
}

uws_http_handler_t :: #type proc "c" (user_data: rawptr, res: uws_res_t, req: uws_req_t)

uws_opcode_t :: enum c.int {
	CONTINUATION = 0,
	TEXT         = 1,
	BINARY       = 2,
	CLOSE        = 8,
	PING         = 9,
	PONG         = 10,
}

uws_ws_send_status_t :: enum c.int {
    BACKPRESSURE,
    SUCCESS,
    DROPPED,
}

uws_ws_cork_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr)

uws_ws_upgrade_handler_t :: #type proc "c" (
	res: uws_res_t,
	req: uws_req_t,
	ctx: rawptr,
	per_pattern_user_data: rawptr,
) -> rawptr

uws_ws_create_user_data_t :: #type proc "c" (per_pattern_user_data: rawptr) -> rawptr

uws_ws_open_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr)

uws_ws_message_handler_t :: #type proc "c" (
	ws: uws_web_socket_t,
    user_data: rawptr,
	message: [^]u8,
	length: c.size_t,
	opcode: uws_opcode_t,
)

uws_ws_drain_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr)

uws_ws_ping_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr, message: [^]u8, length: c.size_t)

uws_ws_pong_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr, message: [^]u8, length: c.size_t)

uws_ws_close_handler_t :: #type proc "c" (ws: uws_web_socket_t, user_data: rawptr, message: [^]u8, length: c.size_t)

uws_ws_subscription_handler_t :: #type proc "c" (
    ws: uws_web_socket_t,
	user_data: rawptr,
	topic: cstring,
	subscriptions: c.int,
	old_subscriptions: c.int,
)

uws_ws_destroy_user_data_t :: #type proc "c" (user_data: rawptr)

uws_ws_behavior_t :: struct {
	compression:                 u16,
	idle_timeout:                u16,
	max_payload_length:          u32,
	max_backpressure:            u32,
	max_lifetime:                u16,
	close_on_backpressure_limit: c.bool,
	reset_idle_timeout_on_send:  c.bool,
	send_pings_automatically:    c.bool,
	reserved:                    c.bool,
	reserved_2:                  u16,
	reserved_3:                  u32,
	upgrade:                     uws_ws_upgrade_handler_t,
	create_user_data:            uws_ws_create_user_data_t,
	open:                        uws_ws_open_handler_t,
	message:                     uws_ws_message_handler_t,
	dropped:                     uws_ws_message_handler_t,
	drain:                       uws_ws_drain_handler_t,
	ping:                        uws_ws_ping_handler_t,
	pong:                        uws_ws_pong_handler_t,
	close:                       uws_ws_close_handler_t,
	subscription:                uws_ws_subscription_handler_t,
	destroy_user_data:           uws_ws_destroy_user_data_t,
	per_pattern_user_data:       rawptr,
}

uws_listen_handler_t :: #type proc "c" (listen_socket: rawptr, user_data: rawptr) -> c.int

uws_res_cork_handler_t :: #type proc "c" (res: uws_res_t, user_data: rawptr)
uws_res_on_writable_handler_t :: #type proc "c" (res: uws_res_t, user_data: rawptr, m: u64) -> c.bool
uws_res_on_aborted_handler_t :: #type proc "c" (res: uws_res_t, user_data: rawptr)
uws_res_on_data_handler_t :: #type proc "c" (res: uws_res_t, user_data: rawptr, data: [^]u8, data_length: c.size_t, fin: c.bool)

