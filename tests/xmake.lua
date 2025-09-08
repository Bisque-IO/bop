local target_of = function(name, use_openssl, src)
    target(name)
    set_kind("binary")
    set_languages("c++23", "c11")
    add_toolchains("@clang")

    if is_plat("windows") then
        -- add_defines("_CRT_SECURE_NO_WARNINGS=1")
        add_defines("NOMINMAX")
        -- add_ldflags("/NODEFAULTLIB:MSVCRTD", {force = true})
        -- add_cxflags("/Zc:preprocessor", {force = true})
        -- add_cxflags("/Zc:preprocessor", "/experimental:c11atomics", {force = true})
        add_syslinks("Advapi32", "User32", "Kernel32", "onecore", "ntdll", "Synchronization", "msvcrt")
        -- set_runtimes("MD")
        -- add_ldflags("/NODEFAULTLIB:MSVCRTD", {force = true})
        -- add_ldflags("/NODEFAULTLIB:msvcprtd", {force = true})
    else
        add_syslinks("c", "m")
        add_cxflags("-Wno-unused-function", "-Wno-unused-variable")
        add_ldflags("-fPIC")
    end

    -- add_cxxflags("clang::-stdlib=libc++")
    --add_syslinks("c++")

    add_defines(
        "UWS_HTTPRESPONSE_NO_WRITEMARK"
    )

    add_includedirs(".")
    add_files(src)

    add_packages("doctest")

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
            add_links("odin/libbop/windows/amd64/wolfssl.lib")
            add_includedirs("../lib/wolfssl", "../lib/wolfssl/wolfssl", { public = true })
        end

        add_defines("_WIN32_WINNT=0x0602")

        add_syslinks("bcrypt", "advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid",
            "Dbghelp")

        add_files("../lib/libuv/src/*.c", "../lib/libuv/src/win/*.c")
        add_includedirs("../lib/libuv/src", { public = false })
        add_includedirs("../lib/libuv/include", { public = true })
    else
        if not use_openssl then
            add_defines("ASIO_USE_WOLFSSL=1")
            add_defines("BOOST_ASIO_USE_WOLFSSL=1")
            add_defines("LIBUS_USE_WOLFSSL")
            -- add_packages("wolfssl")
            add_includedirs("../lib/wolfssl", "../lib/wolfssl/wolfssl", { public = true })

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
                -- add_links("odin/libbop/windows/amd64/wolfssl.lib")
            end
        else
            add_defines("LIBUS_USE_OPENSSL")
            add_packages("openssl3")
        end
        if not use_openssl then
            add_defines("ASIO_USE_WOLFSSL=1")
            add_defines("BOOST_ASIO_USE_WOLFSSL=1")
            add_defines("LIBUS_USE_WOLFSSL")
            -- add_packages("wolfssl")
            add_includedirs("../lib/wolfssl", "../lib/wolfssl/wolfssl", { public = true })

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
                -- add_links("odin/libbop/windows/amd64/wolfssl.lib")
            end
        else
            add_defines("LIBUS_USE_OPENSSL")
            add_packages("openssl3")
        end
    end


    

    if is_plat("linux") then
        -- add_defines("LIBUS_USE_IO_URING")
        -- add_packages("libaio", "liburing")
    end

    -- add_deps("usockets")

    add_files("../lib/usockets/src/**.c")
    add_files("../lib/usockets/src/**.cpp")
    add_includedirs("../lib/usockets/src", { public = false })
    add_includedirs("../lib/usockets/include", { public = true })

    -- bop
    add_includedirs("../lib/src/uws", { public = true })


    if not is_plat("windows") and is_arch("x86_64") then
        add_cxflags("-mcx16")
    end
    add_defines(
        "SNMALLOC_ENABLE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_STATIC_LIBRARY=1"
    )
    add_includedirs("../lib/snmalloc/src")
    if not is_plat("windows") then
        add_files("../lib/snmalloc/src/snmalloc/override/malloc.cc")
    end
    add_files("../lib/snmalloc/src/snmalloc/override/new.cc")
    -- set_symbols("debug")
    -- set_strip("all")

    add_packages("zlib", "zstd")

    target_end()
end

target_of("test-uws", false, "uws_test.cpp")
