package bar2

import bop "../../"
import "core:testing"

Bar_Versions :: bop.Struct_Versions{
    id = 1233,
    name = "Bar",
    headers = []bop.Struct_Header{
        {
            base_size = 64,
            id = 1233,
            version = 0,
            flex = true,
        },
        {
            base_size = 64,
            id = 1233,
            version = 0,
            flex = true,
        }
    },
}

Price :: struct {
	open, high, low, close: i32,
}

Bar :: Bar_V2
Bar_Data :: Bar_V2_Data

Bar_Any :: union {
	^Bar_V1_Data,
	^Bar_V2_Data,
}

Bar_V1 :: struct {
}

Bar_V2 :: struct {
}

Bar_V1_Data :: struct {
	id:    i64,
	name:  bop.String_Offset,
	price: Price,
}

Bar_V2_Data :: struct {
	id:      i64,
	price:   Price,
	name:    bop.String_Offset,
	enabled: bool,
}

id :: proc {
	id_get,
	id_get_v1,
    id_get_v2,
}

id_get :: #force_inline proc "contextless" (self: bop.Value(Bar_Any)) -> i64 {
	switch v in self.val {
	case ^Bar_V1_Data:
		return v.id
	case ^Bar_V2_Data:
		return v.id
	case:
		return 0
	}
}

id_get_v1 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V1)) -> i64 {
	return (transmute(^Bar_V1_Data)self.val).id
}

id_get_v2 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V2)) -> i64 {
	return (transmute(^Bar_V2_Data)self.val).id
}

enabled :: proc {
    enabled_get,
    enabled_get_v1,
    enabled_get_v2,
    enabled_set,
    enabled_set_v1,
    enabled_set_v2,
}

enabled_get :: #force_inline proc "contextless" (self: bop.Value(Bar_Any)) -> bool {
    switch v in self.val {
    case ^Bar_V1_Data:
        return false
    case ^Bar_V2_Data:
        return v.enabled
    case:
        return false
    }
}

enabled_get_v1 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V1)) -> bool {
	return false
}

enabled_get_v2 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V2)) -> bool {
    return (transmute(^Bar_V2_Data)self.val).enabled
}

enabled_set :: #force_inline proc "contextless" (self: bop.Value(Bar_Any), value: bool) -> bool {
    switch v in self.val {
    case ^Bar_V1_Data:
        return false
    case ^Bar_V2_Data:
        before := v.enabled
        v.enabled = value
        return before
    case:
        return false
    }
}

enabled_set_v1 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V1), value: bool) -> bool {
    return false
}

enabled_set_v2 :: #force_inline proc "contextless" (self: bop.Ref(Bar_V2), value: bool) -> bool {
    return (transmute(^Bar_V2_Data)self.val).enabled
}

//name :: proc {
//    name_get,
//    name_set,
//}
//
//name_get :: #force_inline proc "contextless" (self: bop.Ref(Bar)) -> string {
//    return bop._str_of(self._buf, &(transmute(^Bar_Data)self.val).name)
//}
//
//name_set :: #force_inline proc "contextless" (self: bop.Ref(Bar), value: string) {
//
//}

parse_v1 :: proc(buf: ^bop.Buffer) -> (bop.Ref(Bar_Any), bop.Error) {
    return {}, .None
}

parse_v2 :: proc(buf: ^bop.Buffer) -> (bop.Ref(Bar_V2), bop.Error) {
    return {}, .None
}

parse_any :: proc(buf: ^bop.Buffer) -> (bop.Value(Bar_Any), bop.Error) {
    return {}, .None
}


@(test)
test_create_order :: proc(t: ^testing.T) {
    m: bop.Value(Bar_Any)
    if enabled(m) {

    }
}

