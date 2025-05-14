package main

import "./order"
import "base:intrinsics"
import "core:fmt"
import "core:mem"
import "core:sync"
import "core:testing"
import "core:thread"
import "core:time"

main :: proc() {
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

//	cpu := new(VCpu_64, runtime.heap_allocator())
	cpu := make_vcpu(64, false)
	core := &cpu.cores[0]
	core.cpu_time_enabled = true
	core.non_zero = &cpu.non_zero_counter
	core.signal = &cpu.signals[0]
	core.worker = proc(data: rawptr) -> u8 {
		//		time.tick_now()
		//		time.tick_now()
		return SCHEDULE_FLAG
	}
	vcore_execute(core)
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

	benchmark(1, 5, 10000000, core, proc(data: ^Vcore, thread: int, cycles: int, iterations: int) {
//		time.tick_now()
		x := intrinsics.read_cycle_counter()
	})

	fmt.println("benching vcore_execute")
	benchmark(
		1,
		5,
		10000000,
		core,
		proc(data: ^Vcore, thread: int, cycles: int, iterations: int) {
			vcore_execute(data)
			//			intrinsics.cpu_relax()
		},
	)

	stop: bool
	th := thread.create_and_start_with_poly_data2(core, &stop, proc(core: ^Vcore, stop: ^bool) {
		for !intrinsics.atomic_load_explicit(stop, .Relaxed) {
			if signal_acquire(core.signal, core.index) {
				vcore_execute(core)
			}
		}
	})

	sw: time.Stopwatch
	time.stopwatch_start(&sw)
	benchmark(
		8,
		10,
		100000000,
		core,
		proc(data: ^Vcore, thread: int, cycles: int, iterations: int) {
			schedule(data)
			//            intrinsics.cpu_relax()
		},
	)
	time.stopwatch_stop(&sw)

	intrinsics.atomic_store(&stop, true)
	thread.join(th)
	thread.destroy(th)

	fmt.println(
		"core executes:",
		intrinsics.atomic_load(&core.counter),
		" in:",
		time.stopwatch_duration(sw),
	)

	benchmark(
		8,
		10,
		1000000,
		core,
		proc(data: ^Vcore, thread: int, cycles: int, iterations: int) {
//			time.tick_now()
			time.read_cycle_counter()
			//			time.now()
		},
	)
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
when ODIN_ARCH == .arm64 || ODIN_ARCH == .arm32 {
	CACHE_LINE_SIZE :: 128
} else {
	CACHE_LINE_SIZE :: 64
}

U64CacheLine :: struct #align (CACHE_LINE_SIZE) {
	value: u64,
}

VCpu_1 :: Vcpu(1, true)
VCpu_2 :: Vcpu(2, true)
VCpu_4 :: Vcpu(4, true)
VCpu_8 :: Vcpu(8, true)
VCpu_16 :: Vcpu(16, true)
VCpu_32 :: Vcpu(32, true)
VCpu_64 :: Vcpu(64, true)
VCpu_128 :: Vcpu(128, true)
VCpu_256 :: Vcpu(256, true)
VCpu_512 :: Vcpu(512, true)
VCpu_1024 :: Vcpu(1024, true)

VCpu_1_NonBlocking :: Vcpu(1, false)
VCpu_2_NonBlocking :: Vcpu(2, false)
VCpu_4_NonBlocking :: Vcpu(4, false)
VCpu_8_NonBlocking :: Vcpu(8, false)
VCpu_16_NonBlocking :: Vcpu(16, false)
VCpu_32_NonBlocking :: Vcpu(32, false)
VCpu_64_NonBlocking :: Vcpu(64, false)
VCpu_128_NonBlocking :: Vcpu(128, false)
VCpu_256_NonBlocking :: Vcpu(256, false)
VCpu_512_NonBlocking :: Vcpu(512, false)
VCpu_1024_NonBlocking :: Vcpu(1024, false)

Non_Zero_Counter :: struct #align (CACHE_LINE_SIZE) {
	counter: u64,
	_:       [CACHE_LINE_SIZE - size_of(u64)]u8,
	mu:      sync.Atomic_Mutex,
	_:       [CACHE_LINE_SIZE - size_of(sync.Atomic_Mutex)]u8,
	cond:    sync.Atomic_Cond,
}

// Vcpu is the root structure of the scheduler.
// It contains a list of Signals and VCores. Signals are an atomic 64bit bitmap
// that maps each bit to a corresponding Vcore.
//odinfmt: disable
Vcpu :: struct($PARALLELISM: uintptr, $BLOCKING: bool) {
//	where PARALLELISM > 0 &&
//		(PARALLELISM & (PARALLELISM - 1)) == 0 &&
//		CORES == PARALLELISM * 64
//{
	allocator: 			mem.Allocator,
	non_zero_counter: 	Non_Zero_Counter,
	signals: 			[PARALLELISM]Signal,
	available: 			[PARALLELISM]Signal,
	cores: 				[PARALLELISM*64]Vcore,
}
//odinfmt: enable

