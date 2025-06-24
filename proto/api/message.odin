package bop

import "base:intrinsics"
import runtime "base:runtime"
import "core:fmt"
import log "core:log"
import "core:mem"
import "core:testing"
import json "core:encoding/json"

// Since block sizes are always at least a multiple of 4, the two least
// significant bits of the size field are used to store the block status:
// - bit 0: whether block is used or free
// - bit 1: whether previous block is used or free
FREE_BIT        : u32le : 1 << 0
PREV_FREE_BIT   : u32le : 1 << 1
CLEAR           :: ~(FREE_BIT | PREV_FREE_BIT)
OVERHEAD        :: size_of(i32)
DATA_OFFSET     :: size_of(i32) * 2
MIN_SIZE        :: size_of(i32) * 3

Error :: enum {
	None,
	No_Version,
}

Header :: struct {
	size:     u32le,
	reserved: u32le,
	base:     Base_Header,
}

Base_Header :: struct {
	base_size: u16le,
	extends:   u16le,
	version:   u16le,
	flex:      bool,
	flags:     byte,
}

Buffer :: struct {
	data:      ^Header,
	capacity:  u32le,
	trash:     u32le,
	allocator: mem.Allocator,
}

buffer_default :: proc(allocator := context.allocator) -> Buffer {
	return Buffer{allocator = allocator}
}

buffer_new :: proc(allocator := context.allocator) -> (^Buffer, runtime.Allocator_Error) {
	return runtime.new(Buffer, allocator)
}

buffer_clear :: proc() {

}

buffer_reset :: proc "c" (buf: ^Buffer) {

}

buffer_free :: proc(buf: ^Buffer) {
	if buf == nil do return
	if buf.data != nil {
		runtime.free(rawptr(buf.data), buf.allocator)
		buf.data = nil
		buf.capacity = 0
		buf.trash = 0
	}
}

Ref :: struct($T: typeid) {
	_buf:       ^Buffer,
	_base_size: int,
	using val:  ^T,
}

Ref_Unsafe :: struct($T: typeid) {
    _buf:       ^Buffer,
    _base_size: int,
    _val:  		^T,
}

Parse_Result :: union($T: typeid) {
    Ref(T),
    Ref_Unsafe(T),
}

Value :: struct($T: typeid) {
	_buf:       ^Buffer,
	_base_size: int,
	using val:  T,
}

Root_Data :: struct($T: typeid) {
	using value: Base_Header
}

allocate :: proc(
	$T: typeid,
	root: Root_Data(T),
	buf: ^Buffer,
	initial_capacity := size_of(Header) + size_of(T) + size_of(T),
) -> (
	msg: Ref(T),
	err: runtime.Allocator_Error,
) {
	ensure(root.base_size == size_of(T), "invalid root: base_size != size_of(T)")
	ensure(buf != nil, "buf cannot be nil")
	ensure(buf.allocator.procedure != nil, "Buffer.allocator not set")

	MIN_CAPACITY :: size_of(Header) + size_of(T)

	initial_capacity := initial_capacity
	if initial_capacity < MIN_CAPACITY {
		initial_capacity = MIN_CAPACITY
	}
	buf.trash = 0
	if int(buf.capacity) < initial_capacity {
		b: []byte
		if buf.data == nil {
			b, err = runtime.mem_alloc(initial_capacity, runtime.DEFAULT_ALIGNMENT, buf.allocator)
			if err != .None {
				return
			}
			buf.data = cast(^Header)rawptr(raw_data(b))
		} else {
            b = runtime.mem_resize(
                rawptr(buf.data),
                int(buf.capacity),
                int(initial_capacity),
                runtime.DEFAULT_ALIGNMENT,
                buf.allocator,
            ) or_return

			buf.data = cast(^Header)rawptr(raw_data(b))
			runtime.mem_zero(rawptr(buf.data), int(initial_capacity))
		}
		buf.capacity = u32le(len(b))
	} else {
		runtime.mem_zero(rawptr(buf.data), int(buf.capacity))
	}

	buf.data.size = size_of(Header) + size_of(T)
	buf.data.base = root.value
	buf.data.base.base_size = size_of(T)
	return {_buf = buf, val = cast(^T)rawptr(&(cast([^]byte)buf.data)[size_of(Header)])}, .None
}

