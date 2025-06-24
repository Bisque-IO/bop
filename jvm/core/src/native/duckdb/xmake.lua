target("duckdb")
    set_kind("static")
    set_languages("c++20")
    add_includedirs("include", { public = true })
    if is_plat("windows") then
--         add_cxflags("/bigobj")
        add_syslinks("Advapi32", "User32", "ntdll")
    end
    add_files("duckdb.cpp")