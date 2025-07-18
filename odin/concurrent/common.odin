package concurrent

import "base:intrinsics"

atomic_load  :: intrinsics.atomic_load_explicit
atomic_swap  :: intrinsics.atomic_exchange_explicit
atomic_store :: intrinsics.atomic_store_explicit
atomic_add   :: intrinsics.atomic_add_explicit
atomic_sub   :: intrinsics.atomic_sub_explicit
atomic_or    :: intrinsics.atomic_or_explicit
atomic_and   :: intrinsics.atomic_and_explicit
cas_weak     :: intrinsics.atomic_compare_exchange_weak_explicit
cas          :: intrinsics.atomic_compare_exchange_strong_explicit
cpu_relax    :: intrinsics.cpu_relax
overflow_add :: intrinsics.overflow_add
overflow_sub :: intrinsics.overflow_sub

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
    CACHE_LINE_SIZE :: 128
} else {
    CACHE_LINE_SIZE :: 64
}

SPINS :: 100
CPU_RELAX_SPINS :: 5

Slot :: struct($T: typeid) #align (16) {
    stamp: u64,
    value: T,
}

Push_Error :: enum {
    Success = 0,
    Full    = -1,
    Closed  = -2,
}

Pop_Error :: enum i32 {
    Success = 0,
    Closed  = -1,
    Empty   = -2,
    Timeout = -3,
}
