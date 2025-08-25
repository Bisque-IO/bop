package movd

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:thread"

import bop "../odin/libbop"
import "../odin/snmalloc"

@(thread_local)
local_context: runtime.Context

tls_context_init :: proc "contextless" () -> runtime.Context {
	c := runtime.default_context()
	c.allocator = snmalloc.allocator()
	context = c
	c.logger = log.create_console_logger(.Debug)
	log.info("created thread_local context")
	local_context = c
	return c
}

tls_context :: #force_inline proc "contextless" () -> runtime.Context {
	c := local_context
	if c.allocator.procedure == nil {
		c = tls_context_init()
	}
	return c
}
