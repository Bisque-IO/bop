package wasmtime

import c "core:c/libc"

import "../libbop"

when ODIN_OS == .Linux && ODIN_ARCH == .amd64 {
	foreign import wasmtimelib "lib/linux_amd64/libwasmtime.a"
} else when ODIN_OS == .Linux && ODIN_ARCH == .arm64 {
	foreign import wasmtimelib "lib/linux_arm64/libwasmtime.a"
} else when ODIN_OS == .Linux && ODIN_ARCH == .riscv64 {
	foreign import wasmtimelib "lib/linux_riscv64/libwasmtime.a"
} else when ODIN_OS == .Darwin && ODIN_ARCH == .arm64 {
	foreign import wasmtimelib "lib/macos_arm64/libwasmtime.a"
} else when ODIN_OS == .Darwin && ODIN_ARCH == .amd64 {
	foreign import wasmtimelib "lib/macos_amd64/libwasmtime.a"
} else when ODIN_OS == .Windows && ODIN_ARCH == .amd64 {
	@(extra_linker_flags = "/NODEFAULTLIB:libcmt")
	foreign import wasmtimelib {
		// "system:Kernel32.lib",
		// "system:User32.lib",
		// "system:Advapi32.lib",
		// "system:Synchronization.lib",
		// "system:Dbghelp.lib",
		"system:ntdll.lib",
		"system:onecore.lib",
		"system:msvcrt.lib",
		"lib/win_amd64/wasmtime.lib",
	} // "system:bcrypt.lib",// "system:ws2_32.lib",
} else {
	#panic("wasmtime lib not available for OS/ARCH")
}

FEATURE_COMPILER :: false
FEATURE_PROFILING :: true
FEATURE_WAT :: true
FEATURE_CACHE :: false
FEATURE_PARALLEL_COMPILATION :: false
FEATURE_WASI :: false
FEATURE_LOGGING :: true
FEATURE_COREDUMP :: true
FEATURE_ADDR2LINE :: true
FEATURE_DEMANGLE :: true
FEATURE_THREADS :: true
FEATURE_GC :: true
FEATURE_GC_DRC :: true
FEATURE_GC_NULL :: true
FEATURE_ASYNC :: false
FEATURE_CRANELIFT :: false
FEATURE_WINCH :: false
FEATURE_DEBUG_BUILTINS :: true
FEATURE_POOLING_ALLOCATOR :: true
FEATURE_COMPONENT_MODEL :: false

#assert(size_of(f32) == size_of(u32))
#assert(size_of(f64) == size_of(u64))
#assert(size_of(uintptr) == size_of(u32) || size_of(uintptr) == size_of(u64))

Byte_Vec :: struct {
	size: c.size_t,
	data: [^]byte,
}

Name :: Byte_Vec

name_new_from_string :: proc "c" (out: ^Name, s: string) {
	byte_vec_new(out, c.size_t(len(s)), raw_data(s[:]))
}

Mutability :: enum u8 {
	Const,
	Var,
}

Limits :: struct {
	min, max: u32,
}

Limits_Max_Default :: 0xffffffff


////////////////////////////////////////////////////
//
// config
//
//////////////////////////

// Different ways that Wasmtime can compile WebAssembly
//
// The default value is #WASMTIME_STRATEGY_AUTO.
Strategy :: enum c.int {
	Auto,
	Cranelift,
}

// Different ways Wasmtime can optimize generated code.
//
// The default value is #WASMTIME_OPT_LEVEL_SPEED.
Opt_Level :: enum c.int {
	None,
	Speed,
	Speed_And_Size,
}

// Different ways to profile JIT code.
//
// The default is #WASMTIME_PROFILING_STRATEGY_NONE.
Profiling_Strategy :: enum c.int {
	None,
	JIT_Dump,
	VTune,
	Perf_Map,
}

Config :: struct {}

// Return the data from a LinearMemory instance.
//
// The size in bytes as well as the maximum number of bytes that can be
// allocated should be returned as well.
//
// For more information about see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/trait.LinearMemory.html
Memory_Get_Callback :: #type proc "c" (env: rawptr, byte_size: ^c.size_t, byte_capacity: ^c.size_t) -> [^]u8

// Grow the memory to the `new_size` in bytes.
//
// For more information about the parameters see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/trait.LinearMemory.html#tymethod.grow_to
Memory_Grow_Callback :: #type proc "c" (env: rawptr, new_size: c.size_t) -> ^Error

// A callback to create a new LinearMemory from the specified parameters.
//
// The result should be written to `memory_ret` and wasmtime will own the values
// written into that struct.
//
// This callback must be thread-safe.
//
// For more information about the parameters see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/trait.MemoryCreator.html#tymethod.new_memory
New_Memory_Callback :: #type proc "c" (
	env: rawptr,
	ty: Memory_Type,
	minimum: c.size_t,
	maximum: c.size_t,
	reserved_size_in_bytes: c.size_t,
	guard_size_in_bytes: c.size_t,
	memory_ret: ^Linear_Memory,
)

// A representation of custom memory creator and methods for an instance of
// LinearMemory.
//
// For more information see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/trait.MemoryCreator.html
Memory_Creator :: struct {
	env:        rawptr,
	new_memory: New_Memory_Callback,
	finalizer:  Finalizer,
}

Finalizer :: #type proc "c" (env: rawptr)

// A LinearMemory instance created from a #wasmtime_new_memory_callback_t.
//
// For more information see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/trait.LinearMemory.html
Linear_Memory :: struct {
	env:         rawptr,
	get_memory:  Memory_Get_Callback,
	grow_memory: Memory_Grow_Callback,
	finalizer:   Finalizer,
}

// A type containing configuration options for the pooling allocator.
//
// For more information see the Rust documentation at
// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html
Pooling_Allocation_Config :: struct {}


@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	config_new :: proc() -> ^Config ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Configures whether DWARF debug information is constructed at runtime
	// to describe JIT code.
	//
	// This setting is `false` by default. When enabled it will attempt to inform
	// native debuggers about DWARF debugging information for JIT code to more
	// easily debug compiled WebAssembly via native debuggers. This can also
	// sometimes improve the quality of output when profiling is enabled.
	config_debug_info_set :: proc(config: ^Config, value: bool) ---

	// Whether or not fuel is enabled for generated code.
	//
	// This setting is `false` by default. When enabled it will enable fuel counting
	// meaning that fuel will be consumed every time a wasm instruction is executed,
	// and trap when reaching zero.
	config_consume_fuel_set :: proc(config: ^Config, value: bool) ---

	// Whether or not epoch-based interruption is enabled for generated code.
	//
	// This setting is `false` by default. When enabled wasm code will check the
	// current epoch periodically and abort if the current epoch is beyond a
	// store-configured limit.
	//
	// Note that when this setting is enabled all stores will immediately trap and
	// need to have their epoch deadline otherwise configured with
	// #wasmtime_context_set_epoch_deadline.
	//
	// Note that the current epoch is engine-local and can be incremented with
	// #wasmtime_engine_increment_epoch.
	config_epoch_interruption_set :: proc(config: ^Config, value: bool) ---

	// Configures the maximum stack size, in bytes, that JIT code can use.
	//
	// This setting is 2MB by default. Configuring this setting will limit the
	// amount of native stack space that JIT code can use while it is executing. If
	// you're hitting stack overflow you can try making this setting larger, or if
	// you'd like to limit wasm programs to less stack you can also configure this.
	//
	// Note that this setting is not interpreted with 100% precision. Additionally
	// the amount of stack space that wasm takes is always relative to the first
	// invocation of wasm on the stack, so recursive calls with host frames in the
	// middle will all need to fit within this setting.
	config_max_wasm_stack_set :: proc(config: ^Config, value: c.size_t) ---

	// Configures whether the WebAssembly tail call proposal is enabled.
	//
	// This setting is `false` by default.
	config_wasm_tail_call_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly reference types proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_reference_types_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly typed function reference types
	// proposal is enabled.
	//
	// This setting is `false` by default.
	config_wasm_function_references_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly GC proposal is enabled.
	//
	// This setting is `false` by default.
	config_wasm_gc_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly SIMD proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_simd_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly relaxed SIMD proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_relaxed_simd_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly relaxed SIMD proposal is
	// in deterministic mode.
	//
	// This setting is `false` by default.
	config_wasm_relaxed_simd_deterministic_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly bulk memory proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_bulk_memory_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly multi value proposal is
	// enabled.
	//
	// This setting is `true` by default.
	config_wasm_multi_value_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly multi-memory proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_multi_memory_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly memory64 proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_memory64_set :: proc(config: ^Config, value: bool) ---

	// Configures whether the WebAssembly wide-arithmetic proposal is
	// enabled.
	//
	// This setting is `false` by default.
	config_wasm_wide_arithmetic_set :: proc(config: ^Config, value: bool) ---

	// Configures the profiling strategy used for JIT code.
	//
	// This setting in #WASMTIME_PROFILING_STRATEGY_NONE by default.
	config_profiler_set :: proc(config: ^Config, value: Profiling_Strategy) ---

	// Configures whether `memory_reservation` is the maximal size of linear
	// memory.
	//
	// This setting is `false` by default.
	//
	// For more information see the Rust documentation at
	// https://bytecodealliance.github.io/wasmtime/api/wasmtime/struct.Config.html#method.memory_may_move.
	config_memory_may_move_set :: proc(config: ^Config, value: bool) ---

	// Configures the size, in bytes, of initial memory reservation size for
	// linear memories.
	//
	// For more information see the Rust documentation at
	// https://bytecodealliance.github.io/wasmtime/api/wasmtime/struct.Config.html#method.memory_reservation.
	config_memory_reservation_set :: proc(config: ^Config, value: u64) ---

	// Configures the guard region size for linear memory.
	//
	// For more information see the Rust documentation at
	// https://bytecodealliance.github.io/wasmtime/api/wasmtime/struct.Config.html#method.memory_guard_size.
	config_memory_guard_size_set :: proc(config: ^Config, value: u64) ---

	// Configures the size, in bytes, of the extra virtual memory space
	// reserved for memories to grow into after being relocated.
	//
	// For more information see the Rust documentation at
	// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.memory_reservation_for_growth
	config_memory_reservation_for_growth_set :: proc(config: ^Config, value: u64) ---

	// Configures whether to generate native unwind information (e.g.
	// .eh_frame on Linux).
	//
	// This option defaults to true.
	//
	// For more information see the Rust documentation at
	// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.native_unwind_info
	config_native_unwind_info_set :: proc(config: ^Config, value: bool) ---

	// Configures whether, when on macOS, Mach ports are used for exception
	// handling instead of traditional Unix-based signal handling.
	//
	// This option defaults to true, using Mach ports by default.
	//
	// For more information see the Rust documentation at
	// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.macos_use_mach_ports
	config_macos_use_mach_ports_set :: proc(config: ^Config, value: bool) ---

	// Sets a custom memory creator.
	//
	// Custom memory creators are used when creating host Memory objects or when
	// creating instance linear memories for the on-demand instance allocation
	// strategy.
	//
	// The config does//*not** take ownership of the #wasmtime_memory_creator_t
	// passed in, but instead copies all the values in the struct.
	//
	// For more information see the Rust documentation at
	// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.with_host_memory
	config_host_memory_creator_set :: proc(config: ^Config, creator: ^Memory_Creator) ---

	// Configures whether copy-on-write memory-mapped data is used to
	// initialize a linear memory.
	//
	// Initializing linear memory via a copy-on-write mapping can drastically
	// improve instantiation costs of a WebAssembly module because copying memory is
	// deferred. Additionally if a page of memory is only ever read from WebAssembly
	// and never written too then the same underlying page of data will be reused
	// between all instantiations of a module meaning that if a module is
	// instantiated many times this can lower the overall memory required needed to
	// run that module.
	//
	// This option defaults to true.
	//
	// For more information see the Rust documentation at
	// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.memory_init_cow
	config_memory_init_cow_set :: proc(config: ^Config, value: bool) ---
}

when FEATURE_THREADS {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {
		// Configures whether the WebAssembly threading proposal is enabled.
		//
		// This setting is `false` by default.
		//
		// Note that threads are largely unimplemented in Wasmtime at this time.
		config_wasm_threads_set :: proc(config: ^Config, value: bool) ---
	}
}

