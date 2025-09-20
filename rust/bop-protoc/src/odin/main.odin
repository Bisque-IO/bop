package bopc

import "core:fmt"
import vmem "core:mem/virtual"
import runtime "base:runtime"

main :: proc() {
    arena: vmem.Arena
    ensure(vmem.arena_init_growing(&arena) == nil)
    allocator := vmem.arena_allocator(&arena)
    defer vmem.arena_destroy(&arena)
    fmt.println("virtual memory Arena Growing allocator")
    context.allocator = allocator

    collection := new(Collection)
    collection^ = Collection{
        name = "model",
        packages = make(map[string]^Package),
        allocator = context.allocator
    }

    add_package(collection, "model")
    find_all_bop_files()


}
