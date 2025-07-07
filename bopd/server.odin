package bopd

import bop "../odin/libbop"
import "base:runtime"
import c "core:c/libc"
import "core:strings"
import "core:fmt"
import "core:log"
import "core:sync"
import "core:time"

Server_Listener_State :: enum {
	Opened	  = 0,
	Running   = 1,
	Stopping  = 2,
	Stopped   = 3,
}

/*

*/
Server_Listener :: struct {
	mu:				sync.Mutex,
	allocator: 		runtime.Allocator,
	host: 			string,
	port: 			u16,
	loop: 			^bop.US_Loop,
	socket_context: ^bop.US_Socket_Context,
	listen_socket:  ^bop.US_Listen_Socket,
	ctx: 			runtime.Context,
	ssl_options: 	bop.US_Socket_Context_Options,
	state:			Server_Listener_State,
	ssl: 			c.int,
	sockets_mu:		sync.RW_Mutex,
	sockets:		map[uintptr]^Server_Socket
}

Server_Context :: struct {
	listener: ^Server_Listener,
	buf:      [dynamic]byte,
	offset:   int,
}

Server_Socket :: struct {
	buf:    [dynamic]byte,
	offset: int,
}

server_on_wakeup :: proc "c" (loop: ^bop.US_Loop) {
//	log.debug("server_on_wakeup")
}

server_on_pre :: proc "c" (loop: ^bop.US_Loop) {}

server_on_post :: proc "c" (loop: ^bop.US_Loop) {}

//////////////////////////////////////////////////////////////////////////////////
//
// on_writeable
//
///////////////////////////////////////

server_on_writeable_ssl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_writeable(1, s)
}

server_on_writeable_nossl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_writeable(0, s)
}

server_on_writeable :: proc($SSL: c.int, s: ^bop.US_Socket) -> ^bop.US_Socket {
	socket := cast(^Server_Socket)bop.us_socket_ext(SSL, s)
	socket_context := cast(^Server_Context)bop.us_socket_context_ext(
		SSL,
		bop.us_socket_context(SSL, s),
	)

	// Stream whatever is remaining of the response
//	socket.offset += bop.us_socket_write(
//		SSL,
//		s,
//		raw_data(socket_context.response[socket.offset:]),
//		c.int(int(len(socket_context.response)) - int(socket.offset)),
//		0,
//	)

	return s
}

//////////////////////////////////////////////////////////////////////////////////
//
// on_close
//
///////////////////////////////////////

server_on_close_ssl :: proc "c" (
	s: ^bop.US_Socket,
	code: c.int,
	reason: rawptr,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_close(1, s, code, reason)
}

server_on_close_nossl :: proc "c" (
	s: ^bop.US_Socket,
	code: c.int,
	reason: rawptr,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_close(0, s, code, reason)
}

server_on_close :: proc(
	$SSL: c.int,
	s: ^bop.US_Socket,
	code: c.int,
	reason: rawptr,
) -> ^bop.US_Socket {
	fmt.println("client disconnected")
	return s
}

//////////////////////////////////////////////////////////////////////////////////
//
// on_end
//
///////////////////////////////////////

server_on_end_ssl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_end(1, s)
}

server_on_end_nossl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_end(0, s)
}

server_on_end :: proc($SSL: c.int, s: ^bop.US_Socket) -> ^bop.US_Socket {
	// HTTP does not support half-closed sockets
	bop.us_socket_shutdown(SSL, s)
	return bop.us_socket_close(SSL, s, 0, nil)
}

//////////////////////////////////////////////////////////////////////////////////
//
// on_data
//
///////////////////////////////////////

server_on_data_ssl :: proc "c" (
	s: ^bop.US_Socket,
	data: [^]byte,
	length: c.int,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_data(1, s, data, length)
}

server_on_data_nossl :: proc "c" (
	s: ^bop.US_Socket,
	data: [^]byte,
	length: c.int,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_data(0, s, data, length)
}

server_on_data :: proc(
	$SSL: c.int,
	s: ^bop.US_Socket,
	data: [^]byte,
	length: c.int,
) -> ^bop.US_Socket {
	client_socket := cast(^Server_Socket)bop.us_socket_ext(SSL, s)
	server_context := cast(^Server_Context)bop.us_socket_context_ext(
		SSL,
		bop.us_socket_context(SSL, s),
	)

	fmt.println("on_data: size=", length)
	fmt.println(string(data[0:int(length)]))


//	log.debugf("on_data: %s", string(data[:length]))


	// We treat all data events as a request
//	client_socket.offset = bop.us_socket_write(
//		SSL,
//		s,
//		raw_data(socket_context.response),
//		c.int(len(socket_context.response)),
//		0,
//	)

	// Reset idle timer
	bop.us_socket_timeout(SSL, s, 30)

	return s
}