when FEATURE_COMPILER {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {
		// Configures whether the WebAssembly stack switching
		// proposal is enabled.
		//
		// This setting is `false` by default.
		config_wasm_stack_switching_set :: proc(config: ^Config, value: bool) ---

		// Configures how JIT code will be compiled.
		//
		// This setting is #WASMTIME_STRATEGY_AUTO by default.
		config_strategy_set :: proc(config: ^Config, strategy: Strategy) ---

		// Configures whether Cranelift's debug verifier is enabled.
		//
		// This setting in `false` by default.
		//
		// When cranelift is used for compilation this enables expensive debug checks
		// within Cranelift itself to verify it's correct.
		config_cranelift_debug_verifier_set :: proc(config: ^Config, value: bool) ---

		// Configures whether Cranelift should perform a NaN-canonicalization
		// pass.
		//
		// When Cranelift is used as a code generation backend this will configure
		// it to replace NaNs with a single canonical value. This is useful for users
		// requiring entirely deterministic WebAssembly computation.
		//
		// This is not required by the WebAssembly spec, so it is not enabled by
		// default.
		//
		// The default value for this is `false`
		config_cranelift_nan_canonicalization_set :: proc(config: ^Config, value: bool) ---

		// Configures Cranelift's optimization level for JIT code.
		//
		// This setting in #WASMTIME_OPT_LEVEL_SPEED by default.
		config_cranelift_opt_level_set :: proc(config: ^Config, opt_level: Opt_Level) ---

		// Enables a target-specific flag in Cranelift.
		//
		// This can be used, for example, to enable SSE4.2 on x86_64 hosts. Settings can
		// be explored with `wasmtime settings` on the CLI.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.cranelift_flag_enable
		config_cranelift_flag_enable :: proc(config: ^Config, prop: cstring) ---

		// Sets a target-specific flag in Cranelift to the specified value.
		//
		// This can be used, for example, to enable SSE4.2 on x86_64 hosts. Settings can
		// be explored with `wasmtime settings` on the CLI.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.cranelift_flag_set
		config_cranelift_flag_set :: proc(config: ^Config, prop: cstring, value: cstring) ---
	}
}

when FEATURE_PARALLEL_COMPILATION {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {
		// Configure whether wasmtime should compile a module using multiple
		// threads.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.parallel_compilation.
		config_parallel_compilation_set :: proc(config: ^Config, value: bool) ---
	}
}

when FEATURE_CACHE {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {
		// Enables Wasmtime's cache and loads configuration from the specified
		// path.
		//
		// By default the Wasmtime compilation cache is disabled. The configuration path
		// here can be `NULL` to use the default settings, and otherwise the argument
		// here must be a file on the filesystem with TOML configuration -
		// https://bytecodealliance.github.io/wasmtime/cli-cache.html.
		//
		// An error is returned if the cache configuration could not be loaded or if the
		// cache could not be enabled.
		config_cache_config_load :: proc(config: ^Config, value: cstring) -> ^Error ---
	}
}

when FEATURE_POOLING_ALLOCATOR {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {
		// Creates a new pooling allocation configuration object. Must be
		// deallocated with a call to wasmtime_pooling_allocation_config_delete.
		pooling_allocation_config_new :: proc() -> ^Pooling_Allocation_Config ---

		// Deallocates a pooling allocation configuration object created with a
		// call to wasmtime_pooling_allocation_config_new.
		pooling_allocation_config_delete :: proc(config: ^Pooling_Allocation_Config) ---

		// Configures the maximum number of “unused warm slots” to retain in the
		// pooling allocator.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_unused_warm_slots.
		pooling_allocation_config_max_unused_warm_slots_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The target number of decommits to do per batch.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.decommit_batch_size.
		pooling_allocation_config_decommit_batch_size_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// How much memory, in bytes, to keep resident for each linear memory
		// after deallocation.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.linear_memory_keep_resident.
		pooling_allocation_config_linear_memory_keep_resident_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// How much memory, in bytes, to keep resident for each table after
		// deallocation.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.table_keep_resident.
		pooling_allocation_config_table_keep_resident_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// The maximum number of concurrent component instances supported
		// (default is 1000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.total_component_instances.
		pooling_allocation_config_total_component_instances_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum size, in bytes, allocated for a component instance’s
		// VMComponentContext metadata.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_component_instance_size.
		pooling_allocation_config_max_component_instance_size_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// The maximum number of core instances a single component may contain
		// (default is unlimited).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_core_instances_per_component.
		pooling_allocation_config_max_core_instances_per_component_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum number of Wasm linear memories that a single component may
		// transitively contain (default is unlimited).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_memories_per_component.
		pooling_allocation_config_max_memories_per_component_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum number of tables that a single component may transitively
		// contain (default is unlimited).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_tables_per_component.
		pooling_allocation_config_max_tables_per_component_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum number of concurrent Wasm linear memories supported
		// (default is 1000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.total_memories.
		pooling_allocation_config_total_memories_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum number of concurrent tables supported (default is 1000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.total_tables.
		pooling_allocation_config_total_tables_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum number of concurrent core instances supported (default is 1000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.total_core_instances.
		pooling_allocation_config_total_core_instances_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum size, in bytes, allocated for a core instance’s VMContext
		// metadata.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_core_instance_size.
		pooling_allocation_config_max_core_instance_size_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// The maximum number of defined tables for a core module (default is 1).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_tables_per_module.
		pooling_allocation_config_max_tables_per_module_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum table elements for any table defined in a module (default is 20000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.table_elements.
		pooling_allocation_config_table_elements_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---


		// The maximum number of defined linear memories for a module (default is 1).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_memories_per_module.
		pooling_allocation_config_max_memories_per_module_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// The maximum byte size that any WebAssembly linear memory may grow to.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.max_memory_size.
		pooling_allocation_config_max_memory_size_set :: proc(config: ^Pooling_Allocation_Config, value: c.size_t) ---

		// The maximum number of concurrent GC heaps supported (default is 1000).
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.PoolingAllocationConfig.html#method.total_gc_heaps.
		pooling_allocation_config_total_gc_heaps_set :: proc(config: ^Pooling_Allocation_Config, value: u32) ---

		// Sets the Wasmtime allocation strategy to use the pooling allocator. It
		// does not take ownership of the pooling allocation configuration object, which
		// must be deleted with a call to wasmtime_pooling_allocation_config_delete.
		//
		// For more information see the Rust documentation at
		// https://docs.wasmtime.dev/api/wasmtime/struct.Config.html#method.allocation_strategy.
		pooling_allocation_strategy_set :: proc(config: ^Config, value: ^Pooling_Allocation_Config) ---
	}
}

////////////////////////////////////////////////////
//
// engine
//
//////////////////////////

Engine :: struct {}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	engine_new :: proc() -> ^Engine ---
	engine_delete :: proc(engine: ^Engine) ---
	engine_new_with_config :: proc(config: ^Config) -> ^Engine ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Create a new reference to the same underlying engine.
	//
	// This function clones the reference-counted pointer to the internal object,
	// and must be freed using #wasm_engine_delete.
	engine_clone :: proc(engine: ^Engine) -> ^Engine ---

	// Increments the engine-local epoch variable.
	//
	// This function will increment the engine's current epoch which can be used to
	// force WebAssembly code to trap if the current epoch goes beyond the
	// #wasmtime_store_t configured epoch deadline.
	//
	// This function is safe to call from any thread, and it is also
	// async-signal-safe.
	//
	// See also #wasmtime_config_epoch_interruption_set.
	engine_increment_epoch :: proc(engine: ^Engine) ---

	// Returns whether this engine is using the Pulley interpreter to execute
	// WebAssembly code.
	engine_is_pulley :: proc(engine: ^Engine) -> bool ---
}

////////////////////////////////////////////////////
//
// error
//
//////////////////////////


// wasmtime_error_t
// Convenience alias for #wasmtime_error
//
// wasmtime_error
// Errors generated by Wasmtime.
// \headerfile wasmtime/error.h
//
// This opaque type represents an error that happened as part of one of the
// functions below. Errors primarily have an error message associated with them
// at this time, which you can acquire by calling #wasmtime_error_message.
//
// Errors are safe to share across threads and must be deleted with
// #wasmtime_error_delete.
Error :: struct {}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new error with the provided message.
	error_new :: proc(value: cstring) -> ^Error ---

	// Deletes an error.
	error_delete :: proc(error: ^Error) ---

	// Returns the string description of this error.
	//
	// This will "render" the error to a string and then return the string
	// representation of the error to the caller. The `message` argument should be
	// uninitialized before this function is called and the caller is responsible
	// for deallocating it with #wasm_byte_vec_delete afterwards.
	error_message :: proc(error: ^Error, message: ^Name) ---

	// Attempts to extract a WASI-specific exit status from this error.
	//
	// Returns `true` if the error is a WASI "exit" trap and has a return status.
	// If `true` is returned then the exit status is returned through the `status`
	// pointer. If `false` is returned then this is not a wasi exit trap.
	error_exit_status :: proc(error: ^Error, status: ^c.int) -> bool ---

	// Attempts to extract a WebAssembly trace from this error.
	//
	// This is similar to #wasm_trap_trace except that it takes a #wasmtime_error_t
	// as input. The `out` argument will be filled in with the wasm trace, if
	// present.
	error_wasm_trace :: proc(error: ^Error, out: ^Frame_Vec) ---
}

////////////////////////////////////////////////////
//
// export
//
//////////////////////////

Export_Type :: struct {}

Export_Type_Vec :: struct {
	size: c.size_t,
	data: [^]^Export_Type,
}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	exporttype_new :: proc(name: ^Name, extern_type: ^Extern_Type) -> ^Export_Type ---
	exporttype_delete :: proc(type: ^Export_Type) ---
	exporttype_name :: proc(type: ^Export_Type) -> ^Name ---
	exporttype_type :: proc(type: ^Export_Type) -> ^Extern_Type ---

	exporttype_vec_new_empty :: proc(out: ^Export_Type_Vec) ---
	exporttype_vec_new :: proc(vec: ^Export_Type_Vec, size: c.size_t, data: [^]^Export_Type) ---
	exporttype_vec_new_uninitialized :: proc(out: ^Export_Type_Vec, size: c.size_t) ---
	exporttype_vec_copy :: proc(out: ^Export_Type_Vec, src: ^Export_Type_Vec) ---
	exporttype_vec_delete :: proc(vec: ^Export_Type_Vec) ---
}

////////////////////////////////////////////////////
//
// extern
//
//////////////////////////

Extern_Kind :: enum u8 {
	Func          = 0,
	Global        = 1,
	Table         = 2,
	Memory        = 3,
	Shared_Memory = 4,
}

// wasmtime_extern_union_t
// Convenience alias for #wasmtime_extern_union
//
// wasmtime_extern_union
// Container for different kinds of extern items.
//
// This type is contained in #wasmtime_extern_t and contains the payload for the
// various kinds of items an extern wasm item can be.
Extern_Union :: struct #raw_union {
	func:          Func,
	global:        Global,
	table:         Table,
	memory:        Memory,
	shared_memory: Shared_Memory,
}


// wasmtime_extern_t
// Convenience alias for #wasmtime_extern_t
//
// wasmtime_extern
// Container for different kinds of extern items.
//
// Note that this structure may contain an owned value, namely
// #wasmtime_module_t, depending on the context in which this is used. APIs
// which consume a #wasmtime_extern_t do not take ownership, but APIs that
// return #wasmtime_extern_t require that #wasmtime_extern_delete is called to
// deallocate the value.
Extern :: struct {
	// Discriminant of which field of #of is valid.
	kind: Extern_Kind,
	// Container for the extern item's value.
	of:   Extern_Union,
}

Extern_Type :: struct {}

