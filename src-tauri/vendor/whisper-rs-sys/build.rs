// Native build settings follow whisper-rs-sys 0.15.0. The local changes are
// extraction of its exact source archive and target-aware shipped bindings.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const SOURCE: &[u8] = include_bytes!("upstream/whisper-rs-sys-0.15.0.crate");
const SOURCE_SHA256: &str = "6986c0fe081241d391f09b9a071fbcbb59720c3563628c3c829057cf69f2a56f";

fn source_root(out: &Path) -> PathBuf {
    let digest = format!("{:x}", Sha256::digest(SOURCE));
    assert_eq!(
        digest, SOURCE_SHA256,
        "Pinned Whisper source archive changed"
    );
    let root = out.join("source/whisper-rs-sys-0.15.0");
    if !root.join(".verified").exists() {
        let source = out.join("source");
        fs::create_dir_all(&source).expect("create native source directory");
        tar::Archive::new(flate2::read::GzDecoder::new(SOURCE))
            .unpack(&source)
            .expect("extract pinned Whisper source archive");
        fs::write(root.join(".verified"), SOURCE_SHA256).expect("mark verified source");
    }
    root
}

fn generate_bindings(root: &Path, out: &Path) {
    if env::var_os("WHISPER_DONT_GENERATE_BINDINGS").is_some() {
        let target = env::var("TARGET").unwrap();
        let snapshot = match target.as_str() {
            "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc" => {
                let snapshot = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                    .join("bindings").join(format!("{target}.rs"));
                println!("cargo:rerun-if-changed={}", snapshot.display());
                snapshot
            }
            // This is the target of the original upstream snapshot. Other
            // targets must generate their own ABI rather than borrowing it.
            "x86_64-unknown-linux-gnu" => root.join("src/bindings.rs"),
            _ => panic!("No shipped Whisper bindings for {target}; unset WHISPER_DONT_GENERATE_BINDINGS and configure libclang"),
        };
        fs::copy(snapshot, out.join("bindings.rs")).expect("copy shipped bindings");
        return;
    }
    let mut bindings = bindgen::Builder::default()
        .rust_edition(bindgen::RustEdition::Edition2021)
        .rust_target(bindgen::RustTarget::stable(88, 0).unwrap())
        .header(root.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", root.join("whisper.cpp").display()))
        .clang_arg(format!("-I{}", root.join("whisper.cpp/include").display()))
        .clang_arg(format!(
            "-I{}",
            root.join("whisper.cpp/ggml/include").display()
        ))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));
    if cfg!(feature = "metal") {
        bindings = bindings.header(
            root.join("whisper.cpp/ggml/include/ggml-metal.h")
                .to_string_lossy(),
        );
    }
    if cfg!(feature = "vulkan") {
        bindings = bindings
            .header(
                root.join("whisper.cpp/ggml/include/ggml-vulkan.h")
                    .to_string_lossy(),
            )
            .clang_arg("-DGGML_USE_VULKAN=1");
    }
    // A foreign-ABI fallback is not safe. A generation error must be visible;
    // developers can explicitly opt into a supported shipped snapshot instead.
    bindings
        .generate()
        .expect("generate Whisper bindings (libclang required)")
        .write_to_file(out.join("bindings.rs"))
        .expect("write Whisper bindings");
}

