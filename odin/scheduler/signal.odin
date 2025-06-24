package scheduler

import "base:intrinsics"
import "base:runtime"
import "core:mem"
import "core:simd"
import "core:sync"

// Signal is an bit-set representing 64 slots. 64-bit atomics are used to transition states.
// Fast searches are achieved via the CPU instruction CTZ (count trailing zeros).
Signal :: ^u64

// return the number of set signal bits
signal_size :: proc "contextless" (signal: Signal) -> u64 {
    return intrinsics.count_ones(intrinsics.atomic_load_explicit(signal, .Relaxed))
}

signal_is_empty :: proc "contextless" (signal: Signal) -> bool {
    return intrinsics.count_ones(intrinsics.atomic_load_explicit(signal, .Relaxed)) == 0
}

// find nearest signal bit index to a supplied bit index starting point.
signal_nearest :: #force_inline proc "contextless" (value: u64, signal_index: u64) -> u64 {
    up := intrinsics.count_trailing_zeros(value >> signal_index)
    down := intrinsics.count_trailing_zeros(intrinsics.reverse_bits(value << (63 - signal_index)))

    if up < down {
        return signal_index+up
    } else if down < up {
        return signal_index-down
    } else {
        return signal_index
    }
}

// find nearest signal bit index to a supplied bit index starting point.
// This is a branchless version
signal_nearest_branchless :: #force_inline proc "contextless" (value: u64, signal_index: u64) -> u64 {
    up := intrinsics.count_trailing_zeros(value >> signal_index)
    down := intrinsics.count_trailing_zeros(intrinsics.reverse_bits(value << (63 - signal_index)))

    up_lt_down := u64(up < down)                // 1 if up < down
    down_lt_up := u64(down < up)                // 1 if down < up
    equal      := 1 - (up_lt_down | down_lt_up) // 1 if equal

    // Create masks: either all 1s (if condition true) or 0s
    up_mask   := u64(0) - up_lt_down
    down_mask := u64(0) - down_lt_up
    same_mask := u64(0) - equal

    // Compute each result
    up_result   := signal_index + up
    down_result := signal_index - down
    same_result := signal_index

    // Mask-select the result
    return (up_result & up_mask) | (down_result & down_mask) | (same_result & same_mask)
}

// Atomically set the signal bit at specified index
signal_set :: #force_inline proc "contextless" (signal: Signal, index: u64) -> (was_empty: bool, was_set: bool) {
    bit := u64(1) << index
    prev := intrinsics.atomic_or_explicit(signal, bit, .Acq_Rel)
    return prev == 0, (prev & bit) == 0
}

// Atomically acquire the signal bit at specified index returning true if successful
signal_acquire :: #force_inline proc "contextless" (signal: Signal, index: u64) -> bool {
    bit := u64(1) << index
    return (intrinsics.atomic_and_explicit(signal, ~bit, .Acq_Rel) & bit) == bit
}

// return true if the signal bit at index is set
signal_is_set :: #force_inline proc "contextless" (signal: Signal, index: u64) -> bool {
    bit := u64(1) << index
    return (intrinsics.volatile_load(signal) & bit) != 0
}

//odinfmt:disable
MAX_U64    :: ~u64(0)
ZERO_U64X8 :: simd.u64x8{0, 0, 0, 0, 0, 0, 0, 0}
MAX_U64X8  :: simd.u64x8{~u64(0), ~u64(0), ~u64(0), ~u64(0), ~u64(0), ~u64(0), ~u64(0), ~u64(0)}
ZERO_U64X4 :: simd.u64x4{0, 0, 0, 0}
//odinfmt:enable

Signal_Handle :: struct {
    signal: Signal,
    ref:    ^u64
}

Signal_Groups :: struct {
    groups: []^Signal_Group,
    waker:  Waker
}

// Signal Group represents a SIMD optimized grouping of Signals and their corresponding Slots.
// It is thread-safe
Signal_Group :: struct {
    signals:  simd.u64x8,
    freelist: simd.u64x8,
    waker:    ^Waker,
    counter:  u64,
    slots:    [512]Slot,
}

signal_group_make :: proc(allocator: runtime.Allocator = context.allocator) -> ^Signal_Group {
    sg := new(Signal_Group, allocator)
    signal_group_init(sg)
    return sg
}

signal_group_init :: proc(sg: ^Signal_Group) {
    sg.signals = ZERO_U64X8
    sg.freelist = MAX_U64X8

    for &core in sg.slots {

    }
}

