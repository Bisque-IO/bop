package bopd

import "core:sync"
import "core:testing"
import "core:thread"

import bop "../odin/libbop"
import time "core:time"

@(test)
test_listener_make :: proc(t: ^testing.T) {
	listener, err := listener_make(nil, 3001, {})
	if err != nil {
		ensure(err != nil)
	}
	listener_delete(listener)

	listener, err = listener_make(nil, 3001, {})
	if err != nil {
		ensure(err != nil)
	}

	th := thread.create_and_start_with_poly_data(
		listener,
		proc(l: ^Server_Listener) {
			listener_run(l)
		},
		context,
	)

	time.sleep(time.Millisecond * 100)

	listener_delete(listener)

	thread.join(th)
	thread.destroy(th)
}

@(test)
test_listener_delete :: proc(t: ^testing.T) {
	listener, err := listener_make(nil, 3002, {})
	if err != nil {
		ensure(err != nil)
	}
	listener_delete(listener)
}
