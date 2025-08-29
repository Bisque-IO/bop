-- windows install conan: scoop install conan
-- make sure conan is pointing to correct URL
-- conan remote update conancenter --url https://center2.conan.io

set_project("bop")

set_description("data platform for modern computing")

set_languages("c++23")
--set_warnings("all")
add_rules("mode.debug", "mode.release")

-- -- Linux ARM64
-- toolchain("linux_arm64")
-- set_kind("standalone")
-- set_toolset("cc", "aarch64-linux-gnu-gcc")
-- set_toolset("cxx", "aarch64-linux-gnu-g++")
-- set_toolset("ld", "aarch64-linux-gnu-ld")
-- toolchain_end()

-- -- Linux RISCV64
-- toolchain("linux_riscv64")
-- set_kind("standalone")
-- set_toolset("cc", "riscv64-linux-gnu-gcc")
-- set_toolset("cxx", "riscv64-linux-gnu-g++")
-- toolchain_end()

-- -- Windows ARM64 (MinGW)
-- toolchain("mingw_arm64")
-- set_kind("standalone")
-- set_toolset("cc", "aarch64-w64-mingw32-gcc")
-- set_toolset("cxx", "aarch64-w64-mingw32-g++")
-- toolchain_end()

-- package("wamr2")
-- add_deps("cmake")
-- set_sourcedir(path.join(os.scriptdir(), "lib/wamr"))
-- on_install(function(package)
--     local configs = {}
--     table.insert(configs, "-DCMAKE_BUILD_TYPE=" .. (package:debug() and "Debug" or "Release"))
--     table.insert(configs, "-DBUILD_SHARED_LIBS=" .. (package:config("shared") and "ON" or "OFF"))
--     import("package.tools.cmake").install(package, configs)
-- end)
-- on_test(function(package)
--     assert(package:has_cfuncs("wasm_engine_new", { includes = "wasm_c_api.h" }))
-- end)
-- package_end()

-- build WAMR from its CMake project
-- package("wamr2")

-- add_configs("interp", { description = "Enable interpreter", default = true, type = "boolean" })
-- add_configs("fast_interp", { description = "Enable fast interpreter", default = false, type = "boolean" })
-- add_configs("aot", { description = "Enable AOT", default = false, type = "boolean" })
-- -- TODO: improve llvm
-- add_configs("jit", { description = "Enable JIT", default = false, type = "boolean", readonly = true })
-- add_configs("fast_jit", { description = "Enable Fast JIT", default = false, type = "boolean", readonly = true })
-- add_configs("libc",
--     { description = "Choose libc", default = "builtin", type = "string", values = { "builtin", "wasi", "uvwasi" } })
-- add_configs("libc_builtin", { description = "Enable builtin libc", default = false, type = "boolean" })
-- add_configs("libc_wasi", { description = "Enable wasi libc", default = false, type = "boolean" })
-- add_configs("libc_uvwasi", { description = "Enable uvwasi libc", default = false, type = "boolean" })
-- add_configs("multi_module", { description = "Enable multiple modules", default = false, type = "boolean" })
-- add_configs("mini_loader", { description = "Enable wasm mini loader", default = false, type = "boolean" })
-- add_configs("wasi_threads", { description = "Enable wasi threads library", default = false, type = "boolean" })
-- add_configs("simd", { description = "Enable SIMD", default = false, type = "boolean" })
-- add_configs("ref_types", { description = "Enable reference types", default = false, type = "boolean" })

-- if is_plat("windows", "mingw") then
--     add_syslinks("ntdll", "ws2_32")
-- elseif is_plat("linux", "bsd") then
--     add_syslinks("m", "dl", "pthread")
-- elseif is_plat("android") then
--     add_syslinks("log", "android")
-- end

-- add_deps("cmake")

-- add_requires("conan::zstd/1.5.7", {
--     alias = "zstd",
--     configs = {
--         shared = false,
--         fPIC = true
--     }
-- })