Extern_Type_Vec :: struct {
	size: c.size_t,
	data: [^]^Extern_Type,
}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	externtype_new :: proc(name: ^Name, extern_type: ^Extern_Type) -> ^Extern_Type ---
	externtype_delete :: proc(type: ^Extern_Type) ---
	externtype_name :: proc(type: ^Extern_Type) -> ^Name ---
	externtype_type :: proc(type: ^Extern_Type) -> ^Extern_Type ---

	externtype_kind :: proc(type: ^Extern_Type) -> Extern_Kind ---

	functype_as_externtype :: proc(type: ^Func_Type) -> ^Extern_Type ---
	globaltype_as_externtype :: proc(type: ^Global_Type) -> ^Extern_Type ---
	tabletype_as_externtype :: proc(type: ^Table_Type) -> ^Extern_Type ---
	memorytype_as_externtype :: proc(type: ^Memory_Type) -> ^Extern_Type ---

	externtype_as_functype :: proc(type: ^Extern_Type) -> ^Func_Type ---
	externtype_as_globaltype :: proc(type: ^Extern_Type) -> ^Global_Type ---
	externtype_as_tabletype :: proc(type: ^Extern_Type) -> ^Table_Type ---
	externtype_as_memorytype :: proc(type: ^Extern_Type) -> ^Memory_Type ---

	externtype_vec_new_empty :: proc(out: ^Extern_Type_Vec) ---
	externtype_vec_new :: proc(extern: ^Extern_Type_Vec, size: c.size_t, data: [^]^Extern_Type) ---
	externtype_vec_new_uninitialized :: proc(out: ^Extern_Type_Vec, size: c.size_t) ---
	externtype_vec_copy :: proc(out: ^Extern_Type_Vec, src: ^Extern_Type_Vec) ---
	externtype_vec_delete :: proc(vec: ^Extern_Type_Vec) ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Deletes a #wasmtime_extern_t.
	extern_delete :: proc(extern: ^Extern) ---

	// Returns the type of the #wasmtime_extern_t defined within the given
	// store.
	//
	// Does not take ownership of `context` or `val`, but the returned
	// #wasm_externtype_t is an owned value that needs to be deleted.
	extern_type :: proc(ctx: ^Context, extern: ^Extern) -> ^Extern_Type ---
}

////////////////////////////////////////////////////
//
// func
//
//////////////////////////

// Representation of a function in Wasmtime.
//
// Functions in Wasmtime are represented as an index into a store and don't
// have any data or destructor associated with the #wasmtime_func_t value.
// Functions cannot interoperate between #wasmtime_store_t instances and if the
// wrong function is passed to the wrong store then it may trigger an assertion
// to abort the process.
Func :: struct {
	// Internal identifier of what store this belongs to.
	//
	// This field may be zero when used in conjunction with #wasmtime_val_t
	// to represent a null `funcref` value in WebAssembly. For a valid function
	// this field is otherwise never zero.
	store_id: u64,

	// Private field for Wasmtime, undefined if `store_id` is zero.
	private:  rawptr,
}

Func_Type :: struct {}

// wasmtime_caller_t
// Alias to #wasmtime_caller
//
// Structure used to learn about the caller of a host-defined function.
// wasmtime_caller
//
// This structure is an argument to #wasmtime_func_callback_t. The purpose
// of this structure is acquire a #wasmtime_context_t pointer to interact with
// objects, but it can also be used for inspect the state of the caller (such as
// getting memories and functions) with #wasmtime_caller_export_get.
//
// This object is never owned and does not need to be deleted.
Caller :: struct {}

// Callback signature for #wasmtime_func_new.
//
// This is the function signature for host functions that can be made accessible
// to WebAssembly. The arguments to this function are:
//
// @param env user-provided argument passed to #wasmtime_func_new
// @param caller a temporary object that can only be used during this function
// call. Used to acquire #wasmtime_context_t or caller's state
// @param args the arguments provided to this function invocation
// @param nargs how many arguments are provided
// @param results where to write the results of this function
// @param nresults how many results must be produced
//
// Callbacks are guaranteed to get called with the right types of arguments, but
// they must produce the correct number and types of results. Failure to do so
// will cause traps to get raised on the wasm side.
//
// This callback can optionally return a #wasm_trap_t indicating that a trap
// should be raised in WebAssembly. It's expected that in this case the caller
// relinquishes ownership of the trap and it is passed back to the engine.
Func_Callback :: #type proc "c" (
	env: rawptr,
	caller: ^Caller,
	args: [^]Val,
	num_args: c.size_t,
	results: [^]Val,
	num_results: c.size_t,
) -> ^Trap

// Callback signature for #wasmtime_func_new_unchecked.
//
// This is the function signature for host functions that can be made accessible
// to WebAssembly. The arguments to this function are:
//
// @param env user-provided argument passed to #wasmtime_func_new_unchecked
// @param caller a temporary object that can only be used during this function
//        call. Used to acquire #wasmtime_context_t or caller's state
// @param args_and_results storage space for both the parameters to the
//        function as well as the results of the function. The size of this
//        array depends on the function type that the host function is created
//        with, but it will be the maximum of the number of parameters and
//        number of results.
// @param num_args_and_results the size of the `args_and_results` parameter in
//        units of #wasmtime_val_raw_t.
//
// This callback can optionally return a #wasm_trap_t indicating that a trap
// should be raised in WebAssembly. It's expected that in this case the caller
// relinquishes ownership of the trap and it is passed back to the engine.
//
// This differs from #wasmtime_func_callback_t in that the payload of
// `args_and_results` does not have type information, nor does it have sizing
// information. This is especially unsafe because it's only valid within the
// particular #wasm_functype_t that the function was created with. The onus is
// on the embedder to ensure that `args_and_results` are all read correctly
// for parameters and all written for results within the execution of a
// function.
//
// Parameters will be listed starting at index 0 in the `args_and_results`
// array. Results are also written starting at index 0, which will overwrite
// the arguments.
Func_Unchecked_Callback :: #type proc "c" (
	env: rawptr,
	caller: ^Caller,
	args: [^]Val,
	results: [^]Val,
	num_args_and_results: c.size_t,
) -> ^Trap

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	functype_new :: proc(params: ^Val_Type_Vec, results: ^Val_Type_Vec) -> ^Func_Type ---
	functype_delete :: proc(type: ^Func_Type) ---
	functype_params :: proc(type: ^Func_Type) -> ^Val_Type_Vec ---
	functype_results :: proc(type: ^Func_Type) -> ^Val_Type_Vec ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {

	// Creates a new host-defined function.
	//
	// Inserts a host-defined function into the `store` provided which can be used
	// to then instantiate a module with or define within a #wasmtime_linker_t.
	//
	// @param store the store in which to create the function
	// @param type the wasm type of the function that's being created
	// @param callback the host-defined callback to invoke
	// @param env host-specific data passed to the callback invocation, can be
	// `NULL`
	// @param finalizer optional finalizer for `env`, can be `NULL`
	// @param ret the #wasmtime_func_t return value to be filled in.
	//
	// The returned function can only be used with the specified `store`.
	func_new :: proc(store: ^Context, type: ^Func_Type, callback: Func_Callback, env: rawptr, finalizer: Finalizer, ret: ^Func) ---


	// Creates a new host function in the same manner of #wasmtime_func_new,
	//        but the function-to-call has no type information available at runtime.
	//
	// This function is very similar to #wasmtime_func_new. The difference is that
	// this version is "more unsafe" in that when the host callback is invoked there
	// is no type information and no checks that the right types of values are
	// produced. The onus is on the consumer of this API to ensure that all
	// invariants are upheld such as:
	//
	//// The host callback reads parameters correctly and interprets their types
	//   correctly.
	//// If a trap doesn't happen then all results are written to the results
	//   pointer. All results must have the correct type.
	//// Types such as `funcref` cannot cross stores.
	//// Types such as `externref` have valid reference counts.
	//
	// It's generally only recommended to use this if your application can wrap
	// this in a safe embedding. This should not be frequently used due to the
	// number of invariants that must be upheld on the wasm<->host boundary. On the
	// upside, though, this flavor of host function will be faster to call than
	// those created by #wasmtime_func_new (hence the reason for this function's
	// existence).
	func_new_unchecked :: proc(store: ^Context, type: ^Func_Type, callback: Func_Unchecked_Callback, env: rawptr, finalizer: Finalizer, ret: ^Func) ---


	// Returns the type of the function specified
	//
	// The returned #wasm_functype_t is owned by the caller.

	func_type :: proc(store: ^Context, func: ^Func) -> ^Func_Type ---


	// Call a WebAssembly function.
	//
	// This function is used to invoke a function defined within a store. For
	// example this might be used after extracting a function from a
	// #wasmtime_instance_t.
	//
	// @param store the store which owns `func`
	// @param func the function to call
	// @param args the arguments to the function call
	// @param nargs the number of arguments provided
	// @param results where to write the results of the function call
	// @param nresults the number of results expected
	// @param trap where to store a trap, if one happens.
	//
	// There are three possible return states from this function:
	//
	// 1. The returned error is non-null. This means `results`
	//    wasn't written to and `trap` will have `NULL` written to it. This state
	//    means that programmer error happened when calling the function, for
	//    example when the size of the arguments/results was wrong, the types of the
	//    arguments were wrong, or arguments may come from the wrong store.
	// 2. The trap pointer is filled in. This means the returned error is `NULL` and
	//    `results` was not written to. This state means that the function was
	//    executing but hit a wasm trap while executing.
	// 3. The error and trap returned are both `NULL` and `results` are written to.
	//    This means that the function call succeeded and the specified results were
	//    produced.
	//
	// The `trap` pointer cannot be `NULL`. The `args` and `results` pointers may be
	// `NULL` if the corresponding length is zero.
	//
	// Does not take ownership of #wasmtime_val_t arguments. Gives ownership of
	// #wasmtime_val_t results.

	func_call :: proc(store: ^Context, func: ^Func, args: [^]Val, num_args: c.size_t, results: [^]Val, num_results: c.size_t, trap: ^^Trap) -> ^Error ---


	// Call a WebAssembly function in an "unchecked" fashion.
	//
	// This function is similar to #wasmtime_func_call except that there is no type
	// information provided with the arguments (or sizing information). Consequently
	// this is less safe to call since it's up to the caller to ensure that `args`
	// has an appropriate size and all the parameters are configured with their
	// appropriate values/types. Additionally all the results must be interpreted
	// correctly if this function returns successfully.
	//
	// Parameters must be specified starting at index 0 in the `args_and_results`
	// array. Results are written starting at index 0, which will overwrite
	// the arguments.
	//
	// Callers must ensure that various correctness variants are upheld when this
	// API is called such as:
	//
	//// The `args_and_results` pointer has enough space to hold all the parameters
	//   and all the results (but not at the same time).
	//// The `args_and_results_len` contains the length of the `args_and_results`
	//   buffer.
	//// Parameters must all be configured as if they were the correct type.
	//// Values such as `externref` and `funcref` are valid within the store being
	//   called.
	//
	// When in doubt it's much safer to call #wasmtime_func_call. This function is
	// faster than that function, but the tradeoff is that embeddings must uphold
	// more invariants rather than relying on Wasmtime to check them for you.

	func_call_unchecked :: proc(store: ^Context, func: ^Func, args_and_results: [^]Val, num_args_and_results: c.size_t, trap: ^^Trap) -> ^Error ---


	// Loads a #wasmtime_extern_t from the caller's context
	//
	// This function will attempt to look up the export named `name` on the caller
	// instance provided. If it is found then the #wasmtime_extern_t for that is
	// returned, otherwise `NULL` is returned.
	//
	// Note that this only works for exported memories right now for WASI
	// compatibility.
	//
	// @param caller the caller object to look up the export from
	// @param name the name that's being looked up
	// @param name_len the byte length of `name`
	// @param item where to store the return value
	//
	// Returns a nonzero value if the export was found, or 0 if the export wasn't
	// found. If the export wasn't found then `item` isn't written to.

	caller_export_get :: proc(caller: ^Caller, name: [^]byte, name_len: c.size_t, item: ^Extern) -> bool ---


	// Returns the store context of the caller object.

	caller_context :: proc(caller: ^Caller) -> ^Context ---


	// Converts a `raw` nonzero `funcref` value from #wasmtime_val_raw_t
	// into a #wasmtime_func_t.
	//
	// This function can be used to interpret nonzero values of the `funcref` field
	// of the #wasmtime_val_raw_t structure. It is assumed that `raw` does not have
	// a value of 0, or otherwise the program will abort.
	//
	// Note that this function is unchecked and unsafe. It's only safe to pass
	// values learned from #wasmtime_val_raw_t with the same corresponding
	// #wasmtime_context_t that they were produced from. Providing arbitrary values
	// to `raw` here or cross-context values with `context` is UB.

	func_from_raw :: proc(ctx: ^Context, raw: rawptr, ret: ^Func) ---


	// Converts a `func`  which belongs to `context` into a `usize`
	// parameter that is suitable for insertion into a #wasmtime_val_raw_t.

	func_to_raw :: proc(ctx: ^Context, raw: rawptr, func: ^Func) -> rawptr ---
}

////////////////////////////////////////////////////
//
// global
//
//////////////////////////

// Representation of a global in Wasmtime.
//
// Globals in Wasmtime are represented as an index into a store and don't
// have any data or destructor associated with the #wasmtime_global_t value.
// Globals cannot interoperate between #wasmtime_store_t instances and if the
// wrong global is passed to the wrong store then it may trigger an assertion
// to abort the process.
Global :: struct {
	// Internal identifier of what store this belongs to, never zero.
	store_id: u64,
	private1: u32,
	private2: u32,
	private3: u32,
}

