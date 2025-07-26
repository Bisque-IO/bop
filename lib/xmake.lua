local target_of = function(kind, use_openssl)
    if is_plat("windows") then
        if kind == "shared" then
            use_openssl = true
        else
            use_openssl = false
        end
    end
    if kind == "static" then
        if not use_openssl then
            target("bop")
            set_basename("bop")
        else
            target("bop-openssl")
            set_basename("bop-openssl")
        end
    else
        if not use_openssl then
            target("bop-shared")
            set_basename("bop")
        else
            target("bop-shared-openssl")
            set_basename("bop-openssl")
        end
    end
    set_kind(kind)
    set_languages("c++23")

    -- ./configure --host=aarch64-linux-gnu --build=x86_64-linux-gnu CC=aarch64-linux-gnu-gcc CXX=aarch64-linux-gnu-g++ --enable-static --enable-pic --enable-opensslall --enable-opensslextra --enable-asio
    -- ./configure --enable-static --enable-pic --enable-opensslall --enable-opensslextra --enable-aesni --enable-all --enable-all-crypto --enable-asio
    -- ./configure --enable-static --enable-pic --enable-opensslall --enable-opensslextra --enable-aesni --enable-asio

    if is_plat("windows") then
        --add_toolchains("@llvm")
        --add_defines("MDBX_DISABLE_CPU_FEATURES=1")
        --add_defines("MDBX_HAVE_BUILTIN_CPU_SUPPORTS=0")
        --add_defines("LIBMDBX_EXPORTS=1")
    else
        --add_toolchains("@llvm")
    end
    -- set_pcxxheader("include/pch.hpp")

    if is_plat("windows") then
        if kind == "static" then
            add_cxflags("/MT")
        else
            --add_cxflags("/MD")
            --add_syslinks("MSVCRT")
        end
        add_defines("NOMINMAX")
        --add_ldflags("/NODEFAULTLIB:MSVCRT")
        --add_ldflags("/NODEFAULTLIB:ucrt")
        --add_ldflags("/NODEFAULTLIB:libcmt")
        --add_cxflags("/MT")
        --add_ldflags("/MT")
        add_cxflags("/Zc:preprocessor", "/std:c23", "/experimental:c11atomics")
        add_syslinks("Advapi32", "User32", "Kernel32", "onecore", "ntdll", "Synchronization", "msvcrt")
        add_defines("MDBX_ENABLE_MINCORE=0")
    else
        add_syslinks("c", "m")
        add_cxflags("-Wno-unused-function", "-Wno-unused-variable")
        add_cxflags("-fPIC")
        add_defines("MDBX_ENABLE_MINCORE=1")
    end
    if not is_plat("windows") then
        add_defines("MDBX_ENABLE_MADVISE=1")
    end

    if is_plat("linux") then
        -- add_defines("BOOST_ASIO_HAS_IO_URING", "BOOST_ASIO_DISABLE_EPOLL")
        -- add_packages("libaio", "liburing")
    end

    --add_cxxflags("clang::-stdlib=libc++")
    --add_syslinks("c++")

    add_defines(
        "ASIO_STANDALONE=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS",
        -- "USE_BOOST_ASIO",
        "ASIO_DISABLE_STD_ALIGNED_ALLOC",
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

    add_includedirs("src")
    add_includedirs("asio")

    -- nuraft
    add_includedirs("nuraft")
    add_includedirs("nuraft/include", { public = true })
    add_includedirs("nuraft/include/libnuraft", { public = true })
    add_files("nuraft/*.cxx", { languages = "c++23" })

    -- lmdb
    -- add_deps("lmdb")
    -- add_includedirs("../lmdb", { public = true })
    -- add_files("../lmdb/mdb.c", { languages = "c99", includedirs = "../lmdb" })
    -- add_files("../lmdb/midl.c", { languages = "c99", includedirs = "../lmdb" })

    -- mdbx
    add_includedirs("mdbx", { public = true })
    add_files("mdbx/mdbx.c", { languages = "c17", includedirs = "include", cflags = "-O3" })
    --add_files("mdbx/mdbx.c")

    -- sqlite
    add_includedirs("../lib/sqlite", { public = true })
    if is_plat("linux") then
        -- add_deps("sqlite")
        add_files("../lib/sqlite/sqlite3_hctree.c",
            { languages = "c99", includedirs = { "./", "../lib/sqlite" }, cflags = "-O3" })
    else
        add_deps("sqlite")
        -- add_files("../lib/sqlite/sqlite3.c",
        -- { languages = "c99", includedirs = { "./", "../lib/sqlite" }, cflags = "-O3" })
    end

    -- usockets
    if is_plat("windows", "mingw") then
        add_defines("LIBUS_USE_UV=1")

        if use_openssl then
            add_packages("openssl3")
            add_defines("LIBUS_USE_OPENSSL")
        else
            add_defines("LIBUS_USE_WOLFSSL")
            add_defines("BOOST_ASIO_USE_WOLFSSL=1")
            add_defines("ASIO_USE_WOLFSSL=1")
            add_defines("HAVE_WOLFSSL_ASIO=1")
            add_links(os.projectdir() .. "/odin/libbop/windows/amd64/wolfssl.lib")
            add_includedirs("wolfssl", "wolfssl/wolfssl", { public = true })
        end

        add_defines("_WIN32_WINNT=0x0602")

        add_syslinks("bcrypt", "advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid",
            "Dbghelp")

        add_files("libuv/src/*.c", "libuv/src/win/*.c")
        add_includedirs("libuv/src", { public = false })
        add_includedirs("libuv/include", { public = true })
    else
        if not use_openssl then
            add_defines("ASIO_USE_WOLFSSL=1")
            --add_defines("BOOST_ASIO_USE_WOLFSSL=1")
            add_defines("LIBUS_USE_WOLFSSL")
            -- add_packages("wolfssl")
            add_includedirs("wolfssl", "wolfssl/wolfssl", { public = true })

            if is_plat("linux") and is_arch("x86_64") then
                add_links(os.projectdir() .. "/odin/libbop/linux/amd64/libwolfssl.a")
            elseif is_plat("linux") and is_arch("arm64", "aarch64") then
                add_links(os.projectdir() .. "/odin/libbop/linux/arm64/libwolfssl.a")
            elseif is_plat("linux") and is_arch("riscv64") then
                add_links(os.projectdir() .. "/odin/libbop/linux/riscv64/libwolfssl.a")
            elseif is_plat("macosx", "macos", "darwin") and is_arch("x86_64") then
                add_links(os.projectdir() .. "/odin/libbop/macos/amd64/libwolfssl.a")
            elseif is_plat("macosx", "macos", "darwin") and is_arch("arm64", "aarch64") then
                add_links(os.projectdir() .. "/odin/libbop/macos/arm64/libwolfssl.a")
            elseif is_plat("windows", "mingw") and is_arch("x64", "x86_64") then
                add_links(os.projectdir() .. "/odin/libbop/windows/amd64/wolfssl.lib")
            end
        else
            add_defines("LIBUS_USE_OPENSSL")
            add_packages("openssl3")
        end
    end
    add_files("usockets/src/**.c", { languages = "c23", includedirs = "include", cflags = "-O3" })
    add_files("usockets/src/**.cpp", { languages = "c++23", includedirs = "include", cflags = "-O3" })
    add_includedirs("usockets/src", { public = false })
    add_includedirs("usockets/include", { public = true })

    -- bop
    add_includedirs(".", { public = true })
    --add_includedirs("ylt/thirdparty", { public = true })
    add_files("src/**.cpp")

    add_includedirs("llco", { public = true })
    add_files("llco/llco.c", { languages = "c23", includedirs = "include", cflags = "-O3" })

    -- add_includedirs("../lib/uwebsockets/src")

    if not is_plat("windows") and is_arch("x86_64") then
        add_cxflags("-mcx16")
    end
    add_defines(
        "SNMALLOC_ENABLE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS=1",
        --         "SNMALLOC_NO_UNIQUE_ADDRESS=1",
        "SNMALLOC_STATIC_LIBRARY=1"
    )
    add_includedirs("snmalloc/src")
    if not is_plat("windows") then
        add_files("snmalloc/src/snmalloc/override/malloc.cc")
    end
    add_files("snmalloc/src/snmalloc/override/new.cc")

    --add_deps("snmalloc")
    --add_deps("sqlite")

    -- add_packages(
    --     "zlib",
    --     "zstd",
    --     "brotli",
    --     "lz4"
    -- --"boost"
    -- )

    -- set_symbols("debug")
    --set_strip("all")

    --     add_ldflags("-fPIC")
    if kind == "shared" then
        --add_rules("utils.symbols.export_all", { export_classes = true })
        if is_plat("linux") then
            --add_cxflags("-static")
            add_shflags("-static-libgcc", "-static-libstdc++")
        end
    end
    if kind == "static" then
        set_policy("build.merge_archive", true)
    end

    -- on_build(function(target)
    --     local output = target:targetfile()
    --     os.cp(output, path.join("dist", path.filename(output)))
    -- end)
    target_end()
end

target_of("static", false)
target_of("static", true)
target_of("shared", false)
target_of("shared", true)

includes("snmalloc", "sqlite", "nuraft", "usockets", "uwebsockets", "libuv", "mdbx", "scratch", "test")