add_requires("zstd ~1.5.7", {
    alias = "zstd",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("zlib ~1.3.1", {
    alias = "zlib",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("brotli ~1.1.0", {
    alias = "brotli",
    configs = {
        shared = false,
        fPIC = true
    }
})

-- add_requires("conan::zlib/1.3.1", {
--     alias = "zlib",
--     configs = {
--         shared = false,
--         fPIC = true
--     }
-- })

-- add_requires("conan::lz4/1.10.0", {
--     alias = "lz4",
--     configs = {
--         shared = false,
--         fPIC = true
--     }
-- })

-- add_requires("conan::brotli/1.1.0", {
--     alias = "brotli",
--     configs = {
--         shared = false,
--         fPIC = true
--     }
-- })

if is_plat("linux") then
    add_requires("conan::libaio/0.3.113", { alias = "libaio", configs = { shared = false } })
    add_requires("conan::liburing/2.11", { alias = "liburing", configs = { shared = false } })
end

add_requires("zig ~0.14.0")

-- if is_plat("windows") then
--     add_requires("openssl3 ~3.3.2", {
--         -- add_requires("conan::openssl/3.5.0", {
--         alias = "openssl3",
--         configs = {
--             fPIC = true,
--             shared = true
--         }
--     })
-- else
--     add_requires("conan::openssl/3.5.0", {
--         alias = "openssl3",
--         configs = {
--             fPIC = true,
--             shared = true
--         }
--     })
-- end

add_requires("openssl3 ~3.3.2", {
    -- add_requires("conan::openssl/3.5.0", {
    alias = "openssl3",
    configs = {
        fPIC = true,
        fpic = true,
        shared = true
    }
})

add_requires("c-ares ~1.34.3", {
    configs = {
        fPIC = true,
        fpic = true,
        shared = false,
    }
})

add_requires("asio ~1.34.2")

add_requires("conan::boost/1.88.0", {
-- add_requires("boost ~1.88.0", {
    alias = "boost",
    configs = {
        shared = false,
        charconv = false,
        chrono = false,
        cobalt = false,
        container = false,
        context = false,
        contract = false,
        coroutine = false,
        date_time = false,
        exception = false,
        fiber = false,
        filesystem = true,
        graph = false,
        graph_parallel = false,
        iostreams = false,
        json = false,
        locale = false,
        log = false,
        math = false,
        mpi = false,
        nowide = false,
        process = false,
        program_options = false,
        python = false,
        random = false,
        regex = false,
        serialization = false,
        stacktrace = false,
        system = true,
        test = false,
        thread = false,
        timer = false,
        type_erasure = false,
        url = false,
        wave = false,
        header_only = true,
        filesystem_use_std_fs = true,
        zlib = true,
        bzip2 = false,
        lzma = false,
        zstd = true,
        segmented_stacks = false,
        debug_level = 0,
        pch = false,
        i18n_backend = "none",
        fPIC = true,
        i18n_backend_iconv = "off",
        i18n_backend_icu = false,
        multithreading = false,
    }
})

add_requires("wolfssl ~5.7.2", {
    --add_requires("conan::wolfssl/5.7.0", {
    alias = "wolfssl1",
    configs = {
        shared = false,
        asio = true,
        quic = true,
        fPIC = true,
        sslv3 = true,
        certgen = true,
        dsa = true,
        ripemd = true,
        sessioncerts = true,
        testcert = true,
        openssl_all = true,
        openssl_extra = true,
        opensslall = true,
        opensslextra = true,
        tls13 = true,
        sni = true,
        with_quic = true,
        with_experimental = true,
        with_rpk = true
    }
})

-- add_requires(
-- -- "onnxruntime ~1.19.2",
-- --     "ftxui ~5.0.0",
-- -- "cli11 ~2.5.0",
--     "doctest ~2.4.11",
--     "gtest ~1.16.0",
--     "gflags ~2.2.2",
--     -- "rapidjson ~2024.08.16",
--     -- "simde ~0.8.2",
--     "benchmark ~1.9.1"
-- )

-- add_requires("nanobench ~4.3.11")

--add_requires("fmt ~11.1.4", { configs = { header_only = true } })

includes("lib", "tests")