//////////////////////////////////////////////////////////////////////////////////
//
// on_open
//
///////////////////////////////////////

server_on_open_ssl :: proc "c" (
	s: ^bop.US_Socket,
	is_client: c.int,
	ip: [^]byte,
	ip_length: c.int,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_open(1, s, is_client, ip, ip_length)
}

server_on_open_nossl :: proc "c" (
	s: ^bop.US_Socket,
	is_client: c.int,
	ip: [^]byte,
	ip_length: c.int,
) -> ^bop.US_Socket {
	context = load_context()
	return server_on_open(0, s, is_client, ip, ip_length)
}

server_on_open :: proc(
	$SSL: c.int,
	s: ^bop.US_Socket,
	is_client: c.int,
	ip: [^]byte,
	ip_length: c.int,
) -> ^bop.US_Socket {
	client_socket := cast(^Server_Socket)bop.us_socket_ext(SSL, s)

	// Reset offset
	client_socket.offset = 0

	// Timeout idle HTTP connections
	bop.us_socket_timeout(SSL, s, 30)

	fmt.println("client connected")

	return s
}

//////////////////////////////////////////////////////////////////////////////////
//
// on_timeout
//
///////////////////////////////////////

server_on_timeout_ssl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_timeout(1, s)
}

server_on_timeout_nossl :: proc "c" (s: ^bop.US_Socket) -> ^bop.US_Socket {
	context = load_context()
	return server_on_timeout(0, s)
}

server_on_timeout :: proc($SSL: c.int, s: ^bop.US_Socket) -> ^bop.US_Socket {
	// Close idle sockets
	return bop.us_socket_close(SSL, s, 0, nil)
}

//////////////////////////////////////////////////////////////////////////////////
//
// setup listener
//
///////////////////////////////////////

/*
Atomically get current state.
*/
listener_state :: proc(listener: ^Server_Listener) -> Server_Listener_State {
	sync.mutex_guard(&listener.mu)
	return listener.state
}

Listener_Make_Error_Code :: enum {
	Success      = 0,
	Create_Loop  = 1,
	SSL			 = 2,
	Port_Taken	 = 3,
}

Listener_Make_Error :: union #shared_nil {
	runtime.Allocator_Error,
	Listener_Make_Error_Code,
}

/*
Makes a new listener and starts listening on the port.
*/
listener_make :: proc(
	host: cstring,
	port: u16,
	ssl_options: bop.US_Socket_Context_Options,
	allocator := context.allocator,
) -> (
	listener: ^Server_Listener,
	err: Listener_Make_Error,
) {
	loop := bop.us_create_loop(nil, server_on_wakeup, server_on_pre, server_on_post, 0)
	if loop == nil {
		return nil, Listener_Make_Error_Code.Create_Loop
	}

	SSL : c.int = 1 if len(ssl_options.cert_file_name) > 0 else 0

	// create socket context
	socket_context := bop.us_create_socket_context(
		SSL,
		loop,
		c.int(size_of(Server_Context)),
		ssl_options,
	)

	if socket_context == nil {
		bop.us_loop_free(loop)
		return nil, Listener_Make_Error_Code.SSL
	}

	// init server context on the socket context
	server_context := cast(^Server_Context)bop.us_socket_context_ext(SSL, socket_context)
	server_context^ = Server_Context{}

	// wire callbacks
	bop.us_socket_context_on_open(SSL, socket_context,
		server_on_open_ssl if SSL == 1 else server_on_open_nossl)
	bop.us_socket_context_on_data(SSL, socket_context,
		server_on_data_ssl if SSL == 1 else server_on_data_nossl)
	bop.us_socket_context_on_writable(SSL, socket_context,
		server_on_writeable_ssl if SSL == 1 else server_on_writeable_nossl)
	bop.us_socket_context_on_close(SSL, socket_context,
		server_on_close_ssl if SSL == 1 else server_on_close_nossl)
	bop.us_socket_context_on_timeout(SSL, socket_context,
		server_on_timeout_ssl if SSL == 1 else server_on_timeout_nossl)
	bop.us_socket_context_on_end(SSL, socket_context,
		server_on_end_ssl if SSL == 1 else server_on_end_nossl)

	// create listener socket and start listening
	listen_socket := bop.us_socket_context_listen(
		SSL,
		socket_context,
		host,
		c.int(port),
		0,
		c.int(size_of(Server_Socket))
	)
	if listen_socket != nil {
		log.infof("listening on port %d", port)

		listener, err = new(Server_Listener, allocator)
		if err != nil {
			bop.us_socket_context_close(SSL, socket_context)
			bop.us_loop_free(loop)
			bop.us_listen_socket_close(SSL, listen_socket)
			return nil, err
		}

		server_context.listener = listener
		listener.allocator = allocator
		listener.state = .Opened
		listener.host = strings.clone_from_cstring(host, allocator)
		listener.port = port
		listener.loop = loop
		listener.listen_socket = listen_socket
		listener.socket_context = socket_context
		listener.ssl = SSL
		listener.ssl_options = ssl_options

		return listener, nil
	} else {
		log.errorf("failed to listen on port %d", port)

		bop.us_socket_context_close(SSL, socket_context)
		bop.us_loop_free(loop)

		return nil, Listener_Make_Error_Code.Port_Taken
	}
}

