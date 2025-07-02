local target_of = function(kind)
    if kind == "static" then
        target("bop-static")
    else
        target("bop")
    end
    set_kind(kind)
    set_languages("c++23", "c23")

    if is_plat("windows") then
        -- add_toolchains("@llvm")
        -- add_defines("MDBX_DISABLE_CPU_FEATURES=1")
    end
    --add_toolchains("@llvm")
    -- set_pcxxheader("include/pch.hpp")

    if is_plat("windows") then
        add_cxflags("/Zc:preprocessor")
        add_syslinks("Advapi32", "User32", "Kernel32", "onecore", "ntdll", "Synchronization")
        add_defines("MDBX_ENABLE_MINCORE=0")
    else
        add_cxflags("-Wno-unused-function", "-Wno-unused-variable")
        add_cxflags("-fPIC")
        add_defines("MDBX_ENABLE_MINCORE=1")
    end
    if is_plat("linux") and is_arch("x86_64") then
        add_cxflags("-mcx16")
        add_defines("MDBX_ENABLE_MADVISE=1")
    end

    -- add_cxflags("-fPIC")
    if is_plat("linux") then
        -- add_defines("BOOST_ASIO_HAS_IO_URING", "BOOST_ASIO_DISABLE_EPOLL")
        -- add_packages("libaio", "liburing")
    end

    add_cxxflags("clang::-stdlib=libc++")

    add_defines(
    -- "BOOST_ASIO_USE_WOLFSSL=1",
    -- "ASIO_STANDALONE=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS",
        "USE_BOOST_ASIO",
        "BOOST_ASIO_DISABLE_STD_ALIGNED_ALLOC",
        "BOOST_BEAST_USE_STD_STRING_VIEW",
        -- "BOOST_ASIO_NO_DEPRECATED=1",
        -- "MDBX_PNL_ASCENDING=0",
        "MDBX_ENABLE_BIGFOOT=1",
        "MDBX_ENABLE_PREFAULT=1",
        "MDBX_ENABLE_PGOP_STAT=1",
        "MDBX_TXN_CHECKOWNER=0",
        "MDBX_DEBUG=0",
        "NDEBUG=1"
    )

    add_defines("ASIO_DISABLE_STD_ALIGNED_ALLOC")
    add_defines("YLT_ENABLE_SSL")
    add_defines("CINATRA_ENABLE_SSL")
    add_defines("CINATRA_ENABLE_GZIP")
    add_defines("CINATRA_ENABLE_BROTLI")

    if kind == "static" then
        add_defines("BOP_STATIC_BUILD=1")
    else
        add_defines("BOP_DYNAMIC_BUILD=1")
    end

    -- nuraft
    add_includedirs("nuraft")
    add_includedirs("nuraft/include", { public = true })
    add_includedirs("nuraft/include/libnuraft", { public = true })
    add_files("nuraft/*.cxx")

    -- lmdb
    -- add_deps("lmdb")
    -- add_includedirs("../lmdb", { public = true })
    -- add_files("../lmdb/mdb.c", { languages = "c99", includedirs = "../lmdb" })
    -- add_files("../lmdb/midl.c", { languages = "c99", includedirs = "../lmdb" })

    -- mdbx
    -- add_includedirs("mdbx", { public = true })
    -- add_files("mdbx/mdbx.c", { languages = "c23", includedirs = "include", cflags = "-O3" })

    -- sqlite
    -- add_includedirs("../lib/sqlite", { public = true })
    -- add_files("../lib/sqlite/sqlite3.c", { languages = "c99", includedirs = { "./", "../lib/sqlite" }, cflags = "-O3" })

    -- bop
    add_includedirs(".", { public = true })
    --add_includedirs("ylt/thirdparty", { public = true })
    add_files("src/**.cpp")


    add_includedirs("llco", { public = true })
    add_files("llco/llco.c", { languages = "c23", includedirs = "include", cflags = "-O3" })

    -- add_includedirs("../lib/uwebsockets/src")

    --add_deps("mdbx")
    add_deps("snmalloc")
    -- add_deps("usockets")
    --     add_deps("usockets", "uws")
    --add_deps("nuraft-static")
    add_deps("sqlite")

    add_packages(
    --         "snmalloc",
    --"mimalloc",
    -- "wolfssl",
        "openssl3",
        "fmt",
        -- "spdlog",
        -- "stringzilla",
        "zlib",
        "zstd",
        "brotli",
        "lz4",
        --"snappy",=
        --"xxhash",
        -- "onnxruntime",
        "boost"
    --             "minio-cpp",
    --         "wamr",
    -- "duckdb",
    --         "aws-c-s3",
    --             "ftxui",
    -- "cli11"
    --             "libcurl"
    )

    -- set_symbols("debug")
    --set_strip("all")

    --     add_ldflags("-fPIC")
    if kind == "shared" then
        add_rules("utils.symbols.export_all", { export_classes = true })
        if is_plat("linux") then
            --add_cxflags("-static")
            add_shflags("-static-libgcc", "-static-libstdc++")
        end
    end
    if kind == "static" then
        set_policy("build.merge_archive", true)
        -- add_cxflags("-static")
        -- add_shflags("-static-libgcc", "-static-libstdc++")
    end
    -- set_policy("build.merge_archive", true)

    -- on_build(function(target)
    --     local output = target:targetfile()
    --     os.cp(output, path.join("dist", path.filename(output)))
    -- end)
    target_end()
end

target_of("static")
target_of("shared")

includes("snmalloc", "sqlite", "nuraft", "usockets", "uwebsockets")
