package executor

import "base:intrinsics"

//odinfmt:disable
atomic_load  :: intrinsics.atomic_load_explicit
atomic_store :: intrinsics.atomic_store_explicit
atomic_sub   :: intrinsics.atomic_sub_explicit
atomic_add   :: intrinsics.atomic_add_explicit
atomic_or    :: intrinsics.atomic_or_explicit
atomic_and   :: intrinsics.atomic_and_explicit
ctz          :: intrinsics.count_trailing_zeros
reverse_bits :: intrinsics.reverse_bits
//odinfmt:enable