fn main() {
    println!("cargo:rerun-if-changed=upstream/whisper-rs-sys-0.15.0.crate");
    println!("cargo:rerun-if-env-changed=WHISPER_DONT_GENERATE_BINDINGS");
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
    println!("cargo:rerun-if-env-changed=DOCS_RS");
    let target = env::var("TARGET").unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let root = source_root(&out);
    let native = root.join("whisper.cpp");
    // Used only by the explicit regeneration binary; consumers need no source
    // path and normal builds never write generated files into the checkout.
    println!(
        "cargo:rustc-env=WHISPER_BINDINGS_SOURCE_ROOT={}",
        root.display()
    );
    generate_bindings(&root, &out);
    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=Accelerate");
        if cfg!(feature = "coreml") {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=CoreML");
        }
        if cfg!(feature = "metal") {
            for framework in ["Foundation", "Metal", "MetalKit"] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
    } else if target.contains("android") {
        println!("cargo:rustc-link-lib=c++_shared");
    } else if !target.contains("msvc") {
        println!(
            "cargo:rustc-link-lib={}",
            if target.contains("freebsd") || target.contains("openbsd") {
                "c++"
            } else {
                "stdc++"
            }
        );
    }

    let mut config = cmake::Config::new(&native);
    config
        .profile("Release")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("WHISPER_ALL_WARNINGS", "OFF")
        .define("WHISPER_ALL_WARNINGS_3RD_PARTY", "OFF")
        .define("WHISPER_BUILD_TESTS", "OFF")
        .define("WHISPER_BUILD_EXAMPLES", "OFF")
        .pic(true);
    if target.contains("windows") {
        config.cxxflag("/utf-8");
        println!("cargo:rustc-link-lib=advapi32");
    }
    if cfg!(feature = "coreml") {
        config
            .define("WHISPER_COREML", "ON")
            .define("WHISPER_COREML_ALLOW_FALLBACK", "1");
        println!("cargo:rustc-link-lib=static=whisper.coreml");
    }
    if cfg!(feature = "cuda") {
        config
            .define("GGML_CUDA", "ON")
            .define("CMAKE_POSITION_INDEPENDENT_CODE", "ON");
        if !target.contains("windows") {
            config.define("CMAKE_CUDA_FLAGS", "-Xcompiler=-fPIC");
        }
        for lib in ["cublas", "cudart", "cublasLt", "cuda"] {
            println!("cargo:rustc-link-lib={lib}");
        }
        if target.contains("windows") {
            println!(
                "cargo:rustc-link-search={}/lib/x64",
                env::var("CUDA_PATH").expect("CUDA_PATH required")
            );
        } else {
            println!("cargo:rustc-link-lib=culibos");
            for dir in [
                "/usr/local/cuda/lib64",
                "/usr/local/cuda/lib64/stubs",
                "/opt/cuda/lib64",
                "/opt/cuda/lib64/stubs",
            ] {
                println!("cargo:rustc-link-search={dir}");
            }
        }
    }
    if cfg!(feature = "hipblas") {
        assert!(
            !target.contains("windows"),
            "Upstream whisper-rs-sys does not support Windows HIP builds"
        );
        config
            .define("GGML_HIP", "ON")
            .define("CMAKE_C_COMPILER", "hipcc")
            .define("CMAKE_CXX_COMPILER", "hipcc");
        if let Ok(value) = env::var("AMDGPU_TARGETS") {
            config.define("AMDGPU_TARGETS", value);
        }
        println!("cargo:rerun-if-env-changed=AMDGPU_TARGETS");
        println!("cargo:rerun-if-env-changed=HIP_PATH");
        println!(
            "cargo:rustc-link-search={}/lib",
            env::var("HIP_PATH").unwrap_or_else(|_| "/opt/rocm".into())
        );
        for lib in ["hipblas", "rocblas", "amdhip64"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    if cfg!(feature = "openmp") {
        if target.contains("gnu") {
            println!("cargo:rustc-link-lib=gomp");
        } else if target.contains("apple") {
            println!("cargo:rustc-link-lib=omp");
            println!("cargo:rustc-link-search=/opt/homebrew/opt/libomp/lib");
        }
    }
    if cfg!(feature = "vulkan") {
        config.define("GGML_VULKAN", "ON");
        println!("cargo:rerun-if-env-changed=VULKAN_SDK");
        if target.contains("windows") || target.contains("apple") {
            let sdk = PathBuf::from(
                env::var_os("VULKAN_SDK").expect("VULKAN_SDK required for Vulkan builds"),
            );
            println!(
                "cargo:rustc-link-search={}",
                sdk.join(if target.contains("windows") {
                    "Lib"
                } else {
                    "lib"
                })
                .display()
            );
        }
        println!(
            "cargo:rustc-link-lib={}",
            if target.contains("windows") {
                "vulkan-1"
            } else {
                "vulkan"
            }
        );
    }
    if cfg!(feature = "openblas") {
        config
            .define("GGML_BLAS", "ON")
            .define("GGML_BLAS_VENDOR", "OpenBLAS")
            .define(
                "BLAS_INCLUDE_DIRS",
                env::var("BLAS_INCLUDE_DIRS").expect("BLAS_INCLUDE_DIRS required"),
            );
        println!("cargo:rerun-if-env-changed=BLAS_INCLUDE_DIRS");
        if let Ok(path) = env::var("OPENBLAS_PATH") {
            println!("cargo:rustc-link-search={path}/lib");
        }
        println!(
            "cargo:rustc-link-lib={}",
            if target.contains("windows") {
                "libopenblas"
            } else {
                "openblas"
            }
        );
    }
    config.define(
        "GGML_METAL",
        if cfg!(feature = "metal") { "ON" } else { "OFF" },
    );
    if cfg!(feature = "metal") {
        config
            .define("GGML_METAL_NDEBUG", "ON")
            .define("GGML_METAL_EMBED_LIBRARY", "ON");
    }
    if cfg!(debug_assertions) || cfg!(feature = "force-debug") {
        config
            .define("CMAKE_BUILD_TYPE", "RelWithDebInfo")
            .cxxflag("-DWHISPER_DEBUG");
    } else {
        config.define("CMAKE_BUILD_TYPE", "Release");
    }
    // Preserve upstream's build-time escape hatch for native toolchain flags.
    for (key, value) in env::vars() {
        if (key.starts_with("WHISPER_") && key != "WHISPER_DONT_GENERATE_BINDINGS")
            || key.starts_with("GGML_")
            || key.starts_with("CMAKE_")
        {
            config.define(key, value);
        }
    }
    if !cfg!(feature = "openmp") {
        config.define("GGML_OPENMP", "OFF");
    }
    if cfg!(feature = "intel-sycl") {
        config
            .define("BUILD_SHARED_LIBS", "ON")
            .define("GGML_SYCL", "ON")
            .define("GGML_SYCL_TARGET", "INTEL")
            .define("CMAKE_C_COMPILER", "icx")
            .define("CMAKE_CXX_COMPILER", "icpx");
    }
    let destination = config.build();
    add_link_search_path(&out.join("build"));
    println!("cargo:rustc-link-search=native={}", destination.display());
    let kind = if cfg!(feature = "intel-sycl") {
        "dylib"
    } else {
        "static"
    };
    for lib in ["whisper", "ggml", "ggml-base", "ggml-cpu"] {
        println!("cargo:rustc-link-lib={kind}={lib}");
    }
    for (enabled, lib) in [
        (
            target.contains("apple") || cfg!(feature = "openblas"),
            "ggml-blas",
        ),
        (cfg!(feature = "vulkan"), "ggml-vulkan"),
        (cfg!(feature = "hipblas"), "ggml-hip"),
        (cfg!(feature = "metal"), "ggml-metal"),
        (cfg!(feature = "cuda"), "ggml-cuda"),
        (cfg!(feature = "intel-sycl"), "ggml-sycl"),
    ] {
        if enabled {
            println!("cargo:rustc-link-lib={kind}={lib}");
        }
    }
    let cmake = fs::read_to_string(native.join("CMakeLists.txt")).expect("read native version");
    let version = cmake
        .lines()
        .find_map(|line| line.strip_prefix("project(\"whisper.cpp\" VERSION "))
        .expect("native version declaration")
        .trim_end_matches(')');
    println!("cargo:WHISPER_CPP_VERSION={version}");

    #[cfg(feature = "verify-bindings")]
    if target.contains("windows") {
        println!("cargo:rerun-if-changed=tests/windows_abi.c");
        cc::Build::new()
            .file("tests/windows_abi.c")
            .include(native.join("include"))
            .include(native.join("ggml/include"))
            .flag_if_supported("/std:c11")
            .compile("whisper_bindings_abi");
    }
}

fn add_link_search_path(dir: &Path) {
    if dir.is_dir() {
        println!("cargo:rustc-link-search=native={}", dir.display());
        for entry in fs::read_dir(dir).expect("native build directory").flatten() {
            if entry.path().is_dir() {
                add_link_search_path(&entry.path());
            }
        }
    }
}
