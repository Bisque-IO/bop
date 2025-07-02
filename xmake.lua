-- windows install conan: scoop install conan
-- make sure conan is pointing to correct URL
-- conan remote update conancenter --url https://center2.conan.io

set_project("bop")

set_description("data platform for modern computing")

set_languages("c++23", "c23")
--set_warnings("all")
add_rules("mode.debug", "mode.release")

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
    -- add_requires("conan::openssl/3.5.0", {
    alias = "openssl3",
    configs = {
        fPIC = true,
        shared = false,
        static = true
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

add_requires("conan::libuv/1.49.2", {
    alias = "libuv",
    configs = {
        shared = false,
        fPIC = true
    }
})

add_requires("wolfssl ~5.7.2", {
    alias = "wolfssl",
    configs = {
        shared = false,
        asio = true,
        fPIC = true,
        openssl_all = true,
        openssl_extra = true
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