Global_Type :: struct {}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	globaltype_new :: proc(type: ^Val_Type, mutability: Mutability) -> ^Global_Type ---
	globaltype_delete :: proc(type: ^Global_Type) ---
	globaltype_content :: proc(type: ^Global_Type) -> ^Val_Type ---
	globaltype_mutability :: proc(type: ^Global_Type) -> Mutability ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {

	// Creates a new global value.
	//
	// Creates a new host-defined global value within the provided `store`
	//
	// @param store the store in which to create the global
	// @param type the wasm type of the global being created
	// @param val the initial value of the global
	// @param ret a return pointer for the created global.
	//
	// This function may return an error if the `val` argument does not match the
	// specified type of the global, or if `val` comes from a different store than
	// the one provided.
	//
	// This function does not take ownership of any of its arguments but error is
	// owned by the caller.

	global_new :: proc(store: ^Context, type: ^Global_Type, val: ^Val, ret: ^Global) -> ^Error ---


	// Returns the wasm type of the specified global.
	//
	// The returned #wasm_globaltype_t is owned by the caller.

	global_type :: proc(store: ^Context, global: ^Global) -> ^Global_Type ---


	// Get the value of the specified global.
	//
	// @param store the store that owns `global`
	// @param global the global to get
	// @param out where to store the value in this global.
	//
	// This function returns ownership of the contents of `out`, so
	// #wasmtime_val_unroot may need to be called on the value.

	global_get :: proc(store: ^Context, global: ^Global, val: ^Val) -> ^Error ---


	// Sets a global to a new value.
	//
	// @param store the store that owns `global`
	// @param global the global to set
	// @param val the value to store in the global
	//
	// This function may return an error if `global` is not mutable or if `val` has
	// the wrong type for `global`.
	//
	// THis does not take ownership of any argument but returns ownership of the
	// error.

	global_set :: proc(store: ^Context, global: ^Global, val: ^Val) -> ^Error ---
}

////////////////////////////////////////////////////
//
// import
//
//////////////////////////

Import_Type :: struct {}

Import_Type_Vec :: struct {
	size: c.size_t,
	data: [^]^Import_Type,
}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	importtype_new :: proc(module: ^Name, name: ^Name, extern_type: ^Extern_Type) -> ^Import_Type ---
	importtype_delete :: proc(type: ^Import_Type) ---
	importtype_module :: proc(type: ^Import_Type) -> ^Name ---
	importtype_name :: proc(type: ^Import_Type) -> ^Name ---
	importtype_type :: proc(type: ^Import_Type) -> ^Extern_Type ---

	importtype_vec_new_empty :: proc(out: ^Import_Type_Vec) ---
	importtype_vec_new :: proc(vec: ^Import_Type_Vec, size: c.size_t, data: [^]^Import_Type) ---
	importtype_vec_new_uninitialized :: proc(out: ^Import_Type_Vec, size: c.size_t) ---
	importtype_vec_copy :: proc(out: ^Import_Type_Vec, src: ^Import_Type_Vec) ---
	importtype_vec_delete :: proc(vec: ^Import_Type_Vec) ---
}


////////////////////////////////////////////////////
//
// instance
//
//////////////////////////

// Representation of a instance in Wasmtime.
//
// Instances are represented with a 64-bit identifying integer in Wasmtime.
// They do not have any destructor associated with them. Instances cannot
// interoperate between #wasmtime_store_t instances and if the wrong instance
// is passed to the wrong store then it may trigger an assertion to abort the
// process.
Instance :: struct {
	// Internal identifier of what store this belongs to, never zero.
	store_id: u64,
	// Private data for use in Wasmtime.
	private:  c.size_t,
}


// A #wasmtime_instance_t, pre-instantiation, that is ready to be
// instantiated.
//
// Must be deleted using #wasmtime_instance_pre_delete.
//
// For more information see the Rust documentation:
// https://docs.wasmtime.dev/api/wasmtime/struct.InstancePre.html
///
Instance_Pre :: struct {}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// instance_delete :: proc(instance: ^Instance) ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Instantiate a wasm module.
	//
	// This function will instantiate a WebAssembly module with the provided
	// imports, creating a WebAssembly instance. The returned instance can then
	// afterwards be inspected for exports.
	//
	// @param store the store in which to create the instance
	// @param module the module that's being instantiated
	// @param imports the imports provided to the module
	// @param nimports the size of `imports`
	// @param instance where to store the returned instance
	// @param trap where to store the returned trap
	//
	// This function requires that `imports` is the same size as the imports that
	// `module` has. Additionally the `imports` array must be 1:1 lined up with the
	// imports of the `module` specified. This is intended to be relatively low
	// level, and #wasmtime_linker_instantiate is provided for a more ergonomic
	// name-based resolution API.
	//
	// The states of return values from this function are similar to
	// #wasmtime_func_call where an error can be returned meaning something like a
	// link error in this context. A trap can be returned (meaning no error or
	// instance is returned), or an instance can be returned (meaning no error or
	// trap is returned).
	//
	// Note that this function requires that all `imports` specified must be owned
	// by the `store` provided as well.
	//
	// This function does not take ownership of any of its arguments, but all return
	// values are owned by the caller.
	instance_new :: proc(
		store: 		 ^Context,
		module: 	 ^Module,
		imports: 	 ^Extern,
		num_imports: c.size_t,
		instance: 	 ^Instance,
		trap: 		 ^^Trap,
	) -> ^Error ---

	// Get an export by name from an instance.
	//
	// @param store the store that owns `instance`
	// @param instance the instance to lookup within
	// @param name the export name to lookup
	// @param name_len the byte length of `name`
	// @param item where to store the returned value
	//
	// Returns nonzero if the export was found, and `item` is filled in. Otherwise
	// returns 0.
	//
	// Doesn't take ownership of any arguments but does return ownership of the
	// #wasmtime_extern_t.
	instance_export_get :: proc(
		store: 	  ^Context,
		instance: ^Instance,
		name: 	  [^]byte,
		name_len: c.size_t,
		item: 	  ^Extern,
	) -> bool ---

	// Get an export by index from an instance.
	//
	// @param store the store that owns `instance`
	// @param instance the instance to lookup within
	// @param index the index to lookup
	// @param name where to store the name of the export
	// @param name_len where to store the byte length of the name
	// @param item where to store the export itself
	//
	// Returns nonzero if the export was found, and `name`, `name_len`, and `item`
	// are filled in. Otherwise returns 0.
	//
	// Doesn't take ownership of any arguments but does return ownership of the
	// #wasmtime_extern_t. The `name` pointer return value is owned by the `store`
	// and must be immediately used before calling any other APIs on
	// #wasmtime_context_t.
	instance_export_nth :: proc(
		store: ^Context,
		instance: ^Instance,
		index: c.size_t,
		name: [^]byte,
		name_len: c.size_t,
		item: ^Extern,
	) -> bool ---

	// Instantiates instance within the given store.
	//
	// This will also run the function's startup function, if there is one.
	//
	// For more information on instantiation see #wasmtime_instance_new.
	//
	// @param instance_pre the pre-initialized instance
	// @param store the store in which to create the instance
	// @param instance where to store the returned instance
	// @param trap_ptr where to store the returned trap
	//
	// @returns One of three things can happen as a result of this function. First
	// the module could be successfully instantiated and returned through
	// `instance`, meaning the return value and `trap` are both set to `NULL`.
	// Second the start function may trap, meaning the return value and `instance`
	// are set to `NULL` and `trap` describes the trap that happens. Finally
	// instantiation may fail for another reason, in which case an error is returned
	// and `trap` and `instance` are set to `NULL`.
	//
	// This function does not take ownership of any of its arguments, and all return
	// values are owned by the caller.
	instance_pre_instantiate :: proc(
		instance_pre: ^Instance_Pre,
		store: ^Context,
		instance: ^Instance,
		trap: ^^Trap,
	) -> ^Error ---

	// Delete a previously created wasmtime_instance_pre_t.
	instance_pre_delete :: proc(instance_pre: ^Instance_Pre) ---

	// Get the module (as a shallow clone) for a instance_pre.
	//
	// The returned module is owned by the caller and the caller//*must**
	// delete it via `wasmtime_module_delete`.
	instance_pre_module :: proc(instance_pre: ^Instance_Pre) -> ^Module ---
}

////////////////////////////////////////////////////
//
// memory
//
//////////////////////////

// Representation of a memory in Wasmtime.
//
// Memories in Wasmtime are represented as an index into a store and don't
// have any data or destructor associated with the #wasmtime_memory_t value.
// Memories cannot interoperate between #wasmtime_store_t instances and if the
// wrong memory is passed to the wrong store then it may trigger an assertion
// to abort the process.
Memory :: struct {
	store_id: u64,
	private1: u32,
	private2: u32,
}

Memory_Type :: struct {}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	memorytype_delete :: proc(type: ^Memory_Type) ---
	memorytype_limits :: proc(type: ^Memory_Type) -> ^Limits ---

	memory_delete :: proc(memory: ^Memory) ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {

	// Creates a new memory type from the specified parameters.
	//
	// Note that this function is preferred over #wasm_memorytype_new for
	// compatibility with the memory64 proposal.
	memorytype_new :: proc(min: u64, max_present: bool, max: u64, is_64: bool, shared: bool) -> ^Memory_Type ---

	// Returns the minimum size, in pages, of the specified memory type.
	//
	// Note that this function is preferred over #wasm_memorytype_limits for
	// compatibility with the memory64 proposal.
	memorytype_minimum :: proc(ty: ^Memory_Type) -> u64 ---

	// Returns the maximum size, in pages, of the specified memory type.
	//
	// If this memory type doesn't have a maximum size listed then `false` is
	// returned. Otherwise `true` is returned and the `max` pointer is filled in
	// with the specified maximum size, in pages.
	//
	// Note that this function is preferred over #wasm_memorytype_limits for
	// compatibility with the memory64 proposal.
	memorytype_maximum :: proc(ty: ^Memory_Type) -> bool ---

	// Returns whether this type of memory represents a 64-bit memory.
	memorytype_is64 :: proc(ty: ^Memory_Type) -> bool ---


	// Returns whether this type of memory represents a shared memory.
	memorytype_isshared :: proc(ty: ^Memory_Type) -> bool ---

	// Creates a new WebAssembly linear memory
	//
	// @param store the store to create the memory within
	// @param ty the type of the memory to create
	// @param ret where to store the returned memory
	//
	// If an error happens when creating the memory it's returned and owned by the
	// caller. If an error happens then `ret` is not filled in.
	memory_new :: proc(store: ^Context, ty: ^Memory_Type, ret: ^Memory) -> ^Error ---

	// Returns the type of the memory specified
	memory_type :: proc(store: ^Context, memory: ^Memory) -> ^Memory_Type ---

	// Returns the base pointer in memory where the linear memory starts.
	memory_data :: proc(store: ^Context, memory: ^Memory) -> [^]byte ---

	// Returns the byte length of this linear memory.
	memory_data_size :: proc(store: ^Context, memory: ^Memory) -> c.size_t ---

	// Returns the length, in WebAssembly pages, of this linear memory
	memory_size :: proc(store: ^Context, memory: ^Memory) -> u64 ---

	// Attempts to grow the specified memory by `delta` pages.
	//
	// @param store the store that owns `memory`
	// @param memory the memory to grow
	// @param delta the number of pages to grow by
	// @param prev_size where to store the previous size of memory
	//
	// If memory cannot be grown then `prev_size` is left unchanged and an error is
	// returned. Otherwise `prev_size` is set to the previous size of the memory, in
	// WebAssembly pages, and `NULL` is returned.
	memory_grow :: proc(store: ^Context, memory: ^Memory, delta: u64, prev_size: ^u64) -> ^Error ---
}

////////////////////////////////////////////////////
//
// module
//
//////////////////////////


// wasmtime_module_t
// Convenience alias for #wasmtime_module
//
// wasmtime_module
// A compiled Wasmtime module.
//
// This type represents a compiled WebAssembly module. The compiled module is
// ready to be instantiated and can be inspected for imports/exports. It is safe
// to use a module across multiple threads simultaneously.
///
Module :: struct {}

