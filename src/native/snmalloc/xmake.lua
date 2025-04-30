target("snmalloc")
    set_kind("static")
    set_languages("c++23")
    add_cxflags("-O3")
    set_optimize("aggressive")
    if is_plat("windows", "mingw") then
        -- add_syslinks("Synchronization", "WindowsApp")
        -- add_syslinks("Synchronization", "Kernel32")
        add_syslinks("Synchronization")
    else
        add_files("src/snmalloc/override/malloc.cc")
        add_files("src/snmalloc/override/new.cc")
        add_cxflags("-fPIC")
    end

    if is_plat("linux") and is_arch("x86_64") then
        add_cxflags("-mcx16")
    end

    add_defines(
        "SNMALLOC_ENABLE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS=1",
--         "SNMALLOC_NO_UNIQUE_ADDRESS=1",
        "SNMALLOC_STATIC_LIBRARY=1"
    )
    add_includedirs("src", { public = true })