make_vcpu :: proc ($PARALLELISM: uintptr, $BLOCKING: bool, allocator := context.allocator) -> ^Vcpu(PARALLELISM, BLOCKING) {
	#assert(PARALLELISM > 0 && PARALLELISM < 1024 && (PARALLELISM & (PARALLELISM - 1)) == 0,
		"PARALLELISM must be a power of 2 between 1 and 1024")
	cpu := new(Vcpu(PARALLELISM, BLOCKING), context.allocator)
	cpu.allocator = allocator
	return cpu
}

destroy :: proc (cpu: ^Vcpu($P, $B)) {

}

vcpu_capacity :: proc "contextless" (cpu: ^Vcpu($P, $B)) -> u64 {
	return SIZE
}

non_zero_counter_await :: proc(non_zero: ^Non_Zero_Counter) {
	if intrinsics.atomic_load_explicit(&non_zero.counter, .Relaxed) == 0 {
		sync.atomic_cond_wait(&non_zero.cond, &non_zero.mu)
	}
}

non_zero_counter_incr :: proc "contextless" (non_zero: ^Non_Zero_Counter) {
	if intrinsics.atomic_add_explicit(&non_zero.counter, 1, .Seq_Cst) == 0 {
		// signal all waiting threads
		sync.atomic_cond_broadcast(&non_zero.cond)
	}
}

non_zero_counter_decr :: proc "contextless" (nz: ^Non_Zero_Counter) {
	intrinsics.atomic_sub_explicit(&nz.counter, 1, .Seq_Cst)
}

execute :: proc(cpu: ^Vcpu($P, $B)) {

}

execute_wait :: proc(cpu: ^Vcpu($P, $B)) {
	CORES :: P * 64
	CORES_MASK :: CORES - 1
	SIGNAL_MASK :: P - 1
}

Signal :: struct #align (CACHE_LINE_SIZE) {
	value: u64,
}

// return the number of set signal bits
signal_size :: proc "contextless" (signal: ^Signal) -> u64 {
	return intrinsics.count_ones(intrinsics.atomic_load_explicit(&signal.value, .Relaxed))
}

signal_is_empty :: proc "contextless" (signal: ^Signal) -> bool {
	return intrinsics.count_ones(intrinsics.atomic_load_explicit(&signal.value, .Relaxed)) == 0
}

// find nearest signal bit index to a supplied bit index starting point.
signal_nearest :: proc "contextless" (value: u64, signal_index: u64) -> u64 {
	found := intrinsics.count_trailing_zeros(value >> signal_index) + signal_index
	if found < 64 do return found
	return signal_index - intrinsics.count_trailing_zeros(value << (63 - signal_index))
}

// atomically set the signal bit at specified index
signal_set :: proc "contextless" (
	signal: ^Signal,
	index: u64,
) -> (
	was_empty: bool,
	was_set: bool,
) {
	bit := u64(1) << index
	prev := intrinsics.atomic_or_explicit(&signal.value, bit, .Seq_Cst)
	return prev == 0, (prev & bit) == 0
}

// atomically acquire the signal bit at specified index returning true if successful
signal_acquire :: proc "contextless" (signal: ^Signal, index: u64) -> bool {
	bit := u64(1) << index
	return (intrinsics.atomic_and_explicit(&signal.value, ~bit, .Seq_Cst) & bit) == bit
}

// return true if the signal bit at index is set
signal_is_set :: proc "contextless" (signal: ^Signal, index: u64) -> bool {
	bit := u64(1) << index
	return (intrinsics.atomic_load_explicit(&signal.value, .Relaxed) & bit) != 0
}

_FLAGS_PADDING :: CACHE_LINE_SIZE - size_of(u8) - size_of(u64) - size_of(^int) - size_of(^int)

Vcore :: struct {
	non_zero:          ^Non_Zero_Counter,
	signal:            ^Signal,
	index:             u64,
	flags:             u8,
	_:                 [_FLAGS_PADDING]u8,
	cpu_time:          u64,
	cpu_time_overhead: u64,
	counter:           u64,
	contention:        u64,
	cpu_time_enabled:  bool,
	data:              rawptr,
	worker:            proc(data: rawptr) -> u8,
}

SCHEDULE_FLAG :: 1
EXECUTE_FLAG :: 2

