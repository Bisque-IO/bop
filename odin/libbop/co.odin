package libbop

import "base:intrinsics"
import "base:runtime"
import "core:fmt"
import "core:log"
import "core:testing"
import "core:time"
import "core:sys/linux"
import "core:sys/posix"

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
	// desc := Co_Descriptor {
	// 	entry      = coro_entry,
	// 	cleanup    = coro_cleanup,
	// 	// stack      = alloc(65536),
	// 	stack      = zalloc(32768),
	// 	stack_size = 32768,
	// 	udata      = nil,
	// }
	// // defer dealloc(desc.stack)

	// fmt.println("before start")

	// co_start(&desc, false)

	// fmt.println("after start")

	// co_switch(co, false)

	test_perf()
}

ITERS :: 10_000_000

counter := 0

co: ^Co

@(export)
coro_entry :: proc "c" (udata: rawptr) {
    defer co_switch(nil, true)
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
	//	co_switch_quick(nil)
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

master_co: ^Co
perf1co: ^Co
perf2co: ^Co
perfstart := time.tick_now()
perf_dur: time.Duration
perf_count := 0
NUM_YIELDS :: 1000000

perf1 :: proc "c" (udata: rawptr) {
	defer co_switch(master_co, true)

	perf1co = co_current()
	co_switch(master_co, false)
	//	co_switch_quick(nil)

	for perf_count < NUM_YIELDS {
		// co_switch(perf2co, false)
		co_switch_fast(perf1co, perf2co)
		// perf_count += 1
	}
}

perf2 :: proc "c" (udata: rawptr) {
	defer co_switch(master_co, true)

	perf2co = co_current()
	perfstart = time.tick_now()

	for perf_count < NUM_YIELDS {
		// co_switch(perf1co, false)
		co_switch_fast(perf2co, perf1co)
		perf_count += 2
	}

	perf_dur = time.tick_diff(perfstart, time.tick_now())
}

run_perf :: proc "c" () {
    defer co_switch(nil, true)
    master_co = co_current()

	desc1 := Co_Descriptor {
		stack      = alloc(65536),
		stack_size = 65536,
		entry      = perf1,
		cleanup    = coro_cleanup,
		udata      = cast(rawptr)uintptr(1),
	}
	co_start(&desc1, false)

	desc2 := Co_Descriptor {
		stack      = alloc(65536),
		stack_size = 65536,
		entry      = perf2,
		cleanup    = coro_cleanup,
		udata      = cast(rawptr)uintptr(2),
	}
	co_start(&desc2, false)
}

test_perf :: proc() {
    for i in 0..<50 {
        perf_count = 0
        master_desc := Co_Descriptor {
    		stack      = alloc(65536),
    		stack_size = 65536,
    		entry      = perf1,
    		cleanup    = coro_cleanup,
    		udata      = cast(rawptr)uintptr(1),
    	}
    	co_start(&master_desc, false)

        perfstart = time.tick_now()
        run_perf()
        fmt.printf("perf: %d switches in %s, %d / switch\n", perf_count, perf_dur, perf_dur / NUM_YIELDS)
    }
}
