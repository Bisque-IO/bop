package executor

import "base:intrinsics"
import "base:runtime"
import "core:debug/trace"
import "core:fmt"
import "core:simd"
import "core:sync"
import "core:testing"
import "core:thread"
import "core:time"

find_first_set_bit_x8 :: proc(bitmap: []simd.u64x8) -> int {
	for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != ZERO_U64X8 {
			return block_index
		}
	}
	return -1
}

find_first_set_bit_x4 :: proc(bitmap: []simd.u64x4) -> int {
	for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != ZERO_U64X4 {
			return block_index
		}
	}
	return -1
}

find_first_set_bit_x1 :: proc(bitmap: []u64) -> (int, int) {
	#no_bounds_check for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != 0 {
			return block_index, int(intrinsics.count_trailing_zeros(bitmap[block_index]))
		}
	}
	return -1, -1
}

find_first_set_bit_x4_2 :: proc(bitmap: []simd.u64x4) -> (int, int, int) {
	for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != ZERO_U64X4 {
			ctz := intrinsics.count_trailing_zeros(bitmap[block_index])
			ctz_p := simd.to_array_ptr(&ctz)
			#unroll for i in 0 ..< 8 {
				val := ctz_p[i]
				if val != 64 {
					return block_index, block_index * 8 + i, block_index * 512 + (i * 64 + int(val))
				}
			}
			return block_index, block_index * 4, 0
		}
	}
	return -1, -1, -1
}

//find_first_set_bit_x8_2 :: #force_inline proc "contextless" (
//	bitmap: []simd.u64x8,
//) -> (
//	int,
//	int,
//	int,
//) #no_bounds_check {
//	#no_bounds_check for block_index in 0 ..< len(bitmap) {
//		if bitmap[block_index] != ZERO_U64X8 {
//			ctz := intrinsics.count_trailing_zeros(bitmap[block_index])
//			ctz_p := simd.to_array_ptr(&ctz)
//			#unroll for i in 0 ..< 8 {
//				val := ctz_p[i]
//				if val != 64 {
//					return block_index,
//						block_index * 8 + i,
//						block_index * 512 + (i * 64 + int(val))
//				}
//			}
//			return block_index, block_index * 8, 0
//		}
//	}
//	return -1, -1, -1
//}

find_first_set_bit_x8_2 :: #force_inline proc "contextless" (
	bitmap: []simd.u64x8,
) -> (
	int,
	int,
	int,
) #no_bounds_check {
	#no_bounds_check for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != ZERO_U64X8 {
			//			ctz := intrinsics.count_trailing_zeros(bitmap[block_index])
			ctz_p := simd.to_array_ptr(&bitmap[block_index])
			#unroll for i in 0 ..< 8 {
				val := ctz_p[i]
				if val != 0 {
					//					return block_index, block_index * 8 + i, block_index * 512 + (i * 64 + int(intrinsics.count_trailing_zeros(val)))
					return block_index, block_index * 8 + i, int(intrinsics.count_trailing_zeros(val))
				}
			}
			return block_index, block_index * 8, 0
		}
	}
	return -1, -1, -1
}

find_first_set_bit_x8_16 :: #force_inline proc "contextless" (
	$SIZE: int,
	bitmap: ^[SIZE]simd.u64x8,
) -> (
	int,
	int,
	int,
) #no_bounds_check {

	#unroll for block_index in 0 ..< len(bitmap) {
		if bitmap[block_index] != ZERO_U64X8 {
			//			ctz := intrinsics.count_trailing_zeros(bitmap[block_index])
			//			ctz_p := simd.to_array_ptr(&bitmap[block_index])
			lanes := simd.to_array_ptr(&bitmap[block_index])
			#unroll for i in 0 ..< len(lanes) {
				val := lanes[i]
				if val != 0 {
					//					return block_index, block_index * 8 + i, block_index * 512 + (i * 64 + int(intrinsics.count_trailing_zeros(val)))
					return block_index, block_index * 8 + i, int(intrinsics.count_trailing_zeros(val))
				}
			}
			return block_index, block_index * 8, 0
		}
	}
	return -1, -1, -1
}

