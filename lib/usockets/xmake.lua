function target_usockets(kind)
    name = "usockets"
    if kind == "shared" then
        name = name .. "-shared"
    end
    target(name)
        set_kind(kind)
        set_languages("c++23", "c23")
        -- add_toolchains("@llvm")
        add_cxflags("-O3", "-fPIC")
        add_defines("LIBUS_USE_OPENSSL")
        add_files("src/**.c", "src/**.cpp")
        add_includedirs("src", { public = false })
        add_includedirs("include", { public = true })

        -- add_cxxflags("clang::-stdlib=libc++")
        
        if kind == "shared" and is_plat("linux") then
            -- add_cxflags("-static")
            add_shflags("-static-libgcc", "-static-libstdc++", { force = true })
            -- before_link(function(target)
            --     target:add("ldflags", "-static")
            --     print('hi')
            --     table.insert(target:linker():linkflags(), "-static-libgcc")
            --     print(target:linker():linkflags())
            --     print(target:linker():kind())
            --     -- print(target:linker()._TOOL)
            -- end)
        end
        -- add_ldflags("-stdlib=libc++")
        -- add_cxxflags("-stdlib=libc++")

        if is_plat("windows", "mingw") then
            add_defines("LIBUS_USE_UV=1")
            add_packages("libuv")
            --set_languages("c++23", "c23")
            --add_packages("boost")
            add_syslinks("advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid", "dbghelp")
        end

        add_packages("openssl3", "zlib")
    target_end()
end

target_usockets("shared")
target_usockets("static")

function add_example(name, src)
    target("usockets-" .. name)
        set_kind("binary")
        set_languages("c++23", "c11")
        -- add_cxflags("-O3")
        set_default(false)

        --add_deps("usockets")
        add_defines("LIBUS_USE_OPENSSL")
        add_includedirs("src", { public = true })
        add_includedirs("include", { public = true })
        add_files("src/**.c", "src/**.cpp")

        add_packages("openssl3", "zlib")

        add_files(src)
        set_configdir("$(builddir)/$(plat)/$(arch)/$(mode)")
        add_configfiles(
            "misc/certs/invalid_ca_crt.pem",
            "misc/certs/invalid_ca_crt.srl",
            "misc/certs/invalid_client_crt.pem",
            "misc/certs/invalid_client_key.pem",
            "misc/certs/selfsigned_client_crt.pem",
            "misc/certs/selfsigned_client_key.pem",
            "misc/certs/valid_ca_crt.pem",
            "misc/certs/valid_ca_crt.srl",
            "misc/certs/valid_ca_key.pem",
            "misc/certs/valid_client_crt.pem",
            "misc/certs/valid_client_key.pem",
            "misc/certs/valid_server_crt.pem",
            "misc/certs/valid_server_key.pem",
            { onlycopy = true }
        )
    target_end()
end

add_example("echo-server", "examples/echo_server.c")

if not is_plat("windows", "mingw") then
    add_example("hammer-test-unix", "examples/hammer_test_unix.c")
end

add_example("hammer-test", "examples/hammer_test.c")
add_example("http-load-test", "examples/http_load_test.c")
add_example("http-server", "examples/http_server.c")
add_example("peer-verify-test", "examples/peer_verify_test.c")
add_example("tcp-load-test", "examples/tcp_load_test.c")
add_example("tcp-server", "examples/tcp_server.c")
add_example("udp-benchmark", "examples/udp_benchmark.c")