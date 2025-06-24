package scheduler

import "base:intrinsics"
import "core:sync"
import "core:thread"

Waker :: struct #align (CACHE_LINE_SIZE) {
    counter: u64,
    _:       [CACHE_LINE_SIZE - size_of(u64)]u8,
    mu:      sync.Atomic_Mutex,
    _:       [CACHE_LINE_SIZE - size_of(sync.Atomic_Mutex)]u8,
    cond:    sync.Atomic_Cond,
}

waker_await :: #force_inline proc "contextless" (self: ^Waker) {
    if intrinsics.atomic_load_explicit(&self.counter, .Acquire) == 0 {
        sync.atomic_cond_wait(&self.cond, &self.mu)
    }
}

waker_incr :: #force_inline proc "contextless" (non_zero: ^Waker) {
    if intrinsics.atomic_add_explicit(&non_zero.counter, 1, .Release) == 0 {
        // signal all waiting threads
        sync.atomic_cond_broadcast(&non_zero.cond)
    }
}

waker_decr :: #force_inline proc "contextless" (nz: ^Waker) {
    intrinsics.atomic_sub_explicit(&nz.counter, 1, .Seq_Cst)
}