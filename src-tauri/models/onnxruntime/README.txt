ONNX Runtime 1.28.0, statically linked through ort 2.0.0-rc.13.
Unmodified upstream source: https://github.com/microsoft/onnxruntime/tree/v1.28.0
Rust wrapper source: https://github.com/pykeio/ort/tree/v2.0.0-rc.13

LICENSE covers ONNX Runtime. ort-LICENSE-MIT covers the Rust wrapper.
ThirdPartyNotices.txt contains the upstream runtime's dependency notices.
The build uses ort-sys's checksum-pinned prebuilt archive. Windows also ships
that archive's DirectML.dll dependency, even for CPU-only sessions.
