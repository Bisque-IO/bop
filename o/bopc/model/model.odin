package model

Type :: union {

}

Field :: struct {
    name: string,
}

Struct :: struct {
    name: string,
    fields: []Field,
}