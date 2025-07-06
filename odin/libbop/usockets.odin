package libbop

import c "core:c/libc"

US_RECV_BUFFER_LENGTH  :: 524288
US_TIMEOUT_GRANULARITY :: 4
US_RECV_BUFFER_PADDING :: 32
US_EXT_ALIGNMENT       :: 16

US_LISTEN_DEFAULT        :: 0
US_LISTEN_EXCLUSIVE_PORT :: 1

US_Socket            :: struct {}

US_Listen_Socket     :: struct {}

US_Timer             :: struct {}

US_Socket_Context    :: struct {}

US_Loop              :: struct {}

US_Poll              :: struct {}

US_UDP_Socket        :: struct {}

US_UDP_Packet_Buffer :: struct {}

US_Socket_Context_Options :: struct {
    key_file_name:               cstring,
    cert_file_name:              cstring,
    passphrase:                  cstring,
    dh_params_file_name:         cstring,
    ca_file_name:                cstring,
    ssl_ciphers:                 cstring,
    ssl_prefer_low_memory_usage: c.int
}