when FEATURE_COMPILER {
	@(link_prefix = "wasmtime_")
	@(default_calling_convention = "c")
	foreign wasmtimelib {

		// Compiles a WebAssembly binary into a #wasmtime_module_t
		//
		// This function will compile a WebAssembly binary into an owned #wasm_module_t.
		// This performs the same as #wasm_module_new except that it returns a
		// #wasmtime_error_t type to get richer error information.
		//
		// On success the returned #wasmtime_error_t is `NULL` and the `ret` pointer is
		// filled in with a #wasm_module_t. On failure the #wasmtime_error_t is
		// non-`NULL` and the `ret` pointer is unmodified.
		//
		// This function does not take ownership of any of its arguments, but the
		// returned error and module are owned by the caller.
		module_new :: proc(
			engine: ^Engine,
			wasm: [^]byte,
			wasm_len: c.size_t,
			ret: ^^Module,
		) -> ^Error ---

		// Validate a WebAssembly binary.
		//
		// This function will validate the provided byte sequence to determine if it is
		// a valid WebAssembly binary within the context of the engine provided.
		//
		// This function does not take ownership of its arguments but the caller is
		// expected to deallocate the returned error if it is non-`NULL`.
		//
		// If the binary validates then `NULL` is returned, otherwise the error returned
		// describes why the binary did not validate.
		module_validate :: proc(engine: ^Engine, wasm: [^]byte, wasm_len: c.size_t) -> ^Error ---

		// This function serializes compiled module artifacts as blob data.
		//
		// @param module the module
		// @param ret if the conversion is successful, this byte vector is filled in
		// with the serialized compiled module.
		//
		// @returns a non-null error if parsing fails, or returns `NULL`. If parsing
		// fails then `ret` isn't touched.
		//
		// This function does not take ownership of `module`, and the caller is
		// expected to deallocate the returned #wasmtime_error_t and #wasm_byte_vec_t.
		module_serialize :: proc(module: ^Module, ret: ^Byte_Vec) -> ^Error ---
	}
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Deletes a module.
	module_delete :: proc(module: ^Module) ---

	// Creates a shallow clone of the specified module, increasing the
	// internal reference count.
	module_clone :: proc(module: ^Module) -> ^Module ---

	// Same as #wasm_module_imports, but for #wasmtime_module_t.
	module_imports :: proc(module: ^Module, out: ^Import_Type_Vec) ---

	// Same as #wasm_module_exports, but for #wasmtime_module_t.
	module_exports :: proc(module: ^Module, out: ^Export_Type_Vec) ---

	// Build a module from serialized data.
	//
	// This function does not take ownership of any of its arguments, but the
	// returned error and module are owned by the caller.
	//
	// This function is not safe to receive arbitrary user input. See the Rust
	// documentation for more information on what inputs are safe to pass in here
	// (e.g. only that of `wasmtime_module_serialize`)
	module_deserialize :: proc(engine: ^Engine, bytes: [^]byte, bytes_len: c.size_t, ret: ^^Module) -> ^Error ---

	// Deserialize a module from an on-disk file.
	//
	// This function is the same as #wasmtime_module_deserialize except that it
	// reads the data for the serialized module from the path on disk. This can be
	// faster than the alternative which may require copying the data around.
	//
	// This function does not take ownership of any of its arguments, but the
	// returned error and module are owned by the caller.
	//
	// This function is not safe to receive arbitrary user input. See the Rust
	// documentation for more information on what inputs are safe to pass in here
	// (e.g. only that of `wasmtime_module_serialize`)
	module_deserialize_file :: proc(engine: ^Engine, path: cstring, ret: ^^Module) -> ^Error ---

	// Returns the range of bytes in memory where this module’s compilation
	// image resides.
	//
	// The compilation image for a module contains executable code, data, debug
	// information, etc. This is roughly the same as the wasmtime_module_serialize
	// but not the exact same.
	//
	// For more details see:
	// https://docs.wasmtime.dev/api/wasmtime/struct.Module.html#method.image_range
	module_image_range :: proc(module: ^Module, start: ^rawptr, end: ^rawptr) ---
}

////////////////////////////////////////////////////
//
// profiling
//
//////////////////////////


// Collects basic profiling data for a single WebAssembly guest.
//
// To use this, you’ll need to arrange to call #wasmtime_guestprofiler_sample at
// regular intervals while the guest is on the stack. The most straightforward
// way to do that is to call it from a callback registered with
// #wasmtime_store_epoch_deadline_callback.
//
// For more information see the Rust documentation at:
// https://docs.wasmtime.dev/api/wasmtime/struct.GuestProfiler.html
///
Guest_Profiler :: struct {}

// wasmtime_guestprofiler_modules_t
// Alias to #wasmtime_guestprofiler_modules
//
// #wasmtime_guestprofiler_modules
// Tuple of name and module for passing into #wasmtime_guestprofiler_new.
Guest_Profiler_Modules :: struct {
	name: ^Name,
	mod:  ^Module,
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Begin profiling a new guest.
	//
	// @param module_name    name recorded in the profile
	// @param interval_nanos intended sampling interval in nanoseconds recorded in
	//                       the profile
	// @param modules        modules and associated names that will appear in
	//                       captured stack traces, pointer to the first element
	// @param modules_len    count of elements in `modules`
	//
	// @returns Created profiler that is owned by the caller.
	//
	// This function does not take ownership of the arguments.
	//
	// For more information see the Rust documentation at:
	// https://docs.wasmtime.dev/api/wasmtime/struct.GuestProfiler.html#method.new
	guestprofiler_new :: proc(
		module_name: ^Name,
		interval_nanos: u64,
		modules: ^Guest_Profiler_Modules,
		modules_len: c.size_t,
	) -> ^Guest_Profiler ---

	// Deletes profiler without finishing it.
	//
	// @param guestprofiler profiler that is being deleted
	guestprofiler_delete :: proc(profiler: ^Guest_Profiler) ---

	// Add a sample to the profile.
	//
	// @param guestprofiler the profiler the sample is being added to
	// @param store         store that is being used to collect the backtraces
	// @param delta_nanos   CPU time in nanoseconds that was used by this guest
	//                      since the previous sample
	//
	// Zero can be passed as `delta_nanos` if recording CPU usage information
	// is not needed.
	// This function does not take ownership of the arguments.
	//
	// For more information see the Rust documentation at:
	// https://docs.wasmtime.dev/api/wasmtime/struct.GuestProfiler.html#method.sample
	guestprofiler_sample :: proc(profiler: ^Guest_Profiler, store: ^Store, delta_nanos: u64) ---

	// Writes out the captured profile.
	//
	// @param guestprofiler the profiler which is being finished and deleted
	// @param out           pointer to where #wasm_byte_vec_t containing generated
	//                      file will be written
	//
	// @returns Returns #wasmtime_error_t owned by the caller in case of error,
	// `NULL` otherwise.
	//
	// This function takes ownership of `guestprofiler`, even when error is
	// returned.
	// Only when returning without error `out` is filled with #wasm_byte_vec_t owned
	// by the caller.
	//
	// For more information see the Rust documentation at:
	// https://docs.wasmtime.dev/api/wasmtime/struct.GuestProfiler.html#method.finish
	guestprofiler_finish :: proc(profiler: ^Guest_Profiler, out: ^Byte_Vec) -> ^Error ---
}

////////////////////////////////////////////////////
//
// shared memory
//
//////////////////////////


// Interface for shared memories.
//
// For more information see the Rust documentation at:
// https://docs.wasmtime.dev/api/wasmtime/struct.SharedMemory.html
///
Shared_Memory :: struct {}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new WebAssembly shared linear memory
	//
	// @param engine engine that created shared memory is associated with
	// @param ty the type of the memory to create
	// @param ret where to store the returned memory
	//
	// If an error happens when creating the memory it's returned and owned by the
	// caller. If an error happens then `ret` is not filled in.
	sharedmemory_new :: proc(engine: ^Engine, ty: ^Memory_Type, ret: ^^Shared_Memory) -> ^Error ---

	// Deletes shared linear memory
	//
	// @param memory memory to be deleted
	sharedmemory_delete :: proc(memory: ^Shared_Memory) ---

	// Clones shared linear memory
	//
	// @param memory memory to be cloned
	//
	// This function makes shallow clone, ie. copy of reference counted
	// memory handle.
	sharedmemory_clone :: proc(memory: ^Shared_Memory) -> ^Shared_Memory ---

	// Returns the type of the shared memory specified
	sharedmemory_type :: proc(memory: ^Shared_Memory) -> ^Memory_Type ---


	// Returns the base pointer in memory where
	// the shared linear memory starts.
	sharedmemory_data :: proc(memory: ^Shared_Memory) -> [^]byte ---

	// Returns the byte length of this shared linear memory.
	sharedmemory_data_size :: proc(memory: ^Shared_Memory) -> c.size_t ---

	// Returns the length, in WebAssembly pages, of this shared linear memory
	sharedmemory_size :: proc(memory: ^Shared_Memory) -> u64 ---

	// Attempts to grow the specified shared memory by `delta` pages.
	//
	// @param memory the memory to grow
	// @param delta the number of pages to grow by
	// @param prev_size where to store the previous size of memory
	//
	// If memory cannot be grown then `prev_size` is left unchanged and an error is
	// returned. Otherwise `prev_size` is set to the previous size of the memory, in
	// WebAssembly pages, and `NULL` is returned.
	sharedmemory_grow :: proc(memory: ^Shared_Memory, delta: u64, prev_size: ^u64) -> ^Error ---
}

////////////////////////////////////////////////////
//
// store
//
//////////////////////////

// wasmtime_store_t
// Convenience alias for #wasmtime_store_t
//
// wasmtime_store
// Storage of WebAssembly objects
//
// A store is the unit of isolation between WebAssembly instances in an
// embedding of Wasmtime. Values in one #wasmtime_store_t cannot flow into
// another #wasmtime_store_t. Stores are cheap to create and cheap to dispose.
// It's expected that one-off stores are common in embeddings.
//
// Objects stored within a #wasmtime_store_t are referenced with integer handles
// rather than interior pointers. This means that most APIs require that the
// store be explicitly passed in, which is done via #wasmtime_context_t. It is
// safe to move a #wasmtime_store_t to any thread at any time. A store generally
// cannot be concurrently used, however.
Store :: struct {}

// wasmtime_context_t
// Convenience alias for #wasmtime_context
//
// wasmtime_context
// An interior pointer into a #wasmtime_store_t which is used as
// "context" for many functions.
//
// This context pointer is used pervasively throughout Wasmtime's API. This can
// be acquired from #wasmtime_store_context or #wasmtime_caller_context. The
// context pointer for a store is the same for the entire lifetime of a store,
// so it can safely be stored adjacent to a #wasmtime_store_t itself.
//
// Usage of a #wasmtime_context_t must not outlive the original
// #wasmtime_store_t. Additionally #wasmtime_context_t can only be used in
// situations where it has explicitly been granted access to doing so. For
// example finalizers cannot use #wasmtime_context_t because they are not given
// access to it.
Context :: struct {}

