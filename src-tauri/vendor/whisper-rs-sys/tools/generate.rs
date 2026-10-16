//! Regenerate both Windows snapshots from the exact shipped native headers.
//! Run with DOCS_RS=1 so generating Rust declarations does not build a GPU
//! backend or require a Vulkan SDK. libclang is needed only for this command.
use std::path::PathBuf;

fn main() {
    let source = PathBuf::from(env!("WHISPER_BINDINGS_SOURCE_ROOT"));
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bindings");
    std::fs::create_dir_all(&output).expect("create bindings directory");
    for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        let bindings = bindgen::Builder::default()
            .rust_edition(bindgen::RustEdition::Edition2021)
            .rust_target(bindgen::RustTarget::stable(88, 0).unwrap())
            .header(source.join("wrapper.h").to_string_lossy())
            .header(
                source
                    .join("whisper.cpp/ggml/include/ggml-vulkan.h")
                    .to_string_lossy(),
            )
            .clang_arg(format!("--target={target}"))
            .clang_arg("-DGGML_USE_VULKAN=1")
            .clang_arg(format!("-I{}", source.join("whisper.cpp").display()))
            .clang_arg(format!(
                "-I{}",
                source.join("whisper.cpp/include").display()
            ))
            .clang_arg(format!(
                "-I{}",
                source.join("whisper.cpp/ggml/include").display()
            ))
            // Avoid shipping unrelated CRT internals. Recursive allowlisting
            // still includes every type required by these public declarations.
            .allowlist_function("(whisper|ggml|gguf)_.*")
            .allowlist_type("(whisper|ggml|gguf)_.*")
            .allowlist_var("(WHISPER|GGML|GGUF)_.*")
            .raw_line(format!(
                "// Target: {target}; whisper-rs-sys 0.15.0; bindgen 0.72."
            ))
            .generate()
            .expect("generate target-specific Windows bindings");
        bindings
            .write_to_file(output.join(format!("{target}.rs")))
            .expect("write generated Windows snapshot");
        println!("Generated {target} bindings.");
    }
}
