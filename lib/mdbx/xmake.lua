target("mdbx")
    set_kind("static")
    add_includedirs(".", { public = true })
    add_files("mdbx.c")
    set_languages("c11")
    add_cxflags("-O3", "-fPIC")
    if is_plat("windows") then
--         add_cxflags("/bigobj")
        add_syslinks("Advapi32", "User32", "ntdll")
        add_defines("MDBX_ENABLE_MINCORE=0")
    else
        add_defines("MDBX_ENABLE_MINCORE=1")
    end
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
--     add_ldflags("-fPIC")
--
-- target("mdbx_chk")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_chk.c")
--
-- target("mdbx_stat")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_stat.c")

-- target("mdbx_copy")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_copy.c")

-- target("mdbx_drop")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_drop.c")

-- target("mdbx_dump")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_dump.c")

-- target("mdbx_load")
--     set_kind("binary")
--     add_files("mdbx.c", "mdbx_load.c")