signal_group_destroy :: proc(self: ^Signal_Group) {

}

signal_group_fast_check :: #force_inline proc "contextless" (sg: ^Signal_Group) -> bool {
    return sg.signals != ZERO_U64X8
}

// Attempts to reserve a core and returns
signal_group_reserve :: #force_inline proc "contextless" (sg: ^Signal_Group, worker: proc()) -> ^Slot {
    assert_contextless(sg != nil, "^Signal_Group is nil")

    // quick check to see if the group is full
    if sg.freelist == ZERO_U64X8 {
        return nil
    }

    // counter is used to set a starting index
    // we want to space the cores out as much as possible based on 64 bit alignment
    counter := intrinsics.atomic_add_explicit(&sg.counter, 1, .Relaxed) - 1


    return nil
}

// Attempts to reserve at a specific index.
signal_group_reserve_at :: #force_inline proc "contextless" (
    sg: ^Signal_Group,
    index: int, worker: proc()) -> int {
// quick check to see if the group is full
    if sg.freelist == ZERO_U64X8 {
        return -1
    }

    // counter is used to set a starting index
    // we want to space the cores out as much as possible based on 64 bit alignment
    counter := intrinsics.atomic_add(&sg.counter, 1) - 1

    return 0
}

//
signal_group_select :: proc "contextless" (sg: ^Signal_Group, s: ^Selector) {
    selector_next(s)

}


import "core:fmt"
import "core:testing"

@(test)
test_signal :: proc(t: ^testing.T) {
    value := u64(0)
    signal := Signal(&value)
    signal_39 := signal^

//    benchmark(1, 10, 10000000, value, proc(data: u64, thread_num: int, cycle_num: int, iterations: int) {
//        signal_nearest_branchless(data, 41)
//    })


    fmt.println("")
//    fmt.println("size_of(Executor_1)", size_of(Executor_1))
//    fmt.println("size_of(Executor_2)", size_of(Executor_2))
//    fmt.println("size_of(Executor_4)", size_of(Executor_4))
//    fmt.println("size_of(Executor_8)", size_of(Executor_8))
//    fmt.println("size_of(Executor_16)", size_of(Executor_16))
//    fmt.println("size_of(Executor_32)", size_of(Executor_32))
//    fmt.println("size_of(Executor_64)", size_of(Executor_64))
//    fmt.println("size_of(Executor_128)", size_of(Executor_128))
//    fmt.println("size_of(Executor_256)", size_of(Executor_256))
//    fmt.println("size_of(Executor_512)", size_of(Executor_512))
//    fmt.println("size_of(Executor_1024)", size_of(Executor_1024))

    value = 0

    fmt.println("nearest 41", signal_nearest_branchless(signal^, 41))
    signal_set(signal, 39)
    fmt.println("nearest 41", signal_nearest_branchless(signal^, 41))
    fmt.println("nearest 37", signal_nearest_branchless(signal^, 37))
    fmt.println("nearest 1", signal_nearest_branchless(signal^, 1))
    fmt.println("nearest 0", signal_nearest_branchless(signal^, 0))
    fmt.println("nearest 39", signal_nearest_branchless(signal^, 39))
    fmt.println("nearest 63", signal_nearest_branchless(signal^, 63))

    signal_set(signal, 15)
    fmt.println("nearest 41", signal_nearest_branchless(signal^, 41))
    fmt.println("nearest 37", signal_nearest_branchless(signal^, 37))
    fmt.println("nearest 25", signal_nearest_branchless(signal^, 25))
    fmt.println("nearest 11", signal_nearest_branchless(signal^, 11))
    fmt.println("nearest 20", signal_nearest_branchless(signal^, 20))
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
    fmt.println("signal_set(0)", signal_set(signal, 0))
    fmt.println("signal_set(0)", signal_set(signal, 0))
    fmt.println(signal^)
    fmt.println("signal_size", signal_size(signal))
    fmt.println("signal_set(0)", signal_set(signal, 63))
    fmt.println("signal_set(0)", signal_set(signal, 63))
    fmt.println(signal^)
    fmt.println("signal_size", signal_size(signal))
    fmt.println("signal_acquire(63)", signal_acquire(signal, 63))
    fmt.println("signal_acquire(63)", signal_acquire(signal, 63))
    fmt.println(signal^)
    fmt.println("signal_size", signal_size(signal))


}