main2 :: proc() {
	value := u64(0)
	signal := Signal(&value)
	signal_39 := signal^

	benchmark(
		1,
		10,
		1000000,
		value,
		proc(data: u64, thread_num: int, cycle_num: int, iterations: int) {
			signal_nearest_branchless(data, 41)
			signal_nearest_branchless(data, 15)
			signal_nearest_branchless(data, 63)
		},
	)

	benchmark(
		1,
		10,
		1000000,
		value,
		proc(data: u64, thread_num: int, cycle_num: int, iterations: int) {
			signal_nearest(data, 41)
			signal_nearest(data, 15)
			signal_nearest(data, 63)
		},
	)

	//	main2()
}

main1 :: proc() #no_bounds_check {
	selector: Selector

	selector_init(&selector)

	for i in 0 ..< 5 {
		fmt.println(selector_next(&selector))
	}

	benchmark(
		1,
		10,
		5000000,
		&selector,
		proc(data: ^Selector, thread_num: int, cycle_num: int, iterations: int) {
			selector_next(data)
		},
	)

	fmt.println(selector)


	fmt.println("u64x8 -> find index 9")
	bitmap := make([]simd.u64x8, 16)
	bitmap[15] = simd.u64x8{0, 0, 0, 0, 0, 0, 0, 897898790454654654}

	fmt.println("count_trailing_zeroes", intrinsics.count_trailing_zeros(bitmap[15]))
	fmt.println("count_trailing_zeroes", intrinsics.count_leading_zeros(bitmap[15]))
	fmt.println(
		"reduce_min",
		intrinsics.simd_reduce_min(intrinsics.count_leading_zeros(bitmap[15])),
	)
	fmt.println("abs_index", find_first_set_bit_x8_2(bitmap))

	benchmark(3, 10, 5000000, bitmap, proc(data: []simd.u64x8, tid: int, cid: int, iter: int) {
		data := (cast(^[16]simd.u64x8)raw_data(data))
		block_index, _, _ := find_first_set_bit_x8_16(16, data)
		if tid == 0 {
			simd.to_array_ptr(&data[15])[7] = u64(iter) + 1
		}
		if block_index != 15 {
			panic("x != 15")
		}
	})

	bitmap_x8x16 := [16]simd.u64x8 {
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		ZERO_U64X8,
		{0, 0, 0, 0, 0, 0, 0, 878978},
	}

	Args :: struct #align (256) {
		table:   ^[16]simd.u64x8,
		counter: u64,
	}

	args := [16]Args {
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
		{table = &bitmap_x8x16, counter = 0},
	}

	benchmark(
		4,
		10,
		5000000,
		&args,
		proc(args: ^[16]Args, thread_num: int, cycle_num: int, iterations: int) {
			#no_bounds_check {
				arg := &args[thread_num]
				data := arg.table
				for block_index in 0 ..< len(data) {
					if data[block_index] != ZERO_U64X8 {
						ctz_p := simd.to_array_ptr(&data[block_index])
						#unroll for i in 0 ..< 8 {
							val := ctz_p[i]
							if val != 0 {
								arg.counter += 1
								return
							}
						}
						return
					}
				}
			}
		},
	)

	fmt.printfln("%d", args[0].counter)

	//	fmt.println("u64x4 -> find index 18")
	//	bitmap4 := make([]simd.u64x4, 32)
	//	bitmap4[18] = simd.u64x4{0, 0, 1, 0}
	//
	//	benchmark(
	//		8,
	//		10,
	//		500000,
	//		bitmap4,
	//		proc(data: []simd.u64x4, thread_num: int, cycle_num: int, iterations: int) {
	//			//		fmt.println(find_first_set_bit_x8(data))
	//			block_index, _, _ := find_first_set_bit_x4_2(data)
	//			//		if x == 1000 {
	//			//			fmt.println("")
	//			//		}
	//			if block_index != 18 {
	//				panic("x != 18")
	//			}
	//		},
	//	)

	//	fmt.println("u64x1 -> find index 72")
	//	bitmap1 := make([]u64, 128)
	//	bitmap1[72] = 1
	//
	//	benchmark(
	//		8,
	//		10,
	//		5000000,
	//		bitmap1,
	//		proc(data: []u64, thread_num: int, cycle_num: int, iterations: int) {
	//			//		fmt.println(find_first_set_bit_x8(data))
	//			signal_index, core_index := find_first_set_bit_x1(data)
	//			if signal_index != 72 {
	//				panic("signal_index != 72")
	//			}
	//		},
	//	)

	//	main2()
}


