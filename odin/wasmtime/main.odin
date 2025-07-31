package wasmtime

import c "core:c/libc"
import "core:fmt"
import "core:os"
import "base:runtime"
import "core:strings"

main :: proc() {
    fmt.println("wasmtime")

    config := config_new()
    // config_threads_set(config, false)
    // config_epoch_interruption_set(config, true)
    // config_debug_info_set(config, true)

    tls_init()

    engine := engine_new_with_config(config)
    defer engine_delete(engine)

    store := store_new(engine, nil, nil)

    buffer, ok := os.read_entire_file("odin/wasm/wasm.wasm")
	// buffer, ok := os.read_entire_file("../../wasm/wasm.wasm")
	defer delete(buffer)

	module: ^Module
	// error := module_deserialize(engine, raw_data(buffer), c.size_t(len(buffer)), &module)
	error := module_deserialize_file(engine, cstring("odin/wasm/wasm.cwasm"), &module)
	// error := module_new(engine, raw_data(buffer), c.size_t(len(buffer)), &module)
	if error != nil {
	    name: Name
        error_message(error, &name)
	    fmt.println("failed", string(name.data[0:name.size]))
		return
	}

	ctx := store_context(store)

	instance: Instance
	trap: ^Trap = nil
	error = instance_new(ctx, module, nil, 0, &instance, &trap)
	if error != nil {
		defer error_delete(error)
	    name: Name
        error_message(error, &name)
	    fmt.println("failed to create instance", string(name.data[0:name.size]))
		return
	}
	// defer instance_delete(&instance)

	item: Extern
	name := "hellope"
	ok = instance_export_get(ctx, &instance, raw_data(name), 7, &item)
	if !ok {
	    fmt.println("failed to get hellope")
		return
	}

	fmt.println("SUCCESS!")
}
