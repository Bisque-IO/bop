package snmalloc

import bop "../libbop"

allocator       :: bop.snmallocator
allocator_proc  :: bop.snmallocator_proc
alloc           :: bop.alloc
alloc_aligned   :: bop.alloc_aligned
zalloc          :: bop.zalloc
zalloc_aligned  :: bop.zalloc_aligned
realloc         :: bop.realloc
dealloc         :: bop.dealloc
dealloc_sized   :: bop.dealloc_sized
