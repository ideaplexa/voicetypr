use std::path::Path;
#[cfg(target_os = "macos")]
use std::process::Command;

fn main() {
    // Set the deployment target to match our minimum system version
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=14.0");

    // Build Swift Parakeet sidecar on macOS
    #[cfg(target_os = "macos")]
    {
        let sidecar_dir = std::path::Path::new("../sidecar/parakeet-swift");
        let build_script = sidecar_dir.join("build.sh");
        let dist_dir = sidecar_dir.join("dist");

        if build_script.exists() {
            println!("Building Swift Parakeet sidecar...");

            // Ensure dist directory exists
            std::fs::create_dir_all(&dist_dir).ok();

            let output = Command::new("bash")
                .arg("build.sh")
                .arg("release")
                .current_dir(sidecar_dir)
                .output();

            match output {
                Ok(output) => {
                    if !output.status.success() {
                        panic!(
                            "Swift sidecar build failed ({}).\n--- swift build stdout ---\n{}\n--- swift build stderr ---\n{}",
                            output.status,
                            String::from_utf8_lossy(&output.stdout),
                            String::from_utf8_lossy(&output.stderr)
                        );
                    } else {
                        println!("Swift sidecar built successfully");

                        // Verify the binary exists
                        let target_triple = std::env::var("TARGET")
                            .unwrap_or_else(|_| "aarch64-apple-darwin".to_string());
                        let binary_name = format!("parakeet-sidecar-{}", target_triple);
                        let binary_path = dist_dir.join(&binary_name);

                        if binary_path.exists() {
                            println!(
                                "Parakeet sidecar binary verified at: {}",
                                binary_path.display()
                            );
                        } else {
                            println!(
                                "cargo:warning=Warning: Expected binary not found at {}",
                                binary_path.display()
                            );
                        }
                    }
                }
                Err(e) => {
                    panic!("Failed to run Swift build script: {}", e);
                }
            }
        } else {
            println!("cargo:warning=Swift build script not found, skipping sidecar build");
        }

        // Tell Cargo to re-run if Swift sources change
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/Sources");
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/Package.swift");
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/Package.resolved");
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/build.sh");
    }

    if std::env::var("VOICETYPR_REQUIRE_VULKAN_SIDECAR").as_deref() == Ok("1") {
        let sidecar_dir = Path::new("../sidecar/whisper-vulkan/dist");
        let sidecar_exe = sidecar_dir.join("whisper-vulkan-sidecar-x86_64-pc-windows-msvc.exe");
        if !sidecar_exe.exists() {
            panic!(
                "Whisper Vulkan sidecar not found: {}",
                sidecar_exe.display()
            );
        }
    }

    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../pnpm-lock.yaml");
    println!("cargo:rerun-if-changed=../pnpm-workspace.yaml");

    // Tauri validates `bundle.resources` paths at COMPILE time (generate_context!
    // / tauri-build, which auto-merges tauri.windows.conf.json for the Windows
    // target). We ship voicetypr.pdb (debug symbols) as a resource, but it is a
    // BUILD OUTPUT of this very compile: scripts/stage-windows-pdb.cjs
    // (beforeBundleCommand) copies the real PDB into place only at bundle time --
    // too late for this validation, and absent entirely in a plain `cargo build`
    // (the CI compile check). Create a placeholder now so validation passes; the
    // real PDB overwrites it before bundling.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let pdb_resource = Path::new("windows/resources/voicetypr.pdb");
        if !pdb_resource.exists() {
            if let Some(dir) = pdb_resource.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            std::fs::File::create(pdb_resource).ok();
        }
    }

    tauri_build::build()
}
