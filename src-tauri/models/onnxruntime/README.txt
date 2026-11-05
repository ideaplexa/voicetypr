Windows uses Microsoft's official ONNX Runtime 1.28.0 CPU-only dynamic DLL.
Source: https://github.com/microsoft/onnxruntime/releases/tag/v1.28.0
Rust wrapper: https://github.com/pykeio/ort/tree/v2.0.0-rc.13

provenance.json pins the official Windows x64 ZIP, lib/onnxruntime.dll and
the exact LICENSE and ThirdPartyNotices.txt bytes from that ZIP. These two
notices must not undergo line-ending conversion. ort-LICENSE-MIT covers the
Rust wrapper separately. The DLL is acquired at packaging time, not committed.
No static ORT archive, import library, DirectML or GPU provider is packaged.

scripts/stage-windows-ort.cjs downloads and verifies the pinned archive into
CARGO_TARGET_DIR/onnxruntime (default src-tauri/target/onnxruntime). --archive
can use an existing ZIP, with the same mandatory hashes and architecture
checks. Missing explicit archives, corrupt caches and unsupported targets fail
closed. Only the unmodified signed onnxruntime.dll is extracted for runtime use.
Never re-sign this Microsoft DLL: preserve its original hash and embedded
Authenticode signature. Sign the application/installer/package, not this DLL.
There is no separate bundled-PE signing pass here. An executable's signature
does not cover sibling DLLs; retain existing updater/Store package signing.
The lazy ORT environment explicitly opts out of Microsoft's native telemetry.
Its discard-only native logger prevents raw native diagnostics/paths on stderr.

NSIS stages the DLL under windows/resources/vcredist and copies it beside the
main executable, with required-file checks. Store MSIX explicitly stages the
DLL at the package root and the notices under models/onnxruntime; both the
layout and unpacked final MSIX are verified against the pins.

assert-no-onnxruntime-import.ps1 requires the main executable's linker MAP.
It rejects native onnxruntime/DirectML imports and an unmangled OrtGetApiBase
public symbol definition (static ORT), not the dynamic loader's symbol string.
The main image name, preferred load address and linker timestamp must match
the supplied executable; a missing, mismatched or incomplete MAP fails closed.
