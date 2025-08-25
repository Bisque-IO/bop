package app

import wamr ".."
import "core:mem"
import "core:os"
import "core:fmt"
import "core:time"

STACK_SIZE :: 64 * mem.Kilobyte
HEAP_SIZE :: 256 * mem.Kilobyte

wasm_module_hellope :: proc(mod_bytes: []byte) -> bool {
    fmt.println("1")
	mod := wamr.load(mod_bytes) or_return
	if mod == nil {
	    fmt.println("mod is nil")
	    return false
	}
	defer wamr.unload(mod)

	fmt.println("2")
	inst := wamr.instantiate(mod, STACK_SIZE, HEAP_SIZE) or_return
	defer wamr.deinstantiate(inst)

	fmt.println("3")
	exec_env := wamr.create_exec_env(inst, STACK_SIZE) or_return
	defer wamr.destroy_exec_env(exec_env)

	fmt.println("4")
	func := wamr.lookup_function(inst, "hellope") or_return

	fmt.println("5")
	func_ok := true

	for _ in 0..<10 {
	    start := time.tick_now()
	    for x in 0..<1000000 {
			func_ok = wamr.wasm_runtime_call_wasm(exec_env, func, 0, nil)
		}
		elapsed := time.tick_since(start)
		fmt.println("", f64(elapsed)/1000000.0)
	}

	return func_ok
}

main :: proc() {
	buffer, ok := os.read_entire_file("odin/wasm/main.aot")
	// buffer, ok := os.read_entire_file("odin/wasm/main.wasm")
	defer delete(buffer)

	fmt.println("buffer size:", len(buffer))

	fmt.println("0")
	init_ok := wamr.init()
	defer wamr.destroy()

	if !wasm_module_hellope(buffer) {
		fmt.println(wamr.get_error())
	}
}
