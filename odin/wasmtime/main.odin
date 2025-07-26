package wasmtime

import c "core:c/libc"
import "core:fmt"
import "core:os"
import "base:runtime"
import "core:strings"

main :: proc() {
    fmt.println("wasmtime")

    config := wasm_config_new()
    // wasmtime_config_wasm_threads_set(config, false)
    wasmtime_config_epoch_interruption_set(config, true)
    wasmtime_config_debug_info_set(config, true)

    wasmtime_tls_init()


    engine := wasm_engine_new_with_config(config)
    defer wasm_engine_delete(engine)

    store := wasmtime_store_new(engine, nil, nil)

    buffer, ok := os.read_entire_file("odin/wasm/wasm.wasm")
	// buffer, ok := os.read_entire_file("../../wasm/wasm.wasm")
	defer delete(buffer)

	module: wasmtime_module
	// error := wasmtime_module_deserialize(engine, raw_data(buffer), c.size_t(len(buffer)), &module)
	error := wasmtime_module_deserialize_file(engine, cstring("odin/wasm/wasm.cwasm"), &module)
	// error := wasmtime_module_new(engine, raw_data(buffer), c.size_t(len(buffer)), &module)
	if error != nil {
	    name: wasm_name_t
        wasmtime_error_message(error, &name)
	    fmt.println("failed", string(name.data[0:name.size]))
		return
	}

	ctx := wasmtime_store_context(store)

	instance: wasmtime_instance_t
	trap: wasm_trap = nil
	error = wasmtime_instance_new(ctx, module, nil, 0, &instance, &trap)
	if error != nil {
	    name: wasm_name_t
        wasmtime_error_message(error, &name)
	    fmt.println("failed to create instance", string(name.data[0:name.size]))
		return
	}

	item: wasmtime_extern_t
	ok = wasmtime_instance_export_get(ctx, &instance, cstring("hellope"), 7, &item)
	if !ok {
	    fmt.println("failed to get hellope")
		return
	}

	fmt.println("SUCCESS!")
}
