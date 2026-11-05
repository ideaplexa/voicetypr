ONNX Runtime 1.28.0, statically linked through ort 2.0.0-rc.13.
Unmodified upstream source: https://github.com/microsoft/onnxruntime/tree/v1.28.0
Rust wrapper source: https://github.com/pykeio/ort/tree/v2.0.0-rc.13

LICENSE covers ONNX Runtime. ort-LICENSE-MIT covers the Rust wrapper.
ThirdPartyNotices.txt contains the upstream runtime's dependency notices.
The build uses ort-sys's checksum-pinned prebuilt archive. Windows also ships
that archive's DirectML.dll dependency, even for CPU-only sessions.

DirectML is NOT MIT. DirectML-LICENSE.txt and DirectML-ThirdPartyNotices.txt
are unmodified files from Microsoft.AI.DirectML 1.15.4. The x64 DLL in the
hash-pinned ORT archive is byte-for-byte identical to NuGet's bin/x64-win DLL.
DirectML-provenance.json pins both packages, DLLs and redistribution notices.
Both NSIS and Store MSIX staging fail closed on a missing or different DLL.
There is no CPU-only Windows archive in rc.13's default distribution; disabling
the Cargo directml feature does not remove ort-sys's unconditional native link.
Avoiding DirectML requires a separately built/reviewed CPU-only ORT, not a second
runtime or a session provider setting. No GPU provider is selected by this app.

CPU checks precede every ORT entry point: x86 requires AVX2 plus the associated
FMA/BMI/F16C/LZCNT/POPCNT/SSE4.2/MOVBE features conservatively permitted by
MSVC /arch:AVX2. Rust's detection also checks OS-enabled AVX state.
Sources: ort rc.13 src/environment.rs and ort-sys build/static_link/mod.rs;
https://learn.microsoft.com/en-us/cpp/build/reference/arch-x64
