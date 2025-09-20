package model

import bop "../"
import bar2 "./bar2"

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
}

Bar_Data :: struct {
	id:      i64,
	name:    bop.String_Offset,
	price:   Price,
	enabled: bool,
}

bar_value :: proc "contextless" (self: bop.Ref(Bar)) -> bop.Value(Bar_Data) {
	val: bop.Value(Bar_Data)
	val._buf = self._buf
	val._base_size = self._base_size
	return val
//    v := bop.Value(Bar_Data) {
//        _buf = self._buf,
//        _base_size = self._base_size
//    }
//    return v
//    return {}
}

bar_id :: proc {
	bar_id_get,
}

bar_id_get :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> i64 {
	return (transmute(^Bar_Data)self.val).id
}

bar_enabled :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> bool {
	if self._base_size <= int(offset_of(Bar_Data, enabled)) do return false
	return (transmute(^Bar_Data)self.val).enabled
}

bar_name :: proc {
	bar_name_get,
	bar_name_set,
}

bar_name_get :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> string {
	return bop._str_of(self._buf, &(transmute(^Bar_Data)self.val).name)
}

bar_name_set :: #force_inline proc "contextless" (self: bop.Ref(Bar), value: string) {

}

import "core:fmt"
import "core:testing"

import "./bar"

@(test)
test_allocate_ :: proc(t: ^testing.T) {
	buf := bop.default_buffer()
	defer bop.buffer_free(&buf)
	//    buf: Buffer
	msg, err := bop.allocate(bar.Bar, &buf)
	if err != .None {
		panic(fmt.aprint(err))
	}
	if bar.enabled(msg) {

	}
}

@(test)
test_allocate :: proc(t: ^testing.T) {
	buf := bop.default_buffer()
	defer bop.buffer_free(&buf)
	//    buf: Buffer
	msg, err := bop.allocate(bar2.Bar, &buf)
	if err != .None {
		panic(fmt.aprint(err))
	}
	if bar2.enabled_get(msg) {

	}
}

@(test)
test_create_order :: proc(t: ^testing.T) {


	fmt.println("hi")
}

