package executor

import "base:intrinsics"
import "core:crypto"
import "core:sync"

Selector :: struct {
	owned:      int,
	value:      u64,
	block:      u64,
	signal:     u64,
	bit:        u64,
	bit_count:  u64,
	contention: u64,
	seed:       u64,
}

selector_init :: proc(s: ^Selector) {
	b: [8]byte
	crypto.rand_bytes(b[0:8])
	s.seed = (cast(^u64)&b[0])^
}

//odinfmt:disable
RND_MULTIPLIER   :: 0x5DEECE66D
RND_ADDEND 		 :: 0xB
RND_MASK 		 :: (1 << 48) - 1
SIGNAL_CAPACITY  :: 64
SIGNAL_MASK 	 :: SIGNAL_CAPACITY - 1
//odinfmt:enable

selector_next :: #force_inline proc "contextless" (s: ^Selector) -> u64 {
	oldseed := s.seed
	nextseed := (oldseed * RND_MULTIPLIER + RND_ADDEND) & RND_MASK
	s.seed = nextseed
	return nextseed >> 16
}

selector_next_map :: proc "contextless" (s: ^Selector) -> u64 {
	s.signal = selector_next(s)
	s.bit = selector_next(s) & SIGNAL_MASK
	s.bit_count = 1
	return s.signal
}

selector_next_select :: proc "contextless" (s: ^Selector) -> u64 {
	if s.bit_count >= SIGNAL_CAPACITY {
		selector_next_map(s)
		return s.bit
	}
	s.bit += 1
	s.bit_count += 1
	return s.bit & SIGNAL_MASK
}
