package libbop

import "base:runtime"
import "core:fmt"
import "core:log"
import "core:testing"
import "core:time"

Co :: struct {}

Co_Entry :: #type proc "c" (udata: rawptr)

Co_Cleanup :: #type proc "c" (stack: rawptr, stack_size: uintptr, udata: rawptr)

Co_Descriptor :: struct {
    stack:      rawptr,
    stack_size: uintptr,
    entry:      Co_Entry,
    cleanup:    Co_Cleanup,
    udata:      rawptr,
}

bench_co :: proc() {
    desc := Co_Descriptor {
        entry      = coro_entry,
        cleanup    = coro_cleanup,
        stack      = alloc(65536),
        stack_size = 65536,
        udata      = nil,
    }
    // defer dealloc(desc.stack)

    fmt.println("before start")

    co_start(&desc, false)

    fmt.println("after start")

    co_switch(co, false)

    test_perf()
}

ITERS :: 1_000_000

counter := 0

co: ^Co

@(export)
coro_entry :: proc "c" (udata: rawptr) {
    co = co_current()
    context = runtime.default_context()
    context.logger = log.create_console_logger()
    defer log.destroy_console_logger(context.logger)

    defer {
        fmt.println("coro_entry defer statement")
        co_switch(nil, true)
    }

    // for i in 0..<ITERS {
    //     llco_switch(nil, false)
    //     counter += 1
    // }
    fmt.println("coroutine 1!")
    // coro.yield(co)
    co_switch(nil, false)
    // llco.llco_switch(nil, false)
    fmt.println("coroutine 2!")
    log.debug("coroutine 2!")
}

coro_cleanup :: proc "c" (stack: rawptr, stack_size: uintptr, udata: rawptr) {
    context = runtime.default_context()
    fmt.println("cleanup")
    dealloc(stack)
}

STACK_SIZE :: 65536

perf1co: ^Co
perf2co: ^Co
perfstart := time.tick_now()
perf_dur: time.Duration
perf_count := 0
NUM_YIELDS :: 1000000

perf1_do_swap :: proc "c" () {
    co_switch(perf2co, false)
}

perf1 :: proc "c" (udata: rawptr) {
    defer co_switch(nil, true)
    perf1co = co_current()
    co_switch(nil, false)

    for perf_count < NUM_YIELDS {
        perf_count += 1
        //		llco.swap(perf2co, false)
        perf1_do_swap()
    }
}

perf2 :: proc "c" (udata: rawptr) {
    defer co_switch(nil, true)
    perf2co = co_current()
    perfstart = time.tick_now()

    for perf_count < NUM_YIELDS {
        perf_count += 1
        co_switch(perf1co, false)
    }

    perf_dur = time.tick_diff(perfstart, time.tick_now())
}

test_perf :: proc() {
    perfstart = time.tick_now()

    desc1 := Co_Descriptor {
        stack      = alloc(STACK_SIZE),
        stack_size = STACK_SIZE,
        entry      = perf1,
        cleanup    = coro_cleanup,
        udata      = cast(rawptr)uintptr(1),
    }
    co_start(&desc1, false)

    desc2 := Co_Descriptor {
        stack      = alloc(STACK_SIZE),
        stack_size = STACK_SIZE,
        entry      = perf2,
        cleanup    = coro_cleanup,
        udata      = cast(rawptr)uintptr(2),
    }
    co_start(&desc2, false)

    fmt.printf(
    "perf: %d switches in %s, %d / switch\n",
    perf_count,
    perf_dur,
    perf_dur / NUM_YIELDS,
    )
}