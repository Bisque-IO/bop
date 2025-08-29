package usockets

import bop "../libbop"

RECV_BUFFER_LENGTH      :: bop.US_RECV_BUFFER_LENGTH
TIMEOUT_GRANULARITY     :: bop.US_TIMEOUT_GRANULARITY
RECV_BUFFER_PADDING     :: bop.US_RECV_BUFFER_PADDING
EXT_ALIGNMENT           :: bop.US_EXT_ALIGNMENT
LISTEN_DEFAULT          :: bop.US_LISTEN_DEFAULT
LISTEN_EXCLUSIVE_PORT   :: bop.US_LISTEN_EXCLUSIVE_PORT

Socket                  :: bop.us_socket_t
Listen_Socket           :: bop.us_listen_socket_t
Timer                   :: bop.us_timer_t
Socket_Context          :: bop.us_socket_context_t
Loop                    :: bop.us_loop_t
Poll                    :: bop.us_poll_t
UDP_Socket              :: bop.us_udp_socket_t
UDP_Packet_Buffer       :: bop.us_udp_packet_buffer_t
Socket_Context_Options  :: bop.us_socket_context_options_t

socket_send_buffer :: bop.us_socket_send_buffer

/* Public interface for UDP sockets */

/*
Peeks data and length of UDP payload
*/
udp_packet_buffer_payload :: bop.us_udp_packet_buffer_payload

udp_packet_buffer_payload_length :: bop.us_udp_packet_buffer_payload_length

/*
Copies out local (received destination) ip (4 or 16 bytes) of received packet
*/
udp_packet_buffer_local_ip :: bop.us_udp_packet_buffer_local_ip

/*
Get the bound port in host byte order
*/
udp_socket_bound_port :: bop.us_udp_socket_bound_port

/*
Peeks peer addr (sockaddr) of received packet
*/
udp_packet_buffer_peer :: bop.us_udp_packet_buffer_peer

/*
Peeks ECN of received packet
*/
udp_packet_buffer_ecn :: bop.us_udp_packet_buffer_ecn

/*
Receives a set of packets into specified packet buffer
*/
udp_socket_receive :: bop.us_udp_socket_receive

udp_buffer_set_packet_payload :: bop.us_udp_buffer_set_packet_payload

udp_socket_send :: bop.us_udp_socket_send

/*
Allocates a packet buffer that is reuable per thread. Mutated by us_udp_socket_receive.
*/
create_udp_packet_buffer :: bop.us_create_udp_packet_buffer

create_udp_socket :: bop.us_create_udp_socket

/*
This one is ugly, should be ext! not user
*/
udp_socket_user :: bop.us_udp_socket_user

/*
Binds the UDP socket to an interface and port
*/
udp_socket_bind :: bop.us_udp_socket_bind

/*
Create a new high precision, low performance timer. May fail and return null
*/
create_timer :: bop.us_create_timer

/*
Returns user data extension for this timer
*/
timer_ext :: bop.us_timer_ext

timer_close :: bop.us_timer_close

/*
Arm a timer with a delay from now and eventually a repeat delay.
 Specify 0 as repeat delay to disable repeating. Specify both 0 to disarm.
 */
timer_set :: bop.us_timer_set

/*
Returns the loop for this timer
*/
timer_loop :: bop.us_timer_loop

socket_context_timestamp :: bop.us_socket_context_timestamp

/* Adds SNI domain and cert in asn1 format */

socket_context_add_server_name :: bop.us_socket_context_add_server_name

socket_context_remove_server_name :: bop.us_socket_context_remove_server_name

socket_context_on_server_name :: bop.us_socket_context_on_server_name

socket_server_name_userdata :: bop.us_socket_server_name_userdata

/*
Returns the underlying SSL native handle, such as SSL_CTX or nullptr
*/
socket_context_get_native_handle :: bop.us_socket_context_get_native_handle

/*
A socket context holds shared callbacks and user data extension for associated sockets
*/
create_socket_context :: bop.us_create_socket_context

/*
Delete resources allocated at creation time.
*/
socket_context_free :: bop.us_socket_context_free

/* Setters of various async callbacks */

socket_context_on_pre_open :: bop.us_socket_context_on_pre_open

socket_context_on_open :: bop.us_socket_context_on_open

socket_context_on_close :: bop.us_socket_context_on_close

socket_context_on_data :: bop.us_socket_context_on_data

socket_context_on_writable :: bop.us_socket_context_on_writable

socket_context_on_timeout :: bop.us_socket_context_on_timeout

socket_context_on_long_timeout :: bop.us_socket_context_on_long_timeout

/*
This one is only used for when a connecting socket fails in a late stage.
*/
socket_context_on_connect_error :: bop.us_socket_context_on_connect_error

/*
Emitted when a socket has been half-closed
*/
socket_context_on_end :: bop.us_socket_context_on_end

/*
Returns user data extension for this socket context
*/
socket_context_ext :: bop.us_socket_context_ext

/*
Closes all open sockets, including listen sockets. Does not invalidate the socket context.
*/
socket_context_close :: bop.us_socket_context_close

/*
Listen for connections. Acts as the main driving cog in a server. Will call set async callbacks.
*/
socket_context_listen :: bop.us_socket_context_listen

socket_context_listen_unix :: bop.us_socket_context_listen_unix

listen_socket_close :: bop.us_listen_socket_close