main :: proc() {
	//	fmt.println("u64x8 -> find index 9")
	//	bitmap := make([]simd.u64x8, 16)
	//	bitmap[9] = simd.u64x8{0, 0, 1, 0, 0, 0, 0, 0}
	//
	//	benchmark(1, 10, 500, bitmap, proc(data: []simd.u64x8, thread_num: int, cycle_num: int, iterations: int) {
	//		fmt.println(find_first_set_bit_x8(data))
	//	})

	mu: sync.Atomic_Mutex
	benchmark(
		1,
		10,
		10000000,
		&mu,
		proc(data: ^sync.Atomic_Mutex, thread_num: int, cycle_num: int, iterations: int) {
			sync.lock(data)
			sync.unlock(data)
		},
	)

	fmt.println("")

	//	Mu :: struct #align (128) {
	//		m: sync.Ticket_Mutex,
	//	}
	//	locks: [32]Mu
	//	benchmark(
	//		32,
	//		10,
	//		1000000,
	//		&locks,
	//		proc(data: ^[32]Mu, thread: int, cycles: int, iterations: int) {
	//			sync.lock(&data[thread].m)
	//			sync.unlock(&data[thread].m)
	//		},
	//	)
	//
	//	lock: sync.Atomic_Mutex
	//	benchmark(
	//		4,
	//		10,
	//		10000000,
	//		&lock,
	//		proc(data: ^sync.Atomic_Mutex, thread: int, cycles: int, iterations: int) {
	//			sync.lock(data)
	//			sync.unlock(data)
	//		},
	//	)

	signal: Signal
	//    benchmark(4, 10, 1000000,
	//        &signal,
	//        proc(data: ^Signal, thread: int, cycle_num: int, iterations: int) {
	//            signal_set(data, u64(0))
	//        }
	//    )

	//	cpu := new(Executor_64, runtime.heap_allocator())
	executor, err := executor_make(64, false)
	if err != nil {
		panic("err")
	}
	group := &executor.groups[0]
	group.waker = new(Waker)
	slot := &group.slots[0]
	slot.cpu_time_enabled = false
	slot.waker = group.waker
	slot.signal = Signal(&simd.to_array_ptr(&group.signals)[0])
	slot.worker = proc(data: rawptr) -> u8 {
		//		time.tick_now()
		return transmute(u8)Slot_Flags.Scheduled
	}
	slot_execute(slot)
	//	benchmark(
	//		32,
	//		10,
	//		50000000,
	//		core,
	//		proc(data: ^VCore, thread: int, cycles: int, iterations: int) {
	//			schedule(data)
	//			//            intrinsics.cpu_relax()
	//		},
	//	)

	//	benchmark(
	//		1,
	//		5,
	//		10000000,
	//		core,
	//		proc(data: ^Slot, thread: int, cycles: int, iterations: int) {
	//			//		time.tick_now()
	////			x := intrinsics.read_cycle_counter()
	//		},
	//	)

	fmt.println("benching vcore_execute")
	benchmark(
		1,
		5,
		10000000,
		slot,
		proc(data: ^Slot, thread: int, cycles: int, iterations: int) {
			slot_execute(data)
			//			intrinsics.cpu_relax()
		},
	)

	stop: bool
	th := thread.create_and_start_with_poly_data2(
	slot,
	&stop,
	proc(core: ^Slot, stop: ^bool) {
		for !stop^ {
			if signal_acquire(core.signal, core.index) {
				slot_execute(core)
			} else {
				intrinsics.cpu_relax()
			}

			//			vcore_execute(core)
		}
	},
	)

	sw: time.Stopwatch
	time.stopwatch_start(&sw)
	benchmark(
		1,
		10,
		100000000,
		slot,
		proc(data: ^Slot, thread: int, cycles: int, iterations: int) {
			//			if ! {
			//				intrinsics.cpu_relax()
			//			}

			slot_schedule(data)
		},
	)
	time.stopwatch_stop(&sw)

	stop = true
	dur := time.stopwatch_duration(sw)
	//	intrinsics.atomic_store(&stop, true)
	thread.join(th)
	thread.destroy(th)

	fmt.println("core executes:", slot.counter, " in:", dur)

	//	benchmark(
	//		8,
	//		10,
	//		1000000,
	//		core,
	//		proc(data: ^Vcore, thread: int, cycles: int, iterations: int) {
	//			//			time.tick_now()
	//			time.read_cycle_counter()
	//			//			time.now()
	//		},
	//	)
}

