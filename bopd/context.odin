package bopd

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:thread"

import bop "../odin/libbop"

@thread_local
local_context: runtime.Context

tls_context :: #force_inline proc "contextless" () -> runtime.Context {
    c := local_context
    if c.allocator.procedure == nil {
        c = runtime.default_context()
        context = c
        context.logger = log.create_console_logger(.Debug)
        context.allocator = bop.snmallocator()
        c = context
        log.info("created thread_local context")
        local_context = c
    }
    return c
}
