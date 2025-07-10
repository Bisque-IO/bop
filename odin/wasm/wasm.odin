package wasm

import "base:runtime"
import "core:fmt"

main :: proc() {
    runtime.print_any_single("hi")
    fmt.println("hi")
//    fmt.println("hi")
}