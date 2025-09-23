package bopc

import "core:fmt"
import "core:mem"

None :: struct {}

Snippet :: struct {
	file:       ^File,
	begin, end: Pos,
}

Error_Code :: enum {
	None,
	Max_Fields_Reached,
	Duplicate_Name,
	Duplicate_Field_Name,
	Field_Type_Not_Set,
	Enum_Not_Int_Type,
}

Error :: struct {
	code:      Error_Code,
	message:   string,
	snippet:   Snippet,
	snippet_2: Snippet,
}

OK :: Error{}

Universe :: struct {
	collections: map[string]^Collection,
	allocator:   mem.Allocator,
}

Collection :: struct {
	universe:  ^Universe,
	name:      string,
	path:      string,
	packages:  map[string]^Package,
	root:      [dynamic]^Package,
	allocator: mem.Allocator,
}

Package :: struct {
	owner:    ^Collection,
	name:     string,
	path:     string,
	files:    map[string]^File,
	children: [dynamic]^Package,
}

Import :: struct {
	// imported package name (e.g. "model" used like "model.Some_Type")
	name: string,
	pkg:  ^Package,
}

Parent :: union #no_nil {
	^File,
	^Field,
}

parent_package :: proc(parent: Parent) -> ^Package {
	switch p in parent {
	case ^File:
		return p.parent
	case ^Field:
		return parent_package(p.parent.parent)
	}
	return nil
}

Child :: union {
	^Const,
	^Alias,
	^Struct,
}

File :: struct {
	parent:   ^Package,
	imports:  [dynamic]Import,
	pkg_name: string,
	children: [dynamic]Child,
}

Const :: struct {
	parent: Parent,
	name:   string,
	type:   Type,
}

Alias :: struct {
	parent: Parent,
	name:   string,
}

Struct :: struct {
	parent: Parent,
	name:   string,
	fields: []^Field,
}

Field :: struct {
	parent: ^Struct,
	name:   string,
	type:   Type,
}

Table :: struct {
	parent:  ^File,
	name:    string,
	columns: []^Column,
}

Column :: struct {
	parent: ^Table,
	name:   string,
}

Variant :: struct {
	parent: Parent,
	name:   string,
	fields: [dynamic]^Variant_Option,
}

Variant_Option :: struct {
	parent: ^Variant,
	type:   Type,
}

Enum :: struct {
	parent:  Parent,
	name:    string,
	type:    Type,
	options: [dynamic]Enum_Option,
	values:  map[Value]None,
}

Enum_Option :: struct {
	parent: ^Enum,
	name:   string,
	value:  Value,
}

Bool :: distinct None

Int :: struct {
	bits:       u16,
	signed:     bool,
	big_endian: bool,
}

Float :: struct {
	bits:       u16,
	big_endian: bool,
}

String :: struct {
	inlined: u32,
}

Pointer :: struct {
	type: Type,
}

Array :: struct {
	elem:   Type,
	length: int,
}

Vector :: struct {
	elem: Type,
}

Dynamic_Array :: Vector

Map :: struct {
	key:   Type,
	value: Type,
}

Sorted_Map :: struct {
	key:   Type,
	value: Type,
}

Unknown :: struct {
	name:    string,
	snippet: Snippet,
}

Action :: struct {
	parent:   ^File,
	name:     string,
	id:       i64,
	request:  Type,
	response: Type,
}

Queue :: struct {
	parent:   ^File,
	name:     string,
	id:       i64,
	request:  Type,
	response: Type,
}

Notification :: struct {
	parent:  ^File,
	name:    string,
	id:      i64,
	message: Type,
}

Daemon :: struct {}

Type_Info :: struct {}

Type :: union {
	Bool,
	Int,
	Float,
	String,
	^Struct,
	^Pointer,
	^Array,
	^Vector,
	^Map,
	^Action,
	^Queue,
	^Notification,
	Unknown,
}

I8 :: Int {
	bits       = 8,
	signed     = true,
	big_endian = false,
}
I16 :: Int {
	bits       = 16,
	signed     = true,
	big_endian = false,
}
I32 :: Int {
	bits       = 32,
	signed     = true,
	big_endian = false,
}
I64 :: Int {
	bits       = 64,
	signed     = true,
	big_endian = false,
}
I128 :: Int {
	bits       = 128,
	signed     = true,
	big_endian = false,
}
I16BE :: Int {
	bits       = 16,
	signed     = true,
	big_endian = true,
}
I32BE :: Int {
	bits       = 32,
	signed     = true,
	big_endian = true,
}
I64BE :: Int {
	bits       = 64,
	signed     = true,
	big_endian = true,
}
I128BE :: Int {
	bits       = 128,
	signed     = true,
	big_endian = true,
}
U8 :: Int {
	bits       = 8,
	signed     = false,
	big_endian = false,
}
U16 :: Int {
	bits       = 16,
	signed     = false,
	big_endian = false,
}
U32 :: Int {
	bits       = 32,
	signed     = false,
	big_endian = false,
}
U64 :: Int {
	bits       = 64,
	signed     = false,
	big_endian = false,
}
U128 :: Int {
	bits       = 128,
	signed     = false,
	big_endian = false,
}
U16BE :: Int {
	bits       = 16,
	signed     = false,
	big_endian = true,
}
U32BE :: Int {
	bits       = 32,
	signed     = false,
	big_endian = true,
}
U64BE :: Int {
	bits       = 64,
	signed     = false,
	big_endian = true,
}
U128BE :: Int {
	bits       = 128,
	signed     = false,
	big_endian = true,
}
F32 :: Float {
	bits       = 32,
	big_endian = false,
}
F64 :: Float {
	bits       = 64,
	big_endian = false,
}
F32BE :: Float {
	bits       = 632,
	big_endian = true,
}
F64BE :: Float {
	bits       = 64,
	big_endian = true,
}

Value :: union {
	bool,
	i8,
	i16,
	i32,
	i64,
	i128,
	i16be,
	i32be,
	i64be,
	i128be,
	string,
	Unknown,
}

collection_create :: proc(name: string, path: string) -> (^Collection, ^Error) {
	return nil, nil
}

add_package :: proc(collection: ^Collection, name: string) -> ^Package {
	code: Error_Code
	code = nil

	c, e := collection_create("", "")
	if e != nil {
		fmt.println()
	}
	p := collection.packages[name]
	if p != nil {
		return p
	}
	p = new(Package, collection.allocator)
	p.owner = collection
	p.name = name
	collection.packages[name] = p
	return p
}
