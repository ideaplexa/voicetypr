// swift-tools-version: 6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "ParakeetSidecar",
    platforms: [
        .macOS(.v14)  // FluidAudio 0.14+ diarization/offline APIs require macOS 14+
    ],
    dependencies: [
        .package(url: "https://github.com/FluidInference/FluidAudio.git", from: "0.17.4")
    ],
    targets: [
        .executableTarget(
            name: "ParakeetSidecar",
            dependencies: [
                .product(name: "FluidAudio", package: "FluidAudio")
            ]
        ),
    ]
)