Update_Deadline_Kind :: enum u8 {
	Continue = 0,
	Yield    = 1,
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new store within the specified engine.
	//
	// @param engine the compilation environment with configuration this store is
	// connected to
	// @param data user-provided data to store, can later be acquired with
	// #wasmtime_context_get_data.
	// @param finalizer an optional finalizer for `data`
	//
	// This function creates a fresh store with the provided configuration settings.
	// The returned store must be deleted with #wasmtime_store_delete.
	store_new :: proc(engine: ^Engine, data: rawptr, finalizer: Finalizer) -> ^Store ---

	// Returns the interior #wasmtime_context_t pointer to this store
	store_context :: proc(store: ^Store) -> ^Context ---

	// Provides limits for a store. Used by hosts to limit resource
	// consumption of instances. Use negative value to keep the default value
	// for the limit.
	//
	// @param store store where the limits should be set.
	// @param memory_size the maximum number of bytes a linear memory can grow to.
	// Growing a linear memory beyond this limit will fail. By default,
	// linear memory will not be limited.
	// @param table_elements the maximum number of elements in a table.
	// Growing a table beyond this limit will fail. By default, table elements
	// will not be limited.
	// @param instances the maximum number of instances that can be created
	// for a Store. Module instantiation will fail if this limit is exceeded.
	// This value defaults to 10,000.
	// @param tables the maximum number of tables that can be created for a Store.
	// Module instantiation will fail if this limit is exceeded. This value
	// defaults to 10,000.
	// @param memories the maximum number of linear memories that can be created
	// for a Store. Instantiation will fail with an error if this limit is exceeded.
	// This value defaults to 10,000.
	//
	// Use any negative value for the parameters that should be kept on
	// the default values.
	//
	// Note that the limits are only used to limit the creation/growth of
	// resources in the future, this does not retroactively attempt to apply
	// limits to the store.
	store_limiter :: proc(
		store: ^Store,
		memory_size: i64,
		table_elements: i64,
		instances: i64,
		tables: i64,
		memories: i64,
	) ---


	// Deletes a store.
	store_delete :: proc(store: ^Store) ---

	// Returns the user-specified data associated with the specified store
	context_get_data :: proc(ctx: ^Context) -> rawptr ---

	// Overwrites the user-specified data associated with this store.
	//
	// Note that this does not execute the original finalizer for the provided data,
	// and the original finalizer will be executed for the provided data when the
	// store is deleted.
	context_set_data :: proc(ctx: ^Context, data: rawptr) ---


	// Perform garbage collection within the given context.
	//
	// Garbage collects `externref`s that are used within this store. Any
	// `externref`s that are discovered to be unreachable by other code or objects
	// will have their finalizers run.
	//
	// The `context` argument must not be NULL.
	context_gc :: proc(ctx: ^Context) ---

	// Set fuel to this context's store for wasm to consume while executing.
	//
	// For this method to work fuel consumption must be enabled via
	// #wasmtime_config_consume_fuel_set. By default a store starts with 0 fuel
	// for wasm to execute with (meaning it will immediately trap).
	// This function must be called for the store to have
	// some fuel to allow WebAssembly to execute.
	//
	// Note that when fuel is entirely consumed it will cause wasm to trap.
	//
	// If fuel is not enabled within this store then an error is returned. If fuel
	// is successfully added then NULL is returned.
	context_set_fuel :: proc(ctx: ^Context, fuel: i64) -> ^Error ---

	// Returns the amount of fuel remaining in this context's store.
	//
	// If fuel consumption is not enabled via #wasmtime_config_consume_fuel_set
	// then this function will return an error. Otherwise `NULL` is returned and the
	// fuel parameter is filled in with fuel consumed so far.
	//
	// Also note that fuel, if enabled, must be originally configured via
	// #wasmtime_context_set_fuel.
	context_get_fuel :: proc(ctx: ^Context, fuel: ^u64) -> ^Error ---

	// Configures the relative deadline at which point WebAssembly code will
	// trap or invoke the callback function.
	//
	// This function configures the store-local epoch deadline after which point
	// WebAssembly code will trap or invoke the callback function.
	//
	// See also #wasmtime_config_epoch_interruption_set and
	// #wasmtime_store_epoch_deadline_callback.
	context_set_epoch_deadline :: proc(ctx: ^Context, ticks_beyond_current: u64) -> ^Error ---

	// Configures epoch deadline callback to C function.
	//
	// This function configures a store-local callback function that will be
	// called when the running WebAssembly function has exceeded its epoch
	// deadline. That function can:
	// - return a #wasmtime_error_t to terminate the function
	// - set the delta argument and return NULL to update the
	//   epoch deadline delta and resume function execution.
	// - set the delta argument, update the epoch deadline,
	//   set update_kind to WASMTIME_UPDATE_DEADLINE_YIELD,
	//   and return NULL to yield (via async support) and
	//   resume function execution.
	//
	// To use WASMTIME_UPDATE_DEADLINE_YIELD async support must be enabled
	// for this store.
	//
	// See also #wasmtime_config_epoch_interruption_set and
	// #wasmtime_context_set_epoch_deadline.

	store_epoch_deadline_callback :: proc(
		store: ^Store,
		func: proc "c" (
			ctx: ^Context,
			data: rawptr,
			epoch_deadline_delta: ^u64,
			update_kind: Update_Deadline_Kind,
		) -> ^Error,
		finalizer: Finalizer,
	) ---
}

////////////////////////////////////////////////////
//
// table
//
//////////////////////////

// Representation of a table in Wasmtime.
//
// Tables in Wasmtime are represented as an index into a store and don't
// have any data or destructor associated with the #wasmtime_table_t value.
// Tables cannot interoperate between #wasmtime_store_t instances and if the
// wrong table is passed to the wrong store then it may trigger an assertion
// to abort the process.
Table :: struct {
	store_id: u64,
	private1: u32,
	private2: u32,
}

Table_Type :: struct {}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	tabletype_new :: proc(type: ^Val_Type, limits: ^Limits) -> ^Table_Type ---
	tabletype_delete :: proc(type: ^Table_Type) ---
	tabletype_element :: proc(type: ^Table_Type) -> ^Val_Type ---
	tabletype_limits :: proc(type: ^Table_Type) -> ^Limits ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new host-defined wasm table.
	//
	// @param store the store to create the table within
	// @param ty the type of the table to create
	// @param init the initial value for this table's elements
	// @param table where to store the returned table
	//
	// This function does not take ownership of any of its parameters, but yields
	// ownership of returned error. This function may return an error if the `init`
	// value does not match `ty`, for example.
	table_new :: proc(store: ^Context, ty: ^Table_Type, init: ^Val, table: ^Table) -> ^Error ---

	// Returns the type of this table.
	//
	// The caller has ownership of the returned #wasm_tabletype_t
	table_type :: proc(store: ^Context, table: ^Table) -> ^Table_Type ---

	// Gets a value in a table.
	//
	// @param store the store that owns `table`
	// @param table the table to access
	// @param index the table index to access
	// @param val where to store the table's value
	//
	// This function will attempt to access a table element. If a nonzero value is
	// returned then `val` is filled in and is owned by the caller. Otherwise zero
	// is returned because the `index` is out-of-bounds.
	table_get :: proc(store: ^Context, table: ^Table, index: u64, val: ^Val) -> bool ---

	// Sets a value in a table.
	//
	// @param store the store that owns `table`
	// @param table the table to write to
	// @param index the table index to write
	// @param value the value to store.
	//
	// This function will store `value` into the specified index in the table. This
	// does not take ownership of any argument but yields ownership of the error.
	// This function can fail if `value` has the wrong type for the table, or if
	// `index` is out of bounds.
	table_set :: proc(store: ^Context, table: ^Table, index: u64, val: ^Val) -> ^Error ---

	// Returns the size, in elements, of the specified table
	table_size :: proc(store: ^Context, table: ^Table) -> u64 ---

	// Grows a table.
	//
	// @param store the store that owns `table`
	// @param table the table to grow
	// @param delta the number of elements to grow the table by
	// @param init the initial value for new table element slots
	// @param prev_size where to store the previous size of the table before growth
	//
	// This function will attempt to grow the table by `delta` table elements. This
	// can fail if `delta` would exceed the maximum size of the table or if `init`
	// is the wrong type for this table. If growth is successful then `NULL` is
	// returned and `prev_size` is filled in with the previous size of the table, in
	// elements, before the growth happened.
	//
	// This function does not take ownership of any of its arguments.
	table_grow :: proc(store: ^Context, table: ^Table, delta: u64, init: ^Val, prev_size: ^u64) -> ^Error ---
}

////////////////////////////////////////////////////
//
// thread local TLS
//
//////////////////////////

TLS_State :: struct {
	tls_ptr:     rawptr,
	stack_limit: c.size_t,
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	tls_save :: proc(store: ^Context, state: ^TLS_State) ---

	tls_restore :: proc(store: ^Context, state: ^TLS_State) ---

	tls_init :: proc() ---
}

////////////////////////////////////////////////////
//
// trap
//
//////////////////////////

// Trap codes for instruction traps.
Trap_Code :: enum u8 {
	// The current stack space was exhausted.
	Stack_Overflow,

	// An out-of-bounds memory access.
	Memory_Out_Of_Bounds,

	// A wasm atomic operation was presented with a not-naturally-aligned
	// linear-memory address.
	Heap_Misaligned,

	// An out-of-bounds access to a table.
	Table_Out_Of_Bounds,

	// Indirect call to a null table entry.
	Indirect_Call_To_Null,

	// Signature mismatch on indirect call.
	Bad_Signature,

	// An integer arithmetic operation caused an overflow.
	Integer_Overflow,

	// An integer division by zero.
	Integer_Division_By_Zero,

	// Failed float-to-int conversion.
	Bad_Conversion_To_Integer,

	// Code that was supposed to have been unreachable was reached.
	Unreachable_Code_Reached,

	// Execution has potentially run too long and may be interrupted.
	Interrupt,

	// When the `component-model` feature is enabled this trap represents a
	// function that was `canon lift`'d, then `canon lower`'d, then called.
	// This combination of creation of a function in the component model
	// generates a function that always traps and, when called, produces this
	// flavor of trap.
	Always_Trap_Adapter,

	// Execution has run out of the configured fuel amount.
	Out_Of_Fuel,

	// Used to indicate that a trap was raised by atomic wait operations on non
	// shared memory.
	Atomic_Wait_Non_Shared_Memory,

	// Call to a null reference.
	Null_Reference,

	// Attempt to access beyond the bounds of an array.
	Array_Out_Of_Bounds,

	// Attempted an allocation that was too large to succeed.
	Allocation_Too_Large,

	// Attempted to cast a reference to a type that it is not an instance of.
	Cast_Failure,

	// When the `component-model` feature is enabled this trap represents a
	// scenario where one component tried to call another component but it
	// would have violated the reentrance rules of the component model,
	// triggering a trap instead.
	Cannot_Enter_Component,

	// Async-lifted export failed to produce a result by calling `task.return`
	// before returning `STATUS_DONE` and/or after all host tasks completed.
	No_Async_Result,

	// A Pulley opcode was executed at runtime when the opcode was disabled at
	// compile time.
	Disabled_Opcode,
}

Trap :: struct {}

Frame :: struct {}

Frame_Vec :: struct {
	size: c.size_t,
	data: [^]^Frame,
}

Message :: Name

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	frame_copy :: proc(frame: ^Frame) -> ^Frame ---
	frame_delete :: proc(frame: ^Frame) ---

	frame_instance :: proc(frame: ^Frame) -> ^Instance ---
	frame_func_index :: proc(frame: ^Frame) -> u32 ---
	frame_func_offset :: proc(frame: ^Frame) -> c.size_t ---
	frame_module_offset :: proc(frame: ^Frame) -> c.size_t ---

	frame_vec_new_empty :: proc(out: ^Frame_Vec) ---
	frame_vec_new :: proc(vec: ^Frame_Vec, size: c.size_t, data: [^]^Frame) ---
	frame_vec_new_uninitialized :: proc(out: ^Frame_Vec, size: c.size_t) ---
	frame_vec_copy :: proc(out: ^Frame_Vec, src: ^Frame_Vec) ---
	frame_vec_delete :: proc(vec: ^Frame_Vec) ---

	trap_delete :: proc(trap: ^Trap) ---
	trap_copy :: proc(trap: ^Trap) -> ^Trap ---
	trap_same :: proc(trap: ^Trap, trap2: ^Trap) -> bool ---
	trap_get_host_info :: proc(trap: ^Trap) -> rawptr ---

	trap_message :: proc(trap: ^Trap, out: ^Message) ---
	trap_origin :: proc(trap: ^Trap) -> ^Frame ---
	trap_trace :: proc(trap: ^Trap, out: ^Frame_Vec) ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new trap with the given message.
	//
	// @param msg the message to associate with this trap
	// @param msg_len the byte length of `msg`
	//
	// The #wasm_trap_t returned is owned by the caller.
	trap_new :: proc(msg: [^]byte, msg_len: c.size_t) -> ^Trap ---

	// Creates a new trap from the given trap code.
	//
	// @param code the trap code to associate with this trap
	//
	// The #wasm_trap_t returned is owned by the caller.
	trap_new_code :: proc(code: Trap_Code) -> ^Trap ---

	// Attempts to extract the trap code from this trap.
	//
	// Returns `true` if the trap is an instruction trap triggered while
	// executing Wasm. If `true` is returned then the trap code is returned
	// through the `code` pointer. If `false` is returned then this is not
	// an instruction trap -- traps can also be created using wasm_trap_new,
	// or occur with WASI modules exiting with a certain exit code.
	trap_code :: proc(trap: ^Trap, code: ^Trap_Code) -> bool ---

	// Returns a human-readable name for this frame's function.
	//
	// This function will attempt to load a human-readable name for function this
	// frame points to. This function may return `NULL`.
	//
	// The lifetime of the returned name is the same as the #wasm_frame_t itself.
	frame_func_name :: proc(frame: ^Frame) -> ^Name ---

	// Returns a human-readable name for this frame's module.
	//
	// This function will attempt to load a human-readable name for module this
	// frame points to. This function may return `NULL`.
	//
	// The lifetime of the returned name is the same as the #wasm_frame_t itself.
	frame_module_name :: proc(frame: ^Frame) -> ^Name ---
}

////////////////////////////////////////////////////
//
// val
//
//////////////////////////