Block :: struct {
	prev_phys: u32le,
	size:      u32le,
	next_free: u32le,
	prev_free: u32le,
}

_block_size :: #force_inline proc "contextless" (self: ^Block) -> u32le {
	return self.size & ~(FREE_BIT | PREV_FREE_BIT)
}

_block_set_size :: #force_inline proc "contextless" (self: ^Block, size: u32le) {
	self.size = size | (self.size & (FREE_BIT | PREV_FREE_BIT))
}

_block_set_used :: #force_inline proc "contextless" (self: ^Block) {
	self.size &= ~FREE_BIT
}

_block_set_free :: #force_inline proc "contextless" (self: ^Block) {
	self.size |= FREE_BIT
}

_block_set_prev_free :: #force_inline proc "contextless" (self: ^Block) {
	self.size |= PREV_FREE_BIT
}

_block_set_prev_used :: #force_inline proc "contextless" (self: ^Block) {
	self.size &= ~PREV_FREE_BIT
}

_block_of :: #force_inline proc "contextless" (self: Ref(Offset)) -> ^Block {
	if intrinsics.expect(
		self.size < MIN_SIZE || self._buf.data.size < self.size + self.offset,
		false,
	) {
		return nil
	}
	return cast(^Block)rawptr(&(cast([^]byte)self._buf.data)[self.offset])
}

_block_ptr :: #force_inline proc "contextless" (self: ^Block) -> rawptr {
	return rawptr(&(cast([^]byte)&self.size)[DATA_OFFSET])
}

_block_alloc :: proc(msg: Ref($T), size: int) -> (^Block, runtime.Allocator_Error) {
	buf := msg._buf
	block_size := OVERHEAD + size
	if buf.data.size + block_size > buf.capacity {
		b, err := runtime.mem_resize(
			rawptr(buf.data),
			int(buf.capacity),
			int(initial_capacity),
			runtime.DEFAULT_ALIGNMENT,
			buf.allocator,
		)
		if err != .None {
			return nil, err
		}
		buf.data = cast(^Header)rawptr(raw_data(b))
		buf.capacity = u32le(len(b))
	}
	block := cast(^Block)rawptr(&(cast([^]byte)buf.data)[buf.size])
	buf.data.size += block_size
	return block, .None
}

Offset :: struct {
	offset: u32le,
	size:   u32le,
}

String_Offset :: distinct Offset

Slice_Offset :: distinct Offset

Pointer_Offset :: struct($T: typeid) {
	offset: u32le,
}

Pointer_Header :: struct {
	base_size: u32le,
}

String_Header :: struct {
	size: u32le,
	pad:  u32le,
}

Slice_Header :: struct {
	length:    u32le,
	base_size: u32le,
}

is_base :: #force_inline proc "contextless" (m: Ref($T)) -> bool {
	return int(rawptr(m.val)) == int(rawptr(m._buf.data)) + size_of(Header)
}

deref_string :: #force_inline proc "contextless" (self: Ref(String_Offset)) -> string {
	// deref block
	b := _block_of(transmute(Ref(Offset))self)
	// make sure valid
	if intrinsics.expect(b == nil, false) {
		return ""
	}

	// deref string header
	h := cast(^String_Header)_block_ptr(b)
	// make sure valid
	if intrinsics.expect(self._buf.data.size < self.offset + DATA_OFFSET + h.size, false) {
		return ""
	}

	// create odin string
	return transmute(string)mem.Raw_String{&(cast([^]byte)h)[size_of(String_Header)], int(h.size)}
}

_str_of :: #force_inline proc "contextless" (self: ^Buffer, val: ^String_Offset) -> string {
	return deref_string(Ref(String_Offset){_buf = self, val = val})
}

Struct_Versions :: struct {
	id:      u32le,
	name:    string,
	headers: []Base_Header,
}

