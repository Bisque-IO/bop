target("sqlite")
    set_kind("static")
    set_languages("c11")
    add_cxflags("-O3", "-fPIC")
    add_includedirs(".", { public = true })
    -- add_files("sqlite3.c")
    if is_plat("linux") then
        add_cxflags("-fvisibility=hidden")
        add_files("sqlite3_hctree.c")
    else
        add_files("sqlite3.c")
    end

-- target("sqlite3")
--     set_kind("binary")
--     add_includedirs(".")
--     add_files("sqlite3.c", "shell.c")
--     set_languages("c99")