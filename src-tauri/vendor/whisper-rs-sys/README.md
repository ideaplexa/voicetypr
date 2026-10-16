# whisper-rs-sys Windows shipped bindings

This is a narrow local patch of **whisper-rs-sys 0.15.0** (Unlicense), from
upstream revision `7558e1b72f54f2f22a53589afb77e65681834c36`.

The original crate archive is retained under `upstream/`, including its native
whisper.cpp/ggml sources and MIT license. SHA-256:
`6986c0fe081241d391f09b9a071fbcbb59720c3563628c3c829057cf69f2a56f`.
`node scripts/prepare-whisper-sys.mjs` verifies or restores that exact archive.
The build extracts it into Cargo's per-build output directory; it does not
download source during compilation or modify Cargo's shared source cache.

## Why the patch exists

Upstream's `WHISPER_DONT_GENERATE_BINDINGS=1` copies one Linux-generated
`bindings.rs` on every target. On Windows its C `long`/stdio structure layout
assertions fail (`_G_fpos_t`, `_G_fpos64_t`, `_IO_FILE`), before app compilation.

The local build selects a matching checked-in Windows MSVC snapshot when the
existing opt-in flag is set. Without the flag, normal bindgen generation stays
available. Snapshots include the Vulkan declarations so the main CPU app and
the optional Vulkan sidecar use the same bindings. Target selection uses
Cargo's **TARGET**, not the build host. Unsupported shipped targets fail with
an actionable error rather than silently accepting a foreign ABI.

## Verification

Run the crate's `verify-bindings` tests with the shipped-bindings option and
an invalid `LIBCLANG_PATH`. They compare Rust struct sizes, alignment and field
offsets with a C fixture compiled against the archive's exact native headers.
The fixture is linked only with the verification feature, not in app builds.

Generated snapshots and their generator are checked in. Regeneration requires
libclang; normal shipped-bindings builds do not. Native compilation still needs
the platform C/C++ toolchain, CMake, and the Vulkan SDK for Vulkan builds.

From the repository root in PowerShell:

```powershell
$env:WHISPER_DONT_GENERATE_BINDINGS = "1"
cargo test --manifest-path src-tauri/vendor/whisper-rs-sys/Cargo.toml --features verify-bindings --test windows_abi
# The same option is consumed by both application and sidecar Cargo patches.
cargo build --manifest-path sidecar/whisper-vulkan/Cargo.toml --release
```

For application tests, invoke `./run-tests.ps1 -TestFilter language` from
`src-tauri` with the same environment option. That runner embeds the required
Windows Common-Controls manifest before executing tests.

To regenerate (libclang configured, Windows C headers available):

```powershell
$env:WHISPER_DONT_GENERATE_BINDINGS = $null
$env:DOCS_RS = "1"
cargo run --manifest-path src-tauri/vendor/whisper-rs-sys/Cargo.toml --features generate-bindings --bin generate-windows-bindings
$env:DOCS_RS = $null
```

`DOCS_RS` is used only by the generator to skip native linking. Do not set it
when verifying or building the app. Generated snapshots retain bindgen's
compile-time layout assertions. The x64 native ABI test runs on Windows x64;
ARM64 declarations require the same test on an ARM64 runner before claiming
native ARM64 ABI verification.
