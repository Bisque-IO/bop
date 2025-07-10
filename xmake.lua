-- windows install conan: scoop install conan
-- make sure conan is pointing to correct URL
-- conan remote update conancenter --url https://center2.conan.io

set_project("bop")

set_description("data platform for modern computing")

set_languages("c++23", "c23")
--set_warnings("all")
add_rules("mode.debug", "mode.release")

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
-- set_sourcedir(path.join(os.scriptdir(), "lib/wamr"))

-- on_install("windows", "linux", "macosx", "bsd", "android", function(package)
--     local configs = {
--         -- "-DWAMR_BUILD_INVOKE_NATIVE_GENERAL=1",
--         -- "-DCMAKE_POLICY_DEFAULT_CMP0057=NEW"
--     }
--     table.insert(configs, "-DCMAKE_BUILD_TYPE=" .. (package:is_debug() and "Debug" or "Release"))
--     table.insert(configs, "-DBUILD_SHARED_LIBS=" .. (package:config("shared") and "ON" or "OFF"))
--     if package:is_plat("windows") and (not package:config("shared")) then
--         package:add("defines", "COMPILING_WASM_RUNTIME_API=1")
--     end

--     table.insert(configs, "-DWAMR_BUILD_INTERP=" .. (package:config("interp") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_FAST_INTERP=" .. (package:config("fast_interp") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_AOT=" .. (package:config("aot") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_JIT=" .. (package:config("jit") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_FAST_JIT=" .. (package:config("fast_jit") and "1" or "0"))

--     table.insert(configs,
--         "-DWAMR_BUILD_LIBC_BUILTIN=" ..
--         ((package:config("libc_builtin") or package:config("libc") == "builtin") and "1" or "0"))
--     table.insert(configs,
--         "-DWAMR_BUILD_LIBC_WASI=" .. ((package:config("libc_wasi") or package:config("libc") == "wasi") and "1" or "0"))
--     table.insert(configs,
--         "-DWAMR_BUILD_LIBC_UVWASI=" ..
--         ((package:config("libc_uvwasi") or package:config("libc") == "uvwasi") and "1" or "0"))

--     table.insert(configs, "-DWAMR_BUILD_MULTI_MODULE=" .. (package:config("multi_module") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_MINI_LOADER=" .. (package:config("mini_loader") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_LIB_WASI_THREADS=" .. (package:config("wasi_threads") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_SIMD=" .. (package:config("simd") and "1" or "0"))
--     table.insert(configs, "-DWAMR_BUILD_REF_TYPES=" .. (package:config("ref_types") and "1" or "0"))

--     local packagedeps
--     -- if package:config("libc_uvwasi") or package:config("libc") == "uvwasi" then
--     --     if package:is_plat("windows", "linux", "macosx") then
--     --         packagedeps = { "uvwasi", "libuv" }
--     --     end
--     -- end
--     -- if package:is_plat("android") then
--     --     table.insert(configs, "-DWAMR_BUILD_PLATFORM=android")
--     -- end
--     import("package.tools.cmake").install(package, configs, { packagedeps = packagedeps })
-- end)
-- on_test(function(package)
--     assert(package:has_cfuncs("wasm_engine_new", { includes = "wasm_c_api.h" }))
-- end)
-- package_end()

-- add_requires("wamr2")

add_requires("conan::zstd/1.5.7", {
    alias = "zstd",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("conan::zlib/1.3.1", {
    alias = "zlib",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("conan::lz4/1.10.0", {
    alias = "lz4",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("conan::brotli/1.1.0", {
    alias = "brotli",
    configs = {
        shared = false,
        fPIC = true
    }
})

if is_plat("linux") then
    add_requires("conan::libaio/0.3.113", { alias = "libaio", configs = { shared = false } })
    add_requires("conan::liburing/2.8", { alias = "liburing", configs = { shared = false } })
end

add_requires("zig ~0.14.0")

add_requires("openssl3 ~3.3.2", {
    --add_requires("conan::openssl/3.5.0", {
    alias = "openssl3",
    configs = {
        fPIC = true,
        shared = true
    }
})

add_requires("boost ~1.88.0", {
    alias = "boost",
    configs = {
        asio = true,
        charconv = true,
        chrono = true,
        cobalt = false,
        container = true,
        coroutine = true,
        date_time = false,
        headers = true,
        json = false,
        random = false,
        regex = true,
        stacktrace = false,
        system = true,
        thread = false,
        timer = false,
        test = false,
        url = true,
        zlib = true,
        zstd = true,
        header_only = false
    }
})

add_requires("wolfssl ~5.7.2", {
    --add_requires("conan::wolfssl/5.7.0", {
    alias = "wolfssl1",
    configs = {
        shared = true,
        asio = true,
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

add_requires(
-- "onnxruntime ~1.19.2",
--     "ftxui ~5.0.0",
    "cli11 ~2.5.0",
    "doctest ~2.4.11",
    "gtest ~1.16.0",
    "gflags ~2.2.2",
    -- "rapidjson ~2024.08.16",
    -- "simde ~0.8.2",
    "benchmark ~1.9.1"
)

add_requires("nanobench ~4.3.11")

add_requires("fmt ~11.1.4", { configs = { header_only = true } })

includes("lib")