@(test)
test_allocate :: proc(t: ^testing.T) {
	Price :: struct {
		open, high, low, close: i32,
	}

	//	Bar :: union {
	//		Bar_V1,
	//		Bar_V2,
	//	}
	//
	//	Bar_V1 :: struct {
	//		id:    i64,
	//		name:  String_Offset,
	//		price: Price,
	//	}
	//
	//	Bar_V2 :: struct {
	//		id:    i64,
	//		name:  String_Offset,
	//		price: Price,
	//		enabled: bool,
	//	}

	Bar :: struct {
		id:      i64le,
		name:    String_Offset,
		price:   Price,
		enabled: bool,
		prices:  Slice_Offset,
	}

	Bar_Header :: Root_Data(Bar){Base_Header{base_size = size_of(Bar), extends = 0, version = 1, flex = true}}

	bar_make :: proc(b: ^Buffer) -> (msg: Ref(Bar), err: runtime.Allocator_Error) {
		return allocate(Bar, Bar_Header, b)
	}

    bar_parse :: proc "c" (b: ^Buffer) -> Parse_Result(Bar) {
        return nil
    }

	bar_id_get :: proc "c" (self: Ref(Bar)) -> i64le {
		return self.id
	}

	bar_id_get_unsafe :: proc "c" (self: Ref(Bar)) -> i64le {
		if self._base_size <= int(offset_of(Bar, id)) do return 0
		return self.id
	}

	bar_id_set :: proc "c" (self: Ref(Bar), value: i64le) {
		self.id = value
	}

	bar_id_set_unsafe :: proc "c" (self: Ref_Unsafe(Bar), value: i64le) {
		if self._base_size <= int(offset_of(Bar, id)) do return
		self._val.id = value
	}

	bar_id :: proc {
		bar_id_get,
		bar_id_get_unsafe,
		bar_id_set,
		bar_id_set_unsafe,
	}

    bar_enabled_get :: proc "c" (self: Ref(Bar)) -> bool {
        return self.enabled
    }

	bar_enabled_get_unsafe :: proc "c" (self: Ref_Unsafe(Bar)) -> bool {
		if self._base_size <= int(offset_of(Bar, enabled)) do return false
		return self._val.enabled
	}

	bar_enabled_set :: proc "c" (self: Ref(Bar), value: bool) {
		self.enabled = value
	}

	bar_enabled_set_unsafe :: proc "c" (self: Ref_Unsafe(Bar), value: bool) {
		if self._base_size <= int(offset_of(Bar, enabled)) do return
		self._val.enabled = value
	}

	bar_enabled :: proc {
		bar_enabled_get,
		bar_enabled_get_unsafe,
		bar_enabled_set,
		bar_enabled_set_unsafe,
	}

	bar_name_get :: proc "c" (self: Ref(Bar)) -> string {
		return _str_of(self._buf, &self.name)
	}

	bar_name_get_unsafe :: proc "c" (self: Ref_Unsafe(Bar)) -> string {
		if self._base_size <= int(offset_of(Bar, enabled)) do return ""
		return _str_of(self._buf, &self._val.name)
	}

	bar_name_set :: proc "c" (self: Ref(Bar), value: string) {

	}

	bar_name_set_unsafe :: proc "c" (self: Ref_Unsafe(Bar), value: string) {

	}

	bar_name :: proc {
		bar_name_get,
		bar_name_get_unsafe,
		bar_name_set,
		bar_name_set_unsafe,
	}

	bar_parse_json :: proc(b: []byte, into: ^Buffer) -> (res: Parse_Result(Bar), err: Error) {
		p: json.Parser
		return nil, nil
	}

	buf := buffer_default()
	defer buffer_free(&buf)

    m2 := bar_parse(&buf)

    switch m in m2 {
	case nil:
	case Ref(Bar):
	case Ref_Unsafe(Bar):
    }
	//    buf: Buffer
	msg, err := allocate(Bar, Bar_Header, &buf)
	if err != .None {
		panic(fmt.tprint(err))
	}

	if bar_enabled_get(msg) {

	}

	if bar_name(msg) == "" {
		log.info("empty name")
	}

	bar_name(msg, "hello")

	log.debug(msg)
}

