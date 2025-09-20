package bar

import bop "../../"

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

Bar :: struct {}

Bar_Data :: struct {
    id:    i64,
    name:  bop.String_Offset,
    price: Price,
    enabled: bool,
}

id :: proc{id_get}

id_get :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> i64 {
    return (transmute(^Bar_Data)self.val).id
}

enabled :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> bool {
    if self._base_size <= int(offset_of(Bar_Data, enabled)) do return false
    return (transmute(^Bar_Data)self.val).enabled
}

name :: proc {
    name_get,
    name_set,
}

name_get :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> string {
    return bop._str_of(self._buf, &(transmute(^Bar_Data)self.val).name)
}

name_set :: #force_inline proc "contextless" (self: bop.Ref(Bar), value: string) {

}

import "core:testing"
import "core:fmt"

@test
test_create_order :: proc(t: ^testing.T) {


    fmt.println("hi")
}