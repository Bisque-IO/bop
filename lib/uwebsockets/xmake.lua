target("uws")
    set_kind("headeronly")
    set_languages("c++23", "c11")

    add_includedirs("src", { public = false })
    add_includedirs("include", { public = true })
    add_includedirs("../usockets/include", { public = true })

    -- add_packages("openssl3", "zlib")

    -- add_files("../usockets/src/**.c", "../usockets/src/**.cpp")

    -- if is_plat("windows", "mingw") then
    --     add_defines("LIBUS_USE_UV=1")
    --     add_packages("libuv")
    --     add_syslinks("advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid", "dbghelp")
    -- end
target_end()

function add_example(name, src)
    target("uws-" .. name)
        set_kind("binary")
        set_languages("c++23")
        set_default(false)
        add_defines("LIBUS_USE_OPENSSL")
        add_files(src)
        add_deps("usockets", "uws")
        add_packages("openssl3", "zlib")
    target_end()
end

add_example("broadcast", "examples/Broadcast.cpp")
add_example("broadcasting-echo-server", "examples/BroadcastingEchoServer.cpp")
add_example("caching-app", "examples/CachingApp.cpp")
add_example("client", "examples/Client.cpp")
add_example("crc32", "examples/Crc32.cpp")
add_example("echo-body", "examples/EchoBody.cpp")
add_example("echo-server", "examples/EchoServer.cpp")
add_example("echo-server-threaded", "examples/EchoServerThreaded.cpp")
add_example("hello-world", "examples/HelloWorld.cpp")
add_example("hello-world-threaded", "examples/HelloWorldThreaded.cpp")
add_example("http-server", "examples/HttpServer.cpp")
add_example("parameter-routes", "examples/ParameterRoutes.cpp")
add_example("server-name", "examples/ServerName.cpp")
add_example("smoke-test", "examples/SmokeTest.cpp")
add_example("upgrade-async", "examples/UpgradeAsync.cpp")
add_example("upgrade-sync", "examples/UpgradeSync.cpp")


function add_test(name, src)
    target("test-uws-" .. name)
        set_kind("binary")
        set_languages("c++23")
        add_defines("LIBUS_USE_OPENSSL")
        set_default(false)
        add_files(src)
        add_deps("usockets", "uws")
        add_packages("openssl3", "zlib")
    target_end()
end

add_test("bloom-filter", "tests/BloomFilter.cpp")
add_test("chunked-encoding", "tests/ChunkedEncoding.cpp")
add_test("extensions-negotiator", "tests/ExtensionsNegotiator.cpp")
add_test("http-parser", "tests/HttpParser.cpp")
add_test("http-router", "tests/HttpRouter.cpp")
add_test("query", "tests/Query.cpp")
add_test("topic-tree", "tests/TopicTree.cpp")


function add_benchmark(name, src)
    target("uws-benchmark-" .. name)
        set_kind("binary")
        set_languages("c++23", "c11")
        add_defines("LIBUS_USE_OPENSSL")
        set_default(false)
        add_cxflags("-O3")
        add_files(src)
        add_deps("usockets", "uws")
        add_packages("openssl3", "zlib")
    target_end()
end

add_benchmark("broadcast-test", "benchmarks/broadcast_test.c")
add_benchmark("load-test", "benchmarks/load_test.c")
add_benchmark("parser", "benchmarks/parser.cpp")
add_benchmark("scale-test", "benchmarks/scale_test.c")