// A bounded MPSC queue, based on Dmitry Vyukov's MPMC queue and tachyonix Rust based MPSC

// On x86-64, aarch64, and powerpc64, N = 128.
// On arm, mips, mips64, sparc, and hexagon, N = 32.
// On m68k, N = 16.
// On s390x, N = 256.
// On all others, N = 64.
//
// Note that N is just a reasonable guess and is not guaranteed to match the actual cache line
// length of the machine the program is running on. On modern Intel architectures, spatial prefetcher
// is pulling pairs of 64-byte cache lines at a time, so we pessimistically assume that cache lines
// are 128 bytes long.
//
// Note that arm32 is most likely 32 bytes, but have read conflicting information of 32, 64 and 128.
// Let's play it extra safe.
when ODIN_ARCH == .amd64 || ODIN_ARCH == .arm64 || ODIN_ARCH == .arm32 {
	CACHE_LINE_SIZE :: 64
} else {
	CACHE_LINE_SIZE :: 64
}

ContendedU64 :: struct #align (CACHE_LINE_SIZE) {
	value: u64,
}

Executor_1 :: Executor(1, true)
Executor_2 :: Executor(2, true)
Executor_4 :: Executor(4, true)
Executor_8 :: Executor(8, true)
Executor_16 :: Executor(16, true)
Executor_32 :: Executor(32, true)
Executor_64 :: Executor(64, true)
Executor_128 :: Executor(128, true)
Executor_256 :: Executor(256, true)
Executor_512 :: Executor(512, true)
Executor_1024 :: Executor(1024, true)

Executor_1_NonBlocking :: Executor(1, false)
Executor_2_NonBlocking :: Executor(2, false)
Executor_4_NonBlocking :: Executor(4, false)
Executor_8_NonBlocking :: Executor(8, false)
Executor_16_NonBlocking :: Executor(16, false)
Executor_32_NonBlocking :: Executor(32, false)
Executor_64_NonBlocking :: Executor(64, false)
Executor_128_NonBlocking :: Executor(128, false)
Executor_256_NonBlocking :: Executor(256, false)
Executor_512_NonBlocking :: Executor(512, false)
Executor_1024_NonBlocking :: Executor(1024, false)

Bench_Result :: struct {}