schedule :: #force_inline proc "contextless" (core: ^Vcore) {
	if (intrinsics.atomic_load_explicit(&core.flags, .Relaxed) & SCHEDULE_FLAG) != 0 {
		return
	}
	previous_flags := intrinsics.atomic_or_explicit(&core.flags, SCHEDULE_FLAG, .Seq_Cst)
	not_scheduled_nor_executing := (previous_flags & (SCHEDULE_FLAG | EXECUTE_FLAG)) == 0
	if (not_scheduled_nor_executing) {
		if was_empty, was_set := signal_set(core.signal, core.index); was_empty && was_set {
			non_zero_counter_incr(core.non_zero)
		}
	}
}

vcore_execute :: proc(core: ^Vcore) {
	result: u8

	if core.cpu_time_enabled {
//		start := time.tick_now()
		start := intrinsics.read_cycle_counter()
		intrinsics.atomic_store_explicit(&core.flags, EXECUTE_FLAG, .Seq_Cst)
		intrinsics.atomic_add_explicit(&core.counter, 1, .Relaxed)
		result: u8
		if core.worker != nil {
			result = core.worker(core.data)
		}
//		elapsed := time.tick_diff(start, time.tick_now())
		elapsed := intrinsics.read_cycle_counter() - start
		intrinsics.atomic_add_explicit(&core.cpu_time, u64(elapsed), .Relaxed)
		//		intrinsics.atomic_add_explicit(&core.cpu_time_overhead, 40, .Relaxed)
	} else {
		intrinsics.atomic_store_explicit(&core.flags, EXECUTE_FLAG, .Seq_Cst)
		intrinsics.atomic_add_explicit(&core.counter, 1, .Relaxed)
		if core.worker != nil {
			result = core.worker(core.data)
		}
	}

	if result == SCHEDULE_FLAG {
		intrinsics.atomic_store_explicit(&core.flags, SCHEDULE_FLAG, .Seq_Cst)
		if was_empty, was_set := signal_set(core.signal, core.index); was_empty && was_set {
			non_zero_counter_incr(core.non_zero)
		}
	} else {
		after_flags := intrinsics.atomic_sub_explicit(&core.flags, EXECUTE_FLAG, .Seq_Cst)
		if (after_flags & SCHEDULE_FLAG) != 0 {
			signal_set(core.signal, core.index)
		}
	}
}

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
		counter:    ^U64CacheLine,
	}

	cycles: [CYCLES][THREADS]U64CacheLine

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

	//    fmt.println("min:", min, "  max:", max, "  total:", total / u64(THREADS))
	fmt.println(
		"avg: ",
		avg_per_op,
		"  ops per sec: ",
		1_000_000_000 / u64(avg_per_op) * u64(THREADS),
	)
}

@(test)
test_signal :: proc(t: ^testing.T) {
	fmt.println("")
	fmt.println("size_of(VCpu_1)", size_of(VCpu_1))
	fmt.println("size_of(VCpu_2)", size_of(VCpu_2))
	fmt.println("size_of(VCpu_4)", size_of(VCpu_4))
	fmt.println("size_of(VCpu_8)", size_of(VCpu_8))
	fmt.println("size_of(VCpu_16)", size_of(VCpu_16))
	fmt.println("size_of(VCpu_32)", size_of(VCpu_32))
	fmt.println("size_of(VCpu_64)", size_of(VCpu_64))
	fmt.println("size_of(VCpu_128)", size_of(VCpu_128))
	fmt.println("size_of(VCpu_256)", size_of(VCpu_256))
	fmt.println("size_of(VCpu_512)", size_of(VCpu_512))
	fmt.println("size_of(VCpu_1024)", size_of(VCpu_1024))
	signal := Signal {
		value = 0,
	}
	//	benchmark(
	//		1,
	//		10,
	//		10000,
	//		&signal,
	//		proc(data: ^Signal, thread: int, cycles: int, iterations: int) {
	//			//			time.tick_now()
	//			//			time.now()
	//		},
	//	)
	fmt.println("signal_set(0)", signal_set(&signal, 0))
	fmt.println("signal_set(0)", signal_set(&signal, 0))
	fmt.println(signal.value)
	fmt.println("signal_size", signal_size(&signal))
	fmt.println("signal_set(0)", signal_set(&signal, 63))
	fmt.println("signal_set(0)", signal_set(&signal, 63))
	fmt.println(signal.value)
	fmt.println("signal_size", signal_size(&signal))
	fmt.println("signal_acquire(63)", signal_acquire(&signal, 63))
	fmt.println("signal_acquire(63)", signal_acquire(&signal, 63))
	fmt.println(signal.value)
	fmt.println("signal_size", signal_size(&signal))
}



print_out :: proc() {
	m: order.Order_Message
}