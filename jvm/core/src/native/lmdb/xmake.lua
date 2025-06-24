-- target("mdb-stat")
--     set_kind("binary")
--     add_files("mdb.c", "midl.c", "mdb_stat.c")

target("lmdb")
    set_kind("shared")
    set_languages("c++23")
    add_includedirs(".", { public = true })
    if is_plat("windows") then
--         add_cxflags("/bigobj")
        add_syslinks("Advapi32", "User32", "ntdll")
    end

    add_files("mdb.c", "midl.c")