// wasmtime_anyref_t
// Convenience alias for #wasmtime_anyref
//
// wasmtime_anyref
// A WebAssembly value in the `any` hierarchy of GC types.
//
// This structure represents an `anyref` that WebAssembly can create and
// pass back to the host. The host can also create values to pass to a guest.
//
// Note that this structure does not itself contain the data that it refers to.
// Instead to contains metadata to point back within a #wasmtime_context_t, so
// referencing the internal data requires using a `wasmtime_context_t`.
//
// Anyref values are required to be explicitly unrooted via
// #wasmtime_anyref_unroot to enable them to be garbage-collected.
//
// Null anyref values are represented by this structure and can be tested and
// created with the `wasmtime_anyref_is_null` and `wasmtime_anyref_set_null`
// functions.
///
Anyref :: struct {
	// Internal metadata tracking within the store, embedders should not
	// configure or modify these fields.
	store_id: u64,
	// Internal to Wasmtime.
	private1: u32,
	// Internal to Wasmtime.
	private2: u32,
}

// Helper function to initialize the `ref` provided to a null anyref
// value.
anyref_set_null :: #force_inline proc "contextless" (ref: ^Anyref) {
	ref.store_id = 0
}

// Helper function to return whether the provided `ref` points to a null
// `anyref` value.
//
// Note that `ref` itself should not be null as null is represented internally
// within a #wasmtime_anyref_t value.
anyref_is_null :: #force_inline proc "contextless" (ref: ^Anyref) -> bool {
	return ref.store_id == 0
}


// wasmtime_externref_t
// Convenience alias for #wasmtime_externref
//
// wasmtime_externref
// A host-defined un-forgeable reference to pass into WebAssembly.
//
// This structure represents an `externref` that can be passed to WebAssembly.
// It cannot be forged by WebAssembly itself and is guaranteed to have been
// created by the host.
//
// This structure is similar to #wasmtime_anyref_t but represents the
// `externref` type in WebAssembly. This can be created on the host from
// arbitrary host pointers/destructors. Note that this value is itself a
// reference into a #wasmtime_context_t and must be explicitly unrooted to
// enable garbage collection.
//
// Note that null is represented with this structure and created with
// `wasmtime_externref_set_null`. Null can be tested for with the
// `wasmtime_externref_is_null` function.
Extern_Ref :: struct {
	// Internal metadata tracking within the store, embedders should not
	// configure or modify these fields.
	store_id: u64,
	// Internal to Wasmtime.
	private1: u32,
	// Internal to Wasmtime.
	private2: u32,
}

// Helper function to initialize the `ref` provided to a null externref
// value.
externref_set_null :: #force_inline proc "contextless" (ref: ^Extern_Ref) {
	ref.store_id = 0
}

// Helper function to return whether the provided `ref` points to a null
// `externref` value.
//
// Note that `ref` itself should not be null as null is represented internally
// within a #wasmtime_externref_t value.
externref_is_null :: #force_inline proc "contextless" (ref: ^Extern_Ref) -> bool {
	return ref.store_id == 0
}

Val_Kind :: enum u8 {
	I32        = 0,
	I64        = 1,
	F32        = 2,
	F64        = 3,
	V128       = 4,
	FUNC_REF   = 5,
	EXTERN_REF = 6,
	ANY_REF    = 7,
}

// wasmtime_valunion_t
// Convenience alias for #wasmtime_valunion
//
// wasmtime_valunion
// Container for different kinds of wasm values.
//
// This type is contained in #wasmtime_val_t and contains the payload for the
// various kinds of items a value can be.
Val_Union :: struct #raw_union #align(8) {
	i32:       i32,
	i64:       i64,
	f32:       f32,
	f64:       f64,
	anyref:    Anyref,
	externref: Extern_Ref,
	// Field used if #wasmtime_val_t::kind is #WASMTIME_FUNCREF
	//
	// Use `wasmtime_funcref_is_null` to test whether this is a null function
	// reference.
	funcref:   Func,
	v128:      i128,
}

// wasmtime_val_raw_t
// Convenience alias for #wasmtime_val_raw
//
// wasmtime_val_raw
// Container for possible wasm values.
//
// This type is used on conjunction with #wasmtime_func_new_unchecked as well
// as #wasmtime_func_call_unchecked. Instances of this type do not have type
// information associated with them, it's up to the embedder to figure out
// how to interpret the bits contained within, often using some other channel
// to determine the type.
Val_Raw :: struct #raw_union #align (8) {
	// Field for when this val is a WebAssembly `i32` value.
	//
	// Note that this field is always stored in a little-endian format.
	i32:       i32le,

	// Field for when this val is a WebAssembly `i64` value.
	//
	// Note that this field is always stored in a little-endian format.
	i64:       i64le,

	// Field for when this val is a WebAssembly `f32` value.
	//
	// Note that this field is always stored in a little-endian format.
	f32:       f32le,

	// Field for when this val is a WebAssembly `f64` value.
	//
	// Note that this field is always stored in a little-endian format.
	f64:       f64le,

	// Field for when this val is a WebAssembly `v128` value.
	//
	// Note that this field is always stored in a little-endian format.
	v128:      i128le,

	// Field for when this val is a WebAssembly `anyref` value.
	//
	// If this is set to 0 then it's a null anyref, otherwise this must be
	// passed to `wasmtime_anyref_from_raw` to determine the
	// `wasmtime_anyref_t`.
	//
	// Note that this field is always stored in a little-endian format.
	anyref:    u32le,

	// Field for when this val is a WebAssembly `externref` value.
	//
	// If this is set to 0 then it's a null externref, otherwise this must be
	// passed to `wasmtime_externref_from_raw` to determine the
	// `wasmtime_externref_t`.
	//
	// Note that this field is always stored in a little-endian format.
	externref: u32le,

	// Field for when this val is a WebAssembly `funcref` value.
	//
	// If this is set to 0 then it's a null funcref, otherwise this must be
	// passed to `wasmtime_func_from_raw` to determine the `wasmtime_func_t`.
	//
	// Note that this field is always stored in a little-endian format.
	funcref:   rawptr,
}

#assert(size_of(Val_Union) == 16)
#assert(align_of(Val_Union) == 8)
#assert(size_of(Val_Raw) == 16)
#assert(align_of(Val_Raw) == 8)

// Initialize a `wasmtime_func_t` value as a null function reference.
//
// This function will initialize the `func` provided to be a null function
// reference. Used in conjunction with #wasmtime_val_t and
// #wasmtime_valunion_t.
funcref_set_null :: #force_inline proc "contextless" (func: ^Func) {
	func.store_id = 0
}

// Helper function to test whether the `func` provided is a null
// function reference.
//
// This function is used with #wasmtime_val_t and #wasmtime_valunion_t and its
// `funcref` field. This will test whether the field represents a null funcref.
funcref_is_null :: #force_inline proc "contextless" (func: ^Func) -> bool {
	return func.store_id == 0
}

Val_Type :: struct {}

Val_Type_Vec :: struct {
	size: c.size_t,
	data: [^]^Val_Type,
}

// wasmtime_val_t
// Convenience alias for #wasmtime_val_t
//
// wasmtime_val
// Container for different kinds of wasm values.
//
// Note that this structure may contain an owned value, namely rooted GC
// references, depending on the context in which this is used. APIs which
// consume a #wasmtime_val_t do not take ownership, but APIs that return
// #wasmtime_val_t require that #wasmtime_val_unroot is called to clean up
// any possible GC roots in the value.
Val :: struct {
	// Discriminant of which field of #of is valid.
	kind: Val_Kind,
	// Container for the extern item's value.
	of:   Val_Union,
}

Val_Kind_Enum :: enum u8 {
	I32,
	I64,
	F32,
	F64,
	Extern_Ref = 128,
	Func_Ref,
}

valkind_is_num :: proc "c" (k: Val_Kind_Enum) -> bool {
	return k < .Extern_Ref
}

valkind_is_ref :: proc "c" (k: Val_Kind_Enum) -> bool {
	return k >= .Extern_Ref
}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	valtype_new :: proc(kind: Val_Kind_Enum) -> ^Val_Type ---
	valtype_delete :: proc(type: ^Val_Type) ---
	valtype_kind :: proc(type: ^Val_Type) -> Val_Kind_Enum ---

	valtype_vec_new_empty :: proc(out: ^Val_Type_Vec) ---
	valtype_vec_new :: proc(vec: ^Val_Type_Vec, size: c.size_t, data: [^]^Val_Type) ---
	valtype_vec_new_uninitialized :: proc(out: ^Val_Type_Vec, size: c.size_t) ---
	valtype_vec_copy :: proc(out: ^Val_Type_Vec, src: ^Val_Type_Vec) ---
	valtype_vec_delete :: proc(vec: ^Val_Type_Vec) ---
}

@(link_prefix = "wasmtime_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	// Creates a new reference pointing to the same data that `anyref`
	// points to (depending on the configured collector this might increase a
	// reference count or create a new GC root).
	//
	// The returned reference is stored in `out`.
	anyref_clone :: proc(ctx: ^Context, anyref: ^Anyref, out: ^Anyref) ---

	// Unroots the `ref` provided within the `context`.
	//
	// This API is required to enable the `ref` value provided to be
	// garbage-collected. This API itself does not necessarily garbage-collect the
	// value, but it's possible to collect it in the future after this.
	//
	// This may modify `ref` and the contents of `ref` are left in an undefined
	// state after this API is called and it should no longer be used.
	//
	// Note that null or i32 anyref values do not need to be unrooted but are still
	// valid to pass to this function.
	anyref_unroot :: proc(ctx: ^Context, ref: ^Anyref) ---

	// Converts a raw `anyref` value coming from #wasmtime_val_raw_t into
	// a #wasmtime_anyref_t.
	//
	// The provided `out` pointer is filled in with a reference converted from
	// `raw`.
	anyref_from_raw :: proc(ctx: ^Context, raw: u32, out: ^Anyref) ---

	// Converts a #wasmtime_anyref_t to a raw value suitable for storing
	// into a #wasmtime_val_raw_t.
	//
	// Note that the returned underlying value is not tracked by Wasmtime's garbage
	// collector until it enters WebAssembly. This means that a GC may release the
	// context's reference to the raw value, making the raw value invalid within the
	// context of the store. Do not perform a GC between calling this function and
	// passing it to WebAssembly.
	anyref_to_raw :: proc(ctx: ^Context, ref: ^Anyref) -> u32 ---

	// Create a new `i31ref` value.
	//
	// Creates a new `i31ref` value (which is a subtype of `anyref`) and returns a
	// pointer to it.
	//
	// If `i31val` does not fit in 31 bits, it is wrapped.
	anyref_from_i31 :: proc(ctx: ^Context, i31_val: u32, out: ^Anyref) ---

	// Get the `anyref`'s underlying `i31ref` value, zero extended, if any.
	//
	// If the given `anyref` is an instance of `i31ref`, then its value is zero
	// extended to 32 bits, written to `dst`, and `true` is returned.
	//
	// If the given `anyref` is not an instance of `i31ref`, then `false` is
	// returned and `dst` is left unmodified.
	anyref_i31_get_u :: proc(ctx: ^Context, ref: ^Anyref, dst: ^u32) -> bool ---


	// Get the `anyref`'s underlying `i31ref` value, sign extended, if any.
	//
	// If the given `anyref` is an instance of `i31ref`, then its value is sign
	// extended to 32 bits, written to `dst`, and `true` is returned.
	//
	// If the given `anyref` is not an instance of `i31ref`, then `false` is
	// returned and `dst` is left unmodified.
	anyref_i31_get_s :: proc(ctx: ^Context, ref: ^Anyref, dst: ^i32) -> bool ---

	// Create a new `externref` value.
	//
	// Creates a new `externref` value wrapping the provided data, returning whether
	// it was created or not.
	//
	// @param context the store context to allocate this externref within
	// @param data the host-specific data to wrap
	// @param finalizer an optional finalizer for `data`
	// @param out where to store the created value.
	//
	// When the reference is reclaimed, the wrapped data is cleaned up with the
	// provided `finalizer`.
	//
	// If `true` is returned then `out` has been filled in and must be unrooted
	// in the future with #wasmtime_externref_unroot. If `false` is returned then
	// the host wasn't able to create more GC values at this time. Performing a GC
	// may free up enough space to try again.
	externref_new :: proc(ctx: ^Context, data: rawptr, finalizer: Finalizer, out: ^Extern_Ref) -> bool ---

	// Get an `externref`'s wrapped data
	//
	// Returns the original `data` passed to #wasmtime_externref_new. It is required
	// that `data` is not `NULL`.
	externref_data :: proc(ctx: ^Context, ref: ^Extern_Ref) -> rawptr ---

	// Creates a new reference pointing to the same data that `ref` points
	// to (depending on the configured collector this might increase a reference
	// count or create a new GC root).
	//
	// The `out` parameter stores the cloned reference. This reference must
	// eventually be unrooted with #wasmtime_externref_unroot in the future to
	// enable GC'ing it.
	externref_clone :: proc(ctx: ^Context, ref: ^Extern_Ref, out: ^Extern_Ref) ---

	// Unroots the pointer `ref` from the `context` provided.
	//
	// This function will enable future garbage collection of the value pointed to
	// by `ref` once there are no more references. The `ref` value may be mutated in
	// place by this function and its contents are undefined after this function
	// returns. It should not be used until after re-initializing it.
	//
	// Note that null externref values do not need to be unrooted but are still
	// valid to pass to this function.
	externref_unroot :: proc(ctx: ^Context, ref: ^Extern_Ref) ---

	// Converts a raw `externref` value coming from #wasmtime_val_raw_t into
	// a #wasmtime_externref_t.
	//
	// The `out` reference is filled in with the non-raw version of this externref.
	// It must eventually be unrooted with #wasmtime_externref_unroot.
	externref_from_raw :: proc(ctx: ^Context, raw: u32, out: ^Extern_Ref) ---

	// Converts a #wasmtime_externref_t to a raw value suitable for storing
	// into a #wasmtime_val_raw_t.
	//
	// Note that the returned underlying value is not tracked by Wasmtime's garbage
	// collector until it enters WebAssembly. This means that a GC may release the
	// context's reference to the raw value, making the raw value invalid within the
	// context of the store. Do not perform a GC between calling this function and
	// passing it to WebAssembly.
	externref_to_raw :: proc(ctx: ^Context, ref: ^Extern_Ref) -> u32 ---

	// Unroot the value contained by `val`.
	//
	// This function will unroot any GC references that `val` points to, for
	// example if it has the `WASMTIME_EXTERNREF` or `WASMTIME_ANYREF` kinds. This
	// function leaves `val` in an undefined state and it should not be used again
	// without re-initializing.
	//
	// This method does not need to be called for integers, floats, v128, or
	// funcref values.
	val_unroot :: proc(ctx: ^Context, val: ^Val) ---

	// Clones the value pointed to by `src` into the `dst` provided.
	//
	// This function will clone any rooted GC values in `src` and have them
	// newly rooted inside of `dst`. When using this API the `dst` should be
	// later unrooted with #wasmtime_val_unroot if it contains GC values.
	val_clone :: proc(ctx: ^Context, src: ^Val, dst: ^Val) ---
}

