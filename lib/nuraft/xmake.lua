local target_of = function(kind)
    local name = "nuraft"
    if kind == "static" then
        name = name .. "-static"
    end
    target(name)
        set_kind(kind)
        set_languages("c++23")

        add_rules("mode.release")

        --add_toolchains("@zig")
        -- add_cxflags("-target x86_64-linux-musl")
        add_defines("USE_BOOST_ASIO=1")
        add_defines("BOOST_ASIO_DISABLE_STD_ALIGNED_ALLOC")
        add_defines("ASIO_DISABLE_STD_ALIGNED_ALLOC")

        add_cxflags("-fPIC")
        add_cxflags("-O3")
        set_optimize("aggressive")
        add_defines("USE_BOOST_ASIO")
        add_defines("BOOST_ASIO_USE_WOLFSSL=1")
        --add_includedirs("../../src", { public = false })

        if is_plat("linux") then
            -- add_defines("ASIO_HAS_IO_URING", "ASIO_DISABLE_EPOLL", "BOOST_ASIO_HAS_IO_URING", "BOOST_ASIO_DISABLE_EPOLL")
            add_packages("libaio", "liburing")
        end

        add_includedirs("./")
        add_includedirs("include", { public = true })
        add_includedirs("include/libnuraft", { public = true })

        add_files("*.cxx")

        add_deps("snmalloc")

        add_packages(
--             "snmalloc",
            "openssl3",
            "wolfssl",
--             "zlib",
--             "zstd"
            "boost"
            -- "asio"
        )

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
