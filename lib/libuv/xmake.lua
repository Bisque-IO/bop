function target_libuv(kind)
    name = "uv"
    if kind == "shared" then
        name = name .. "-shared"
    end
    target(name)
    set_kind(kind)
    set_languages("c++23", "c23")
    add_toolchains("@llvm")
    add_cxflags("-O3", "-fPIC")

    --
    add_files("src/*.c")
    add_includedirs("src", { public = false })
    add_includedirs("include", { public = true })

    -- add_cxxflags("clang::-stdlib=libc++")

    if is_plat("windows", "mingw") then
        add_defines("_CRT_SECURE_NO_WARNINGS=1")
        add_files("src/win/*.c")
        add_syslinks("advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid",
            "dbghelp")
    else
        add_files("src/unix/*.c")
        add_syslinks("pthread")
    end

    target_end()
end

target_libuv("shared")
target_libuv("static")
