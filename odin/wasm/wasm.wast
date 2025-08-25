(module $wasm.wasm
  (type (;0;) (func))
  (func $hellope (type 0)
    return)
  (memory (;0;) 2)
  (global $__stack_pointer (mut i32) (i32.const 66560))
  (export "memory" (memory 0))
  (export "hellope" (func $hellope)))
