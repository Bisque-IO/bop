package libbop

import c "core:c/libc"

US_RECV_BUFFER_LENGTH :: 524288
US_TIMEOUT_GRANULARITY :: 4
US_RECV_BUFFER_PADDING :: 32
US_EXT_ALIGNMENT :: 16

US_LISTEN_DEFAULT :: 0
US_LISTEN_EXCLUSIVE_PORT :: 1

us_socket_t :: struct {}

us_listen_socket_t :: struct {}

us_timer_t :: struct {}

us_socket_context_t :: struct {}

us_loop_t :: struct {}

us_poll_t :: struct {}

us_udp_socket_t :: struct {}

us_udp_packet_buffer_t :: struct {}

us_socket_context_options_t :: struct {
	key_file_name:               cstring,
	cert_file_name:              cstring,
	passphrase:                  cstring,
	dh_params_file_name:         cstring,
	ca_file_name:                cstring,
	ssl_ciphers:                 cstring,
	ssl_prefer_low_memory_usage: c.int,
}
