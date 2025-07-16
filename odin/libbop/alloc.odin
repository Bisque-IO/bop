package libbop

import mem "core:mem"

import c "core:c/libc"

snmallocator :: #force_inline proc "contextless" () -> mem.Allocator {
	return mem.Allocator{snmallocator_proc, nil}
}

snmallocator_proc :: proc(
	allocator_data: rawptr,
	mode: mem.Allocator_Mode,
	size, alignment: int,
	old_memory: rawptr,
	old_size: int,
	location := #caller_location,
) -> (
	data: []byte,
	err: mem.Allocator_Error,
) {
	switch mode {
	case .Alloc:
		ptr := zalloc_aligned(c.size_t(alignment), c.size_t(size))
		if ptr == nil {
			err = .Out_Of_Memory
			return
		}
		data = mem.byte_slice(ptr, size)
		return

	case .Alloc_Non_Zeroed:
		ptr := alloc_aligned(c.size_t(alignment), c.size_t(size))
		if ptr == nil {
			err = .Out_Of_Memory
			return
		}
		data = mem.byte_slice(ptr, size)
		return

	case .Free:
		if old_size > 0 {
			dealloc_sized(old_memory, c.size_t(old_size))
		} else {
			dealloc(old_memory)
		}
		return nil, nil

	case .Resize:
		ptr := realloc(old_memory, c.size_t(size))
		if ptr == nil {
			err = .Out_Of_Memory
			return
		}
		data = mem.byte_slice(ptr, size)
		if old_size < size {
			// zero extension range
			mem.zero(rawptr(&data[old_size]), size - old_size)
		}
		return

	case .Resize_Non_Zeroed:
		ptr := realloc(old_memory, c.size_t(size))
		if ptr == nil {
			err = .Out_Of_Memory
			return
		}
		data = mem.byte_slice(ptr, size)
		return

	case .Free_All, .Query_Features, .Query_Info:
		return nil, .Mode_Not_Implemented
	}

	return nil, .Mode_Not_Implemented
}