/*
Adopt a socket which was accepted either internally, or from another accept() outside libusockets
*/
adopt_accepted_socket :: bop.us_adopt_accepted_socket

/*
Land in on_open or on_connection_error or return null or return socket
*/
socket_context_connect :: bop.us_socket_context_connect

socket_context_connect_unix :: bop.us_socket_context_connect_unix

/*
Is this socket established? Can be used to check if a connecting socket has
fired the on_open event yet. Can also be used to determine if a socket is a
listen_socket or not, but you probably know that already.
*/
socket_is_established :: bop.us_socket_is_established

/*
Cancel a connecting socket. Can be used together with us_socket_timeout to
limit connection times. Entirely destroys the socket - this function works
like us_socket_close but does not trigger on_close event since you never
got the on_open event first.
*/
socket_close_connecting :: bop.us_socket_close_connecting

/*
Returns the loop for this socket context.
*/
socket_context_loop :: bop.us_socket_context_loop

/*
Invalidates passed socket, returning a new resized socket which belongs to a
different socket context. Used mainly for "socket upgrades" such as when
transitioning from HTTP to WebSocket.
*/
socket_context_adopt_socket :: bop.us_socket_context_adopt_socket

/*
Create a child socket context which acts much like its own socket context with
its own callbacks yet still relies on the parent socket context for some shared
resources. Child socket contexts should be used together with socket adoptions
and nothing else.
*/
create_child_socket_context :: bop.us_create_child_socket_context

/* Public interfaces for loops */

/* Returns a new event loop with user data extension */
create_loop :: bop.us_create_loop

/*
Frees the loop immediately
*/
loop_free :: bop.us_loop_free

/*
Returns the loop user data extension
*/
loop_ext :: bop.us_loop_ext

/*
Blocks the calling thread and drives the event loop until no more non-fallthrough
polls are scheduled
*/
loop_run :: bop.us_loop_run

/*
Signals the loop from any thread to wake up and execute its wakeup handler from
the loop's own running thread. This is the only fully thread-safe function and
serves as the basis for thread safety
*/
wakeup_loop :: bop.us_wakeup_loop

/*
Hook up timers in existing loop
*/
loop_integrate :: bop.us_loop_integrate

/*
Returns the loop iteration number
*/
loop_iteration_number :: bop.us_loop_iteration_number

/* Public interfaces for polls */

/*
A fallthrough poll does not keep the loop running, it falls through
*/
create_poll :: bop.us_create_poll

/*
After stopping a poll you must manually free the memory
*/
poll_free :: bop.us_poll_free

/*
Associate this poll with a socket descriptor and poll type
*/
poll_init :: bop.us_poll_init

/*
Start, change and stop polling for events
*/
poll_start :: bop.us_poll_start
poll_change :: bop.us_poll_change
poll_stop :: bop.us_poll_stop

/*
Return what events we are polling for
*/
poll_events :: bop.us_poll_events

/*
Returns the user data extension of this poll
*/
poll_ext :: bop.us_poll_ext

/*
Get associated socket descriptor from a poll
*/
poll_fd :: bop.us_poll_fd

/*
Resize an active poll
*/
poll_resize :: bop.us_poll_resize

/* Public interfaces for sockets */

/*
Returns the underlying native handle for a socket, such as SSL or file
descriptor. In the case of file descriptor, the value of pointer is fd.
*/
socket_get_native_handle :: bop.us_socket_get_native_handle

/*
Write up to length bytes of data. Returns actual bytes written. Will call
the on_writable callback of active socket context on failure to write
everything off in one go. Set hint msg_more if you have more immediate data
to write.
*/
socket_write :: bop.us_socket_write

/*
Special path for non-SSL sockets. Used to send header and payload in
one go. Works like us_socket_write.
*/
socket_write2 :: bop.us_socket_write2

/*
Set a low precision, high performance timer on a socket. A socket can
only have one single active timer at any given point in time. Will remove
any such pre set timer
*/
socket_timeout :: bop.us_socket_timeout

/*
Set a low precision, high performance timer on a socket. Suitable for
per-minute precision.
*/
socket_long_timeout :: bop.us_socket_long_timeout

/*
Return the user data extension of this socket
*/
socket_ext :: bop.us_socket_ext

/*
Return the socket context of this socket
*/
socket_context :: bop.us_socket_context

/*
Withdraw any msg_more status and flush any pending data
*/
socket_flush :: bop.us_socket_flush

/*
Shuts down the connection by sending FIN and/or close_notify
*/
socket_shutdown :: bop.us_socket_shutdown

/*
Shuts down the connection in terms of read, meaning next event loop
iteration will catch the socket being closed. Can be used to defer
closing to next event loop iteration.
*/
socket_shutdown_read :: bop.us_socket_shutdown_read

/*
Returns whether the socket has been shut down or not
*/
socket_is_shut_down :: bop.us_socket_is_shut_down

/*
Returns whether this socket has been closed. Only valid if memory
has not yet been released.
*/
socket_is_closed :: bop.us_socket_is_closed

/*
Immediately closes the socket
*/
socket_close :: bop.us_socket_close

/*
Returns local port or -1 on failure.
*/
socket_local_port :: bop.us_socket_local_port

/*
Returns remote ephemeral port or -1 on failure.
*/
socket_remote_port :: bop.us_socket_remote_port

/*
Copy remote (IP) address of socket, or fail with zero length.
*/
socket_remote_address :: bop.us_socket_remote_address