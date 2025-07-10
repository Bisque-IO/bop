function target_wamr(kind)
    name = "wamr"
    if kind == "shared" then
        name = name .. "-shared"
    end
    target(name)
    set_kind(kind)
    set_languages("c++23", "c23")
    add_toolchains("@llvm")
    --add_cxflags("-O3", "-fPIC")

    add_includedirs("core", { public = false })
    add_includedirs("core/iwasm/include", { public = true })
    add_includedirs("core/shared", { public = false })
    add_includedirs("core/shared/platform", { public = false })
    add_includedirs("core/shared/platform/include", { public = false })
    add_includedirs("core/shared/utils", { public = false })
    add_includedirs("core/shared/utils/uncommon", { public = false })

    add_files("core/shared/mem-alloc/*.c")
    add_includedirs("core/shared/mem-alloc")

    add_files("core/shared/utils/*.c")
    add_includedirs("core/shared/utils")

    add_files("core/shared/utils/uncommon/*.c")
    add_includedirs("core/shared/utils/uncommon")

    add_files("core/iwasm/aot/aot_intrinsic.c")
    add_files("core/iwasm/aot/aot_loader.c")
    add_files("core/iwasm/aot/aot_runtime.c")
    add_includedirs("core/iwasm/aot")

    add_files("core/iwasm/aot/debug/*.c")
    add_includedirs("core/iwasm/aot/debug")

    add_files("core/iwasm/aot/arch/aot_reloc_x86_64.c")
    add_includedirs("core/iwasm/aot/arch")


    add_files("core/iwasm/common/*.c")
    add_includedirs("core/iwasm/common")

    add_files("core/iwasm/common/gc/*.c")
    add_includedirs("core/iwasm/common/gc")

    add_files("core/iwasm/common/arch/invokeNative_general.c")
    add_includedirs("core/iwasm/common/arch")

    add_files("core/iwasm/interpreter/*.c")
    add_includedirs("core/iwasm/interpreter")

    add_files("core/iwasm/libraries/debug-engine/*.c")
    add_includedirs("core/iwasm/libraries/debug-engine")

    add_files("core/iwasm/libraries/shared-heap/*.c")
    add_includedirs("core/iwasm/libraries/shared-heap")

    add_files("core/iwasm/libraries/thread-mgr/*.c")
    add_includedirs("core/iwasm/libraries/thread-mgr")
    add_includedirs("core/shared/platform/include")


    -- add_cxxflags("clang::-stdlib=libc++")

    if is_plat("linux") then
        add_includedirs("core/shared/platform/linux", { public = true })
        add_files("core/shared/platform/linux/*.c")
        add_includedirs("core/shared/platform/linux")
        add_files("core/shared/platform/common/memory/*.c")
        add_includedirs("core/shared/platform/common/memory")
        add_files("core/shared/platform/common/posix/*.c")
        add_includedirs("core/shared/platform/common/posix")
        add_files("core/shared/platform/common/math/*.c")
        add_includedirs("core/shared/platform/common/math")
        add_files("core/shared/platform/common/libc-util/libc_errno.c")
        add_includedirs("core/shared/platform/common/libc-util")
        add_defines("_POSIX_C_SOURCE=200112L")
        add_defines("WAMR_BUILD_MEMORY_ALLOCATOR=tlsf")
        add_syslinks("c", "dl", "pthread", "thread")
    end

    if is_plat("windows") then
        --set_languages("c++23", "c23")
        --add_packages("boost")
        add_defines("WAMR_BUILD_LINUX_PERF=0")
        add_syslinks("advapi32", "iphlpapi", "psapi", "user32", "userenv", "ws2_32", "shell32", "ole32", "uuid",
            "dbghelp")
        add_files("core/shared/platform/windows/*.c")
        add_includedirs("core/shared/platform/windows")

        add_files("core/shared/platform/common/memory/*.c")
        add_includedirs("core/shared/platform/common/memory")

        add_files("core/shared/platform/common/math/*.c")
        add_includedirs("core/shared/platform/common/math")

        add_includedirs("core/shared/platform/windows", { public = true })
        add_includedirs("core/shared/utils", { public = true })
        add_includedirs("core/shared/utils/uncommon", { public = true })

        add_defines("WAMR_BUILD_MEMORY_ALLOCATOR=tlsf")
    else

    end

    target_end()
end

target_wamr("static")


target("iwasm")
    set_kind("binary")
    -- Global settings
    set_languages("c99")
    add_rules("mode.debug", "mode.release")

    if is_plat("windows") then
        add_defines("WIN32", "_CRT_SECURE_NO_WARNINGS")
    elseif is_plat("linux") or is_plat("macosx") then
        add_defines("WASM_PLATFORM_UNIX")
    end
    set_default(true)
    add_includedirs("core/shared", "core/common", "core/iwasm", "core", "core/base", "core/platform")
    add_files(
        "core/shared/*.c",
        "core/utils/*.c",
        "core/common/*.c",
        "core/base/*.c",
        "core/platform/*.c",
        "core/iwasm/*.c"
    )

    -- Optional: Enable WASI/libc support
    add_files("core/iwasm/libraries/libc-builtin/*.c")
    add_includedirs("core/iwasm/libraries/libc-builtin")

    -- Optional: Add pthread support
    add_files("core/iwasm/libraries/lib-pthread/*.c")
    add_includedirs("core/iwasm/libraries/lib-pthread")

    -- Add extra platform-specific libraries
    if is_plat("linux") then
        add_syslinks("pthread", "dl", "m")
    elseif is_plat("macosx") then
        add_frameworks("CoreFoundation")
    elseif is_plat("windows") then
        add_syslinks("ws2_32", "advapi32", "userenv")
    end