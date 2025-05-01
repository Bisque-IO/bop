target("bop")
    set_kind("shared")
--     set_toolchains("@llvm")
    add_includedirs(".", { public = true })
    add_includedirs("lmdb", { public = true })
    add_includedirs("mdbx", { public = true })
    add_includedirs("sqlite", { public = true })
    add_includedirs("snmalloc/src", { public = true })
--     add_includedirs("duckdb/include", { public = true })
--     add_files("lmdb/mdb.c", "lmdb/midl.c")
    add_files("lmdb/mdb.c", "lmdb/midl.c", { languages = "c23", includedirs = "lmdb" })
    add_files("mdbx/mdbx.c", { language = "c23", includedirs = "mdbx" })
    add_files("sqlite/sqlite3.c", { language = "c23", includedirs = "sqlite" })
--     add_files("duckdb/duckdb.cpp", { language = "c++20", includedirs = "duckdb/include" })
    add_files("*.cpp")
    set_languages("c++20", "c23")
--     set_languages("c++23")
--     set_languages("c23")
    add_cxflags("-O3")
    add_cxflags("-fPIC")
--     add_deps("mdbx", "sqlite", "snmalloc")
    if is_plat("windows", "mingw") then
--         add_cxflags("/wd4244 /wd4267 /wd4200 /wd26451 /wd26495 /D_CRT_SECURE_NO_WARNINGS /utf-8")
--         add_cxflags("/D_CRT_SECURE_NO_WARNINGS /utf-8")
--         add_cxflags("/bigobj")
        add_syslinks("WindowsApp", "Synchronization", "bcrypt", "Advapi32", "User32", "Kernel32", "ntdll", "ws2_32")
        add_defines("MDBX_ENABLE_MINCORE=0")
--         add_defines("_WIN32_WINNT=0x0602")

        if is_arch("x64", "x86_64") then
            set_basename("bop-x86_64-windows")
        end
    elseif is_plat("linux") then
        set_basename("bop-$(arch)-$(plat)")
        add_defines("MDBX_ENABLE_MINCORE=1")
    elseif is_plat("macosx") then
        set_basename("bop-$(arch)-macos")
        add_defines("MDBX_ENABLE_MINCORE=1")
    end

    if is_plat("linux") then
        add_cxflags("-mcx16")
    end
    add_defines(
        "SNMALLOC_ENABLE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS=1"
    )
    add_defines(
        "MDBX_PNL_ASCENDING=1",
        "MDBX_ENABLE_BIGFOOT=1",
        "MDBX_ENABLE_PREFAULT=1",
        "MDBX_ENABLE_MADVISE=1",
        "MDBX_ENABLE_PGOP_STAT=1",
        "MDBX_TXN_CHECKOWNER=0",
        "MDBX_DEBUG=0",
        "NDEBUG=1"
    )
    set_symbols("debug")
    set_strip("debug")
--     add_cxflags("-stdlib=libc++")
--     add_syslinks("c++")
--     add_cxflags("-rdynamic")
--     add_ldflags("-rdynamic")

    add_rules("utils.symbols.export_all", {export_classes = false})
--     set_strip("all")

-- includes("mdbx", "sqlite", "snmalloc")