@(link_prefix = "wasm_")
@(default_calling_convention = "c")
foreign wasmtimelib {
	byte_vec_new_empty :: proc(out: ^Byte_Vec) ---
	byte_vec_new :: proc(extern: ^Byte_Vec, size: c.size_t, data: [^]byte) ---
	byte_vec_new_uninitialized :: proc(out: ^Byte_Vec, size: c.size_t) ---
	byte_vec_copy :: proc(out: ^Byte_Vec, src: ^Byte_Vec) ---
	byte_vec_delete :: proc(vec: ^Byte_Vec) ---
}

I32_VAL :: #force_inline proc "contextless" (i: i32) -> Val {return Val{kind = .I32, of = {i32 = i}}}
I64_VAL :: #force_inline proc "contextless" (i: i64) -> Val {return Val{kind = .I64, of = {i64 = i}}}
F32_VAL :: #force_inline proc "contextless" (z: f32) -> Val {return Val{kind = .F32, of = {f32 = z}}}
F64_VAL :: #force_inline proc "contextless" (z: f64) -> Val {return Val{kind = .F64, of = {f64 = z}}}
REF_VAL :: #force_inline proc "contextless" (r: Anyref) -> Val {return Val{kind = .ANY_REF, of = {anyref = r}}}
INIT_VAL :: #force_inline proc "contextless" () -> Val {return Val{kind = .ANY_REF, of = {anyref = {}}}}


// 	@(default_calling_convention = "c")
// 	foreign wasmtimelib {
// 		linker_define_wasi :: proc(linker: linker) -> error ---

// 		context_set_wasi :: proc(_context: context, wasi: ^wasi_config_t) -> error ---

// 		wasi_config_delete :: proc(unamed0: ^wasi_config_t) ---

// 		wasi_config_new :: proc() -> ^wasi_config_t ---

// 		wasi_config_set_argv :: proc(config: ^wasi_config_t, argc: i32, argv: ^cstring) ---

// 		wasi_config_inherit_argv :: proc(config: ^wasi_config_t) ---

// 		wasi_config_set_env :: proc(config: ^wasi_config_t, envc: i32, names: ^cstring, values: ^cstring) ---

// 		wasi_config_inherit_env :: proc(config: ^wasi_config_t) ---

// 		wasi_config_set_stdin_file :: proc(config: ^wasi_config_t, path: cstring) -> bool ---

// 		wasi_config_set_stdin_bytes :: proc(config: ^wasi_config_t, binary: byte_vec) ---

// 		wasi_config_inherit_stdin :: proc(config: ^wasi_config_t) ---

// 		wasi_config_set_stdout_file :: proc(config: ^wasi_config_t, path: cstring) -> bool ---

// 		wasi_config_inherit_stdout :: proc(config: ^wasi_config_t) ---

// 		wasi_config_set_stderr_file :: proc(config: ^wasi_config_t, path: cstring) -> bool ---

// 		wasi_config_inherit_stderr :: proc(config: ^wasi_config_t) ---

// 		wasi_config_preopen_dir :: proc(config: ^wasi_config_t, path: cstring, guest_path: cstring) -> bool ---

// 		wasi_config_preopen_socket :: proc(config: ^wasi_config_t, fd_num: u32, host_port: cstring) -> bool ---
// 	}
// }

// Byte vectors

//name :: byte_vec_t
// name_new :: byte_vec_new
// name_new_empty :: byte_vec_new_empty
// name_new_new_uninitialized :: byte_vec_new_uninitialized
// name_copy :: byte_vec_copy
// name_delete :: byte_vec_delete

// name_new_from_string :: #force_inline proc "contextless" (out: ^name_t, s: cstring) {
// 	name_new(out, len(s), s)
// }

// name_new_from_string_nt :: #force_inline proc "contextless" (out: ^name_t, s: cstring) {
// 	name_new(out, len(s) + 1, s)
// }

// valkind_is_num :: #force_inline proc "contextless" (k: valkind_t) -> bool {
// 	return k < valkind_t.ANYREF
// }
// valkind_is_ref :: #force_inline proc "contextless" (k: valkind_t) -> bool {
// 	return k >= valkind_t.ANYREF
// }
// valtype_is_num :: #force_inline proc "contextless" (t: valtype) -> bool {
// 	return valkind_is_num(valtype_kind(t))
// }
// valtype_is_ref :: #force_inline proc "contextless" (t: valtype) -> bool {
// 	return valkind_is_ref(valtype_kind(t))
// }

// // Value Type construction short-hands

// valtype_new_i32 :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.I32)
// }
// valtype_new_i64 :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.I64)
// }
// valtype_new_f32 :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.F32)
// }
// valtype_new_f64 :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.F64)
// }

// valtype_new_anyref :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.ANYREF)
// }
// valtype_new_funcref :: #force_inline proc "contextless" () -> valtype {
// 	return valtype_new(.FUNCREF)
// }

// // Function Types construction short-hands

// functype_new_0_0 :: #force_inline proc "contextless" () -> functype {
// 	params, results: valtype_vec_t
// 	valtype_vec_new_empty(&params)
// 	valtype_vec_new_empty(&results)
// 	return functype_new(&params, &results)
// }

// functype_new_1_0 :: #force_inline proc "contextless" (p: valtype) -> functype {
// 	ps: [1]valtype = {p}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 1, &ps[0])
// 	valtype_vec_new_empty(&results)
// 	return functype_new(&params, &results)
// }

// functype_new_2_0 :: #force_inline proc "contextless" (p1, p2: valtype) -> functype {
// 	ps: [2]valtype = {p1, p2}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 2, &ps[0])
// 	valtype_vec_new_empty(&results)
// 	return functype_new(&params, &results)
// }

// functype_new_3_0 :: #force_inline proc "contextless" (p1, p2, p3: valtype) -> functype {
// 	ps: [3]valtype = {p1, p2, p3}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 3, &ps[0])
// 	valtype_vec_new_empty(&results)
// 	return functype_new(&params, &results)
// }

// functype_new_0_1 :: #force_inline proc "contextless" (r: valtype) -> functype {
// 	rs: [1]valtype = {r}
// 	params, results: valtype_vec_t
// 	valtype_vec_new_empty(&params)
// 	valtype_vec_new(&results, 1, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_1_1 :: #force_inline proc "contextless" (p, r: valtype) -> functype {
// 	ps: [1]valtype = {p}
// 	rs: [1]valtype = {r}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 1, &ps[0])
// 	valtype_vec_new(&results, 1, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_2_1 :: #force_inline proc "contextless" (p1, p2, r: valtype) -> functype {
// 	ps: [2]valtype = {p1, p2}
// 	rs: [1]valtype = {r}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 2, &ps[0])
// 	valtype_vec_new(&results, 1, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_3_1 :: #force_inline proc "contextless" (p1, p2, p3, r: valtype) -> functype {
// 	ps: [3]valtype = {p1, p2, p3}
// 	rs: [1]valtype = {r}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 3, &ps[0])
// 	valtype_vec_new(&results, 1, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_0_2 :: #force_inline proc "contextless" (r1, r2: valtype) -> functype {
// 	rs: [2]valtype = {r1, r2}
// 	params, results: valtype_vec_t
// 	valtype_vec_new_empty(&params)
// 	valtype_vec_new(&results, 2, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_1_2 :: #force_inline proc "contextless" (p, r1, r2: valtype) -> functype {
// 	ps: [1]valtype = {p}
// 	rs: [2]valtype = {r1, r2}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 1, &ps[0])
// 	valtype_vec_new(&results, 2, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_2_2 :: #force_inline proc "contextless" (p1, p2, r1, r2: valtype) -> functype {
// 	ps: [2]valtype = {p1, p2}
// 	rs: [2]valtype = {r1, r2}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 2, &ps[0])
// 	valtype_vec_new(&results, 2, &rs[0])
// 	return functype_new(&params, &results)
// }

// functype_new_3_2 :: #force_inline proc "contextless" (p1, p2, p3, r1, r2: valtype) -> functype {
// 	ps: [3]valtype = {p1, p2, p3}
// 	rs: [2]valtype = {r1, r2}
// 	params, results: valtype_vec_t
// 	valtype_vec_new(&params, 3, &ps[0])
// 	valtype_vec_new(&results, 2, &rs[0])
// 	return functype_new(&params, &results)
// }

// // Value construction short-hands

// val_init_ptr :: #force_inline proc "contextless" (out: val, p: rawptr) {
// 	when size_of(uintptr) == size_of(u64) {
// 		out.kind = .I64
// 		out.of.i64 = i64(uintptr(p))
// 	} else {
// 		out.kind = .I32
// 		out.of.i32 = i32(uintptr(p))
// 	}
// }

// val_ptr :: #force_inline proc "contextless" (val: val) -> rawptr {
// 	when size_of(uintptr) == size_of(u64) {
// 		return rawptr(uintptr(val.of.i64))
// 	} else {
// 		return rawptr(uintptr(val.of.i32))
// 	}
// }


// // Odin utils

// stdin :: 0
// stdout :: 1
// stderr :: 2

// to_string :: #force_inline proc "contextless" (error_message: byte_vec) -> string {
// 	return string(([^]u8)(error_message.data)[:error_message.size])
// }

// get_args :: #force_inline proc "contextless" (pargs: val, nargs: size_t) -> []val_t {
// 	return ([^]val_t)(pargs)[:nargs]
// }

// get_memory_from_caller :: proc(caller: ^Caller, wtctx: ^Context, name: cstring) -> []u8 {
// 	item: Extern
// 	ok := caller_export_get(caller, name, len(name), &item)
// 	assert(ok && item.kind == .Extern)
// 	memory: memory_t = item.of.memory
// 	memory_data_size := memory_data_size(wtctx, &memory)
// 	memory_data := memory_data(wtctx, &memory)
// 	assert(memory_data_size > 0 && memory_data != nil)
// 	return ([^]u8)(memory_data)[:memory_data_size]
// }
