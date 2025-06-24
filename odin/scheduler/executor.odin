package scheduler

import "base:intrinsics"
import "base:runtime"
import "core:mem"

//odinfmt:disable
EXECUTE_ERR_EMPTY_SIGNAL :: -1
EXECUTE_ERR_CORE_IS_NIL  :: -2
EXECUTE_ERR_CONTENTED    :: -3
EXECUTE_ERR_NO_WORK      :: -4

// Vcpu is the root structure of the scheduler.
// It contains a list of Signals and VCores. Signals are an atomic 64bit bitmap
// that maps each bit to a corresponding Vcore.
Executor :: struct($PARALLELISM: uintptr, $BLOCKING: bool) {
//	where PARALLELISM > 0 &&
//		(PARALLELISM & (PARALLELISM - 1)) == 0 &&
//		CORES == PARALLELISM * 64
//{
    allocator: runtime.Allocator,
    groups:    [PARALLELISM]Signal_Group
}
//odinfmt: enable



//selector_next_map :: proc "contextless" (s: ^Selector) -> u64 {
//    s.mapped += 1
//    s.select_count = 1
//    s.select = 0
//    return s.mapped
//}
//
//selector_next_select :: proc "contextless" (s: ^Selector) -> u64 {
//    if s.select_count >= SIGNAL_CAPACITY {
//        selector_next_map(s)
//        return s.select
//    }
//    s.select_count = 1
//    s.select += 1
//    return s.select & GROUP_MASK
//}

execute :: proc() -> u64 {
    return 0
}

executor_make :: proc(
    $PARALLELISM: uintptr,
    $BLOCKING: bool,
    allocator := context.allocator,
) -> (executor: ^Executor(PARALLELISM, BLOCKING), err: runtime.Allocator_Error) {
    #assert(
        PARALLELISM > 0 && PARALLELISM < 1024 && (PARALLELISM & (PARALLELISM - 1)) == 0,
        "PARALLELISM must be a power of 2 between 1 and 1024",
    )
    executor = new(Executor(PARALLELISM, BLOCKING), context.allocator) or_return
    executor.allocator = allocator
    return
}

executor_destroy :: proc(cpu: ^Executor($P, $B)) {

}

executor_capacity :: proc "contextless" (cpu: ^Executor($P, $B)) -> u64 {
    return SIZE
}


executor_execute :: proc(self: ^Executor($P, $B), selector: ^Selector) -> int {
    CORES       :: P * 64
    CORES_MASK  :: CORES - 1
    GROUP_MASK  :: P - 1

    signal_index := selector_next_select(selector)
    index 		 := selector.signal & GROUP_MASK
    group        := &self.groups[index]
    signal_value := signal.value

    if signal_value == 0 {
        selector_next_map(selector)
        return EXECUTE_ERR_EMPTY_SIGNAL
    }

    selected := signal_nearest_branchless(signal_value, signal_index)
    bit := 1 << selected
    expected := intrinsics.atomic_and(&signal.value, ~bit)
    acquired := (expected & bit) == bit

    core := &self.slots[index * 64 + selected]

    if !acquired {
        intrinsics.atomic_add(core.contention, 1)
        return EXECUTE_ERR_CONTENTED
    }

    empty := expected == bit

    if slot_execute(core) == WORKER_SCHEDULE {
        core.flags = WORKER_SCHEDULE
        prev := intrinsics.atomic_or(&signal.value, bit)
        if prev == 0 && !empty {
            waker_incr(&self.non_zero_counter)
        }
    } else {
        after_flags := intrinsics.atomic_add(core.flags, -WORKER_EXECUTE)
        if after_flags & WORKER_SCHEDULE != 0 {
            prev := intrinsics.atomic_or(&signal.value, bit)
            if prev == 0 && !empty {
                waker_incr(&self.non_zero_counter)
            }
        } else if intrinsics.atomic_load(&signal.value) == 0 && empty {
            waker_decr(&self.non_zero_counter)
        }
    }

    return 0
}
