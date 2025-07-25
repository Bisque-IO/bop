local nuraft_dir = "../"

function add_nuraft_target(name, src, is_test)
    local target_name = ""
    if is_test then
        target_name = "test-nuraft-" .. name
    else
        target_name = "nuraft-" .. name
    end
    target(target_name)
    set_kind("binary")
    set_languages("c++23")
    add_cxflags("-O3")
    set_optimize("aggressive")
    -- add_toolchains("@llvm")
    -- add_languages("c++20")

    if is_plat("windows") then
        add_defines("_WIN32_WINNT=0x0602")
        -- add_cxflags("/MT")
        add_defines("NOMINMAX")
        add_defines("WIN32_LEAN_AND_MEAN")
        add_syslinks("ntdll", "shell32", "advapi32", "user32", "ws2_32")
        add_cxflags("/Zc:preprocessor", "/experimental:c11atomics")
        -- add_syslinks("onecore", "Synchronization", "msvcrt", "ws2_32")
    end

    add_includedirs(
        nuraft_dir .. "bench",
        nuraft_dir .. "examples",
        nuraft_dir .. "examples/calculator",
        nuraft_dir .. "examples/echo",
        nuraft_dir .. "include",
        nuraft_dir .. "include/libnuraft",
        nuraft_dir .. "test",
        nuraft_dir .. "test/asio",
        nuraft_dir .. "test/unit",
        nuraft_dir
    )
    set_default(true)
    --add_deps("snmalloc")
    --add_deps("nuraft")
    add_files(nuraft_dir .. "*.cxx")
    add_files(src)

    add_defines(
        "ASIO_STANDALONE=1",
        "ASIO_USE_WOLFSSL=1",
        "BOOST_ASIO_USE_WOLFSSL=1",
        "HAVE_WOLFSSL_ASIO=1"
    )

    add_includedirs(os.projectdir() .. "/lib/wolfssl", os.projectdir() .. "/lib/wolfssl/wolfssl", { public = true })

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

    --add_defines("USE_BOOST_ASIO=1")
    add_includedirs("../../asio")

    if is_plat("linux") then
        -- add_defines("ASIO_HAS_IO_URING", "ASIO_DISABLE_EPOLL", "BOOST_ASIO_HAS_IO_URING", "BOOST_ASIO_DISABLE_EPOLL")
        add_packages("libaio", "liburing")
    end

    if not is_plat("windows") and is_arch("x86_64") then
        add_cxflags("-mcx16")
    end

    add_defines(
        "SNMALLOC_ENABLE_WAIT_ON_ADDRESS=1",
        "SNMALLOC_USE_WAIT_ON_ADDRESS=1",
        --         "SNMALLOC_NO_UNIQUE_ADDRESS=1",
        "SNMALLOC_STATIC_LIBRARY=1"
    )
    add_includedirs("../../snmalloc/src")
    add_files("../../snmalloc/src/snmalloc/override/new.cc")

    -- add_defines("USE_BOOST_ASIO")
    -- add_packages("boost")
    -- add_packages("openssl3")
    set_configdir("$(builddir)/$(plat)/$(arch)/$(mode)")
    add_configfiles(nuraft_dir .. "test/cert.pem", { onlycopy = true })
    add_configfiles(nuraft_dir .. "test/key.pem", { onlycopy = true })

    if is_test then
        add_tests("default")
    end
    target_end()
end

add_nuraft_target("calc-server", {
    nuraft_dir .. "examples/calculator/calc_server.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

add_nuraft_target("echo-server", {
    nuraft_dir .. "examples/echo/echo_server.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

add_nuraft_target("quick-start", {
    nuraft_dir .. "examples/quick_start.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

add_nuraft_target("bench", {
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx",
    nuraft_dir .. "bench/raft_bench.cxx"
})

function nuraft_test(name, src)
    add_nuraft_target(name, src, true)
end

nuraft_test("asio-service", {
    nuraft_dir .. "test/asio/asio_service_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("custom-quorum", {
    nuraft_dir .. "test/asio/custom_quorum_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("stream-transport-layer", {
    nuraft_dir .. "test/asio/stream_transport_layer_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("raft-stream-mode", {
    nuraft_dir .. "test/asio/raft_stream_mode_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("req-resp-meta", {
    nuraft_dir .. "test/asio/req_resp_meta_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("buffer", {
    nuraft_dir .. "test/unit/buffer_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("failure", {
    nuraft_dir .. "test/unit/failure_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("leader-election", {
    nuraft_dir .. "test/unit/leader_election_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("learner-new-joiner", {
    nuraft_dir .. "test/unit/learner_new_joiner_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("logger", {
    nuraft_dir .. "test/unit/logger_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("new-joiner", {
    nuraft_dir .. "test/unit/new_joiner_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("raft-server", {
    nuraft_dir .. "test/unit/raft_server_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("serialization", {
    nuraft_dir .. "test/unit/serialization_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("snapshot", {
    nuraft_dir .. "test/unit/snapshot_test.cxx",
    nuraft_dir .. "test/unit/fake_network.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("stat-mgr", {
    nuraft_dir .. "test/unit/stat_mgr_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("strfmt", {
    nuraft_dir .. "test/unit/strfmt_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})

nuraft_test("timer", {
    nuraft_dir .. "test/unit/timer_test.cxx",
    nuraft_dir .. "examples/logger.cc",
    nuraft_dir .. "examples/in_memory_log_store.cxx"
})
