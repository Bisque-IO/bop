use std::{env, fs, path::PathBuf};

fn main() {
    // Let rustc know we use a custom cfg for IDEs
    println!("cargo:rustc-check-cfg=cfg(rust_analyzer)");
    // Always rebuild if the header or script changes
    println!("cargo:rerun-if-changed=build.rs");

    // Resolve header path for bindgen
    let header = env::var("BOP_BINDINGS_HEADER")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("ffi/bindings.h"));
    println!("cargo:rerun-if-changed={}", header.display());

    // Target triple info (used for both bindgen and linking decisions)
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default(); // "msvc" or "gnu" on Windows

    // Collect include paths
    let mut clang_args: Vec<String> = Vec::new();

    // Add project include to find headers under lib/src/**
    let project_include = PathBuf::from("../../lib/src");
    if project_include.exists() {
        clang_args.push(format!("-I{}", project_include.display()));
    }

    // Optional extra include paths from env (separated by ; or :)
    if let Ok(extra_includes) = env::var("BOP_NATIVE_INCLUDE") {
        for p in extra_includes
            .split(|c| c == ';' || c == ':')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            clang_args.push(format!("-I{}", p));
        }
    }

    // Treat header as C++ when requested, or on Windows by default (MS headers often include C++ headers)
    if env::var_os("BOP_CPP").is_some() || target_os == "windows" {
        clang_args.push("-x".into());
        clang_args.push("c++".into());
        // Default to C++23 to match the project
        clang_args.push("-std=c++23".into());
    }

    // Always generate bindings for bop-sys
    let mut builder = bindgen::Builder::default()
        .header(header.display().to_string())
        .clang_args(clang_args)
        // Keep layout stable and avoid deriving Copy on opaque types
        .layout_tests(false)
        .derive_default(true)
        .generate_comments(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    // Apply default allowlist for BOP API prefixes
    let default_prefixes = ["bop_", "us_", "mdbx_", "uws_", "MDBX", "BOP", "LIBUS", "UWS"];
    for prefix in &default_prefixes {
        let pattern = format!("{}.*", prefix);
        builder = builder
            .allowlist_item(&pattern)
            .allowlist_function(&pattern)
            .allowlist_type(&pattern)
            .allowlist_var(&pattern);
    }

    // Optional allowlist via env: BOP_BINDINGS_ALLOWLIST_REGEX (comma-separated regexes)
    // This supplements the default prefixes above
    if let Ok(allow) = env::var("BOP_BINDINGS_ALLOWLIST_REGEX") {
        for pat in allow.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            builder = builder
                .allowlist_item(pat)
                .allowlist_function(pat)
                .allowlist_type(pat)
                .allowlist_var(pat);
        }
    }

    let bindings = builder
        .generate()
        .expect("Unable to generate Rust bindings with bindgen");

    let content = bindings.to_string();
    // Make extern blocks explicitly unsafe to satisfy stricter toolchains/lints
    // content = content.replace("extern \"C\" {", "unsafe extern \"C\" {");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_path.join("bindings.rs"), content).expect("Couldn't write bindings!");

    // Auto-select prebuilt native library location (can be overridden with env)
    // Layout: odin/libbop/<os>/<arch>/libbop.(a|lib)
    // (target_* gathered above)

    // Map to directory names
    let os_dir = match target_os.as_str() {
        "linux" => "linux",
        "macos" => "macos",
        "windows" => "windows",
        other => {
            println!("cargo:warning=Unsupported target OS for prebuilt lib: {}", other);
            "" // no auto path
        }
    };
    let arch_dir = match target_arch.as_str() {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => {
            println!("cargo:warning=Unsupported target arch for prebuilt lib: {}", other);
            ""
        }
    };

    // Allow override of base directory via env; default to repo layout
    let base = env::var("BOP_NATIVE_BASE").unwrap_or_else(|_| "../../odin/libbop".into());
    let auto_lib_dir = if !os_dir.is_empty() && !arch_dir.is_empty() {
        let p = PathBuf::from(&base).join(os_dir).join(arch_dir);
        if p.exists() { Some(p) } else { None }
    } else {
        None
    };

    // Env overrides for additional/custom lib dirs
    if let Ok(lib_dirs) = env::var("BOP_NATIVE_LIB_DIR") {
        for dir in lib_dirs
            .split(|c| c == ';' || c == ':')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            println!("cargo:rustc-link-search=native={}", dir);
        }
    }

    if let Some(p) = auto_lib_dir.as_ref() {
        let abs = p.canonicalize().unwrap_or_else(|_| p.clone());
        println!("cargo:rustc-link-search=native={}", abs.display());
        // Try to detect the static library file to decide link kind and name
        let lib_a = abs.join("libbop.a");
        // On MSVC, the library is named without the lib- prefix
        let lib_lib = abs.join("bop.lib");
        let have_a = lib_a.exists();
        let have_lib = lib_lib.exists();

        // On windows-msvc, prefer .lib. On windows-gnu and other platforms, .a is expected.
        let wants_msvc = target_os == "windows" && target_env == "msvc";
        let wants_gnu = target_os == "windows" && target_env == "gnu";

        if wants_msvc && !have_lib {
            println!("cargo:warning=Target is windows-msvc but {} not found. Provide bop.lib or set BOP_NATIVE_LIBS/DIR.", lib_lib.display());
        }
        if !wants_msvc && !have_a {
            println!("cargo:warning=Static archive {} not found. Provide libbop.a or set BOP_NATIVE_LIBS/DIR.", lib_a.display());
        }

        // Default link kind/name
        if wants_msvc && have_lib {
            // MSVC uses the plain name without lib prefix. Do not force static vs dylib.
            println!("cargo:rustc-link-lib=bop");
            // Link required Windows system libs for mdbx and Crypto/Registry usage
            println!("cargo:rustc-link-lib=advapi32");
            println!("cargo:rustc-link-lib=user32");
            // Link additional Windows SDK libs used by libuv/usockets on Windows
            // GetAdaptersAddresses
            println!("cargo:rustc-link-lib=iphlpapi");
            // COM task memory helpers
            println!("cargo:rustc-link-lib=ole32");
            // SHGetKnownFolderPath
            println!("cargo:rustc-link-lib=shell32");
            // Winsock (usually already present, but add explicitly)
            println!("cargo:rustc-link-lib=ws2_32");
            // Opportunistically link common deps if present in the same folder
            for dep in [
                "wolfssl",
                "zlibstatic",
                "libzstd",
                // "libcrypto_static",
                // "libssl_static",
                "iwasm",
            ] {
                let cand = abs.join(format!("{dep}.lib"));
                if cand.exists() {
                    println!("cargo:rustc-link-lib={}", dep);
                }
            }
        } else if (wants_gnu && have_a) || (!wants_msvc && have_a) {
            // GNU style uses libbop.a with -lbop
            println!("cargo:rustc-link-lib=static=bop");
            // Also link common static deps if present
            let deps = [
                ("wolfssl", "libwolfssl.a"),
                ("z", "libz.a"),
                ("zstd", "libzstd.a"),
            ];
            for (name, file) in deps {
                if abs.join(file).exists() {
                    println!("cargo:rustc-link-lib=static={}", name);
                }
            }
        } else {
            // Fall back to generic if user supplied libs via env
        }
    }

    // Finally, allow explicit list via env to override everything
    if let Ok(libs) = env::var("BOP_NATIVE_LIBS") {
        for lib in libs.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            match env::var("BOP_NATIVE_LINK_KIND").as_deref() {
                Ok("static") => println!("cargo:rustc-link-lib=static={}", lib),
                Ok("dylib") => println!("cargo:rustc-link-lib=dylib={}", lib),
                _ => println!("cargo:rustc-link-lib={}", lib),
            }
        }
    }
}