/*
Runs the underlying usockets loop and poll until completion.
This blocks the current thread until the loop and poll are stopped.
Only valid to call directly after @ref listener_make

@return true on success or false on failure
*/
listener_run :: proc(listener: ^Server_Listener) -> bool {
	ensure(listener != nil)
	listener.ctx = load_context()
	{
		sync.mutex_guard(&listener.mu)
		if listener.state != .Opened do return false
		listener.state = .Running
	}
	defer {
		sync.mutex_guard(&listener.mu)
		log.info("listener stopped")

		if listener.listen_socket != nil {
			bop.us_listen_socket_close(listener.ssl, listener.listen_socket)
			listener.listen_socket = nil
		}
		if listener.socket_context != nil {
			bop.us_socket_context_close(listener.ssl, listener.socket_context)
			listener.socket_context = nil
		}
		if listener.state == .Stopping {
			listener.state = .Stopped
		} else {
			listener.state = .Stopped
		}


	}
	bop.us_loop_run(listener.loop)
	return true
}

/*
Stops the listener and cleans up resources except for Server_Listener instance.
That must be freed by calling one of the following:
@ref listener_delete
@ref listener_delete_with_timeout

@return true if successful or false if not
*/
listener_stop :: proc(listener: ^Server_Listener) -> bool {
	ensure(listener != nil)

	sync.mutex_guard(&listener.mu)
	if listener.state == .Stopped {
		return true
	}
	if listener.state == .Stopping {
		return false
	}
	if listener.listen_socket != nil {
		bop.us_listen_socket_close(listener.ssl, listener.listen_socket)
		listener.listen_socket = nil
	}
	if listener.socket_context != nil {
		bop.us_socket_context_close(listener.ssl, listener.socket_context)
		listener.socket_context = nil
	}

	if listener.state == .Running {
		listener.state = .Stopping
		log.info("listener stopping...")
		return false
	}

	listener.state = .Stopped
	log.info("listener stopped")
	return true
}

/*
Ensures listener is stopped, waiting until it is stopped.

@param listener listener to delete

@return true if deleted or false if timed out

*/
listener_delete :: proc(listener: ^Server_Listener) {
	listener_delete_with_timeout(listener, 0)
}

/*
Ensures listener is stopped, waiting until it is stopped or the supplied timeout elapses.

@param listener listener to delete
@param duration max time to wait

@return true if deleted or false if timed out
*/
listener_delete_with_timeout :: proc(listener: ^Server_Listener, duration: time.Duration) -> bool {
	if listener == nil {
		return true
	}

	if duration > 0 {
		start := time.tick_now()
		for !listener_stop(listener) {
			time.sleep(time.Millisecond*100)
			if time.tick_diff(start, time.tick_now()) >= duration {
				return false
			}
		}
	} else {
		for !listener_stop(listener) {
			time.sleep(time.Millisecond*100)
		}
	}

	context.allocator = listener.allocator
	delete(listener.host)
	delete(listener.sockets)
	if listener.loop != nil {
		bop.us_loop_free(listener.loop)
		listener.loop = nil
	}
	free(listener)
	log.info("listener deleted")
	return true
}
