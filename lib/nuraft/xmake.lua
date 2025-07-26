--[[
#ifdef ASIO_USE_WOLFSSL
#include "wolfssl/options.h"
#include "wolfssl/ssl.h"
#else
typedef struct ssl_ctx_st SSL_CTX;
#endif
]]

local target_of = function(kind)
    local name = "nuraft"
    if kind == "static" then
        name = name .. "-static"
    end
    target(name)
    set_kind(kind)
    set_languages("c++23")

    add_rules("mode.release")

    add_defines("BOOST_ASIO_DISABLE_STD_ALIGNED_ALLOC")
    add_defines("ASIO_DISABLE_STD_ALIGNED_ALLOC")

    add_cxflags("-fPIC")
    add_cxflags("-O3")
    set_optimize("aggressive")

    if is_plat("windows") then
        if kind == "static" then
            add_cxflags("/MT")
        else
            --add_cxflags("/MD")
            --add_syslinks("MSVCRT")
        end
        add_defines("_WIN32_WINNT=0x0602")
        add_defines("NOMINMAX")
        add_cxflags("/Zc:preprocessor", "/std:c23", "/experimental:c11atomics")
        add_syslinks("Advapi32", "User32", "Kernel32", "onecore", "ntdll", "Synchronization", "msvcrt")
    end

    --add_defines("USE_BOOST_ASIO")
    --add_defines("BOOST_ASIO_USE_WOLFSSL=0")
    --add_includedirs("../../src", { public = false })
    -- add_defines("ASIO_USE_WOLFSSL=1")
    --add_defines("BOOST_ASIO_USE_WOLFSSL=1")
    --
    -- add_packages("wolfssl")
    add_defines("ASIO_USE_WOLFSSL=1")
    add_defines("HAVE_WOLFSSL_ASIO=1")
    add_includedirs(os.projectdir() .. "/lib/wolfssl", os.projectdir() .. "/lib/wolfssl/wolfssl", { public = true })

    if is_plat("linux") and is_arch("x86_64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/linux/amd64/libwolfssl.a")
    elseif is_plat("linux") and is_arch("arm64", "aarch64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/linux/arm64/libwolfssl.a")
    elseif is_plat("linux") and is_arch("riscv64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/linux/riscv64/libwolfssl.a")
    elseif is_plat("macosx", "macos", "darwin") and is_arch("x86_64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/macos/amd64/libwolfssl.a")
    elseif is_plat("macosx", "macos", "darwin") and is_arch("arm64", "aarch64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/macos/arm64/libwolfssl.a")
    elseif is_plat("windows", "mingw") and is_arch("x86_64") then
        add_ldflags("-l:" .. os.projectdir() .. "/odin/libbop/windows/amd64/libwolfssl.a")
    end

    add_includedirs("../asio")

    if is_plat("linux") then
        -- add_defines("ASIO_HAS_IO_URING", "ASIO_DISABLE_EPOLL", "BOOST_ASIO_HAS_IO_URING", "BOOST_ASIO_DISABLE_EPOLL")
        add_packages("libaio", "liburing")
    end

    add_includedirs("./")
    add_includedirs("include", { public = true })
    add_includedirs("include/libnuraft", { public = true })

    add_files("*.cxx")

    add_deps("snmalloc")
    if not is_plat("windows") and is_arch("x86_64") then
        add_cxflags("-mcx16")
    end

    -- add_packages(
    --     "openssl3"
    -- )

    if is_plat("macosx") then
        --             add_cxxflags("clang::-std=c++23")
        --             add_cxxflags("clang::-stdlib=libc++")
    end
    -- set_policy("build.merge_archive", true)
    target_end()
end

target_of("static")
target_of("shared")


includes("test")

-- nuraft_test("new-joiner", {
--     nuraft_dir .. "test/new_joiner_test.cxx",
--     nuraft_dir .. "examples/logger.cc",
--     nuraft_dir .. "examples/in_memory_log_store.cxx"
-- })
