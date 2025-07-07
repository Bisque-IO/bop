package bopd

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:mem/virtual"
import "core:thread"

@thread_local
local_context: runtime.Context

load_context :: #force_inline proc "contextless" () -> runtime.Context {
    c := local_context
    if c.allocator.procedure == nil {
        c = runtime.default_context()
        context = c
        context.logger = log.create_console_logger(.Debug)
        c = context
        log.info("created thread_local context")
        local_context = c
    }
    return c
}