benchmark :: proc(
	$THREADS: int,
	$CYCLES: int,
	$ITERS: int,
	data: $T,
	op: proc(data: T, thread_num: int, cycle_num: int, iterations: int),
) {
	Thread_Data :: struct {
		data:       T,
		thread_num: int,
		cycle_num:  int,
		op:         proc(data: T, thread_num: int, cycle_num: int, iterations: int),
		counter:    ^ContendedU64,
	}

	cycles: [CYCLES][THREADS]ContendedU64

	for c in 0 ..< CYCLES {
		stopwatch: time.Stopwatch
		time.stopwatch_start(&stopwatch)

		if THREADS == 1 {
			sw: time.Stopwatch
			time.stopwatch_start(&sw)
			for i in 0 ..< ITERS {
				op(data, 0, c, i)
			}
			time.stopwatch_stop(&sw)
			intrinsics.atomic_store_explicit(
				&cycles[c][0].value,
				u64(time.stopwatch_duration(sw)),
				.Seq_Cst,
			)
		} else {
			producers: [THREADS]^thread.Thread

			for i in 0 ..< THREADS {
				producers[i] = thread.create_and_start_with_poly_data(
					Thread_Data {
						data = data,
						cycle_num = c,
						thread_num = i,
						op = op,
						counter = &cycles[c][i],
					},
					proc(data: Thread_Data) {
						op := data.op
						sw: time.Stopwatch
						time.stopwatch_start(&sw)
						for i in 0 ..< ITERS {
							op(data.data, data.thread_num, data.cycle_num, i)
						}
						time.stopwatch_stop(&sw)
						intrinsics.atomic_store_explicit(
							&data.counter.value,
							u64(time.stopwatch_duration(sw)),
							.Seq_Cst,
						)
					},
				)
			}

			for t in 0 ..< THREADS {
				thread.join(producers[t])
				thread.destroy(producers[t])
			}
		}
	}

	min := u64(time.MAX_DURATION)
	max := u64(0)
	avg: u64
	thread_min := u64(time.MAX_DURATION)
	thread_max := u64(0)
	total: u64

	for c in 0 ..< CYCLES {
		cycle_min := u64(time.MAX_DURATION)
		cycle_max := u64(0)
		cycle_avg: u64
		cycle_total: u64

		for t in 0 ..< THREADS {
			value := intrinsics.atomic_load(&cycles[c][t].value)
			if value < cycle_min {
				cycle_min = value
			}
			if value > cycle_max {
				cycle_max = value
			}
			cycle_total += value
		}

		if thread_min > cycle_min {
			thread_min = cycle_min
		}
		if thread_max < cycle_max {
			thread_max = cycle_max
		}
		total += cycle_total

		if cycle_total < min {
			min = cycle_total
		}
		if cycle_total > max {
			max = cycle_total
		}
	}

	if total < 1 {
		total = 1
	}

	avg_per_op := (f64(total) / f64(CYCLES) / f64(THREADS) / f64(ITERS))

	if (avg_per_op <= 0.0) {
		avg_per_op = 1
	}

	//	fmt.println("avg:", avg_per_op)

	per_op_total := f64(avg_per_op) * f64(THREADS)
	per_op_count_per_sec := f64(0)
	if avg_per_op > 0.0 {
		per_op_count_per_sec = f64(1_000_000_000) / avg_per_op
	}
	//    fmt.println("min:", min, "  max:", max, "  total:", total / u64(THREADS))
	fmt.println("avg: ", avg_per_op, "  ops per sec: ", u64(per_op_count_per_sec) * u64(THREADS))
}

global_trace_ctx: trace.Context

debug_trace_assertion_failure_proc :: proc(prefix, message: string, loc := #caller_location) -> ! {
	runtime.print_caller_location(loc)
	runtime.print_string(" ")
	runtime.print_string(prefix)
	if len(message) > 0 {
		runtime.print_string(": ")
		runtime.print_string(message)
	}
	runtime.print_byte('\n')

	ctx := &global_trace_ctx
	if !trace.in_resolve(ctx) {
		buf: [64]trace.Frame
		runtime.print_string("Debug Trace:\n")
		frames := trace.frames(ctx, 1, buf[:])
		for f, i in frames {
			fl := trace.resolve(ctx, f, context.temp_allocator)
			if fl.loc.file_path == "" && fl.loc.line == 0 {
				continue
			}
			runtime.print_caller_location(fl.loc)
			runtime.print_string(" - frame ")
			runtime.print_int(i)
			runtime.print_byte('\n')
		}
	}
	runtime.trap()
}

@(test)
test_trace :: proc(t: ^testing.T) {
	trace.init(&global_trace_ctx)
	defer trace.destroy(&global_trace_ctx)

	context.assertion_failure_proc = debug_trace_assertion_failure_proc

	do_something()
}

do_something :: proc() {
	assert(false, "")
}

@(test)
test_find_first :: proc(t: ^testing.T) {
	bitmap := make([]simd.u64x8, 16)
	bitmap[9] = simd.u64x8{0, 0, 1, 0, 0, 0, 0, 0}
	index := find_first_set_bit_x8(bitmap)
	fmt.println(index)
}
