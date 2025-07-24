package concurrent

import "base:intrinsics"

//odinfmt:disable
atomic_load     :: intrinsics.atomic_load_explicit
atomic_swap     :: intrinsics.atomic_exchange_explicit
atomic_store    :: intrinsics.atomic_store_explicit
atomic_add      :: intrinsics.atomic_add_explicit
atomic_sub      :: intrinsics.atomic_sub_explicit
atomic_or       :: intrinsics.atomic_or_explicit
atomic_and      :: intrinsics.atomic_and_explicit
cas_weak        :: intrinsics.atomic_compare_exchange_weak_explicit
cas             :: intrinsics.atomic_compare_exchange_strong_explicit
cpu_relax       :: intrinsics.cpu_relax
overflow_add    :: intrinsics.overflow_add
overflow_sub    :: intrinsics.overflow_sub

SPINS           :: 100
CPU_RELAX_SPINS :: 5
//odinfmt:enable

when ODIN_ARCH == .amd64 || ODIN_ARCH == .arm64 || ODIN_ARCH == .arm32 {
	CACHE_LINE_SIZE :: 128
} else {
	CACHE_LINE_SIZE :: 64
}

Slot :: struct($T: typeid) #align (16) {
	stamp: u64,
	value: T,
}

Push_Error :: enum {
	Success = 0,
	Full    = -1,
	Closed  = -2,
}

Pop_Error :: enum {
	Success = 0,
	Closed  = -1,
	Empty   = -2,
	Timeout = -3,
}
