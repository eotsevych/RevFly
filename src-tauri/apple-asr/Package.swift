// swift-tools-version: 6.0
// revfly-asr: runs Parakeet TDT 0.6B v3 on the Neural Engine (Core ML via FluidAudio) for RevFly.
// Built by scripts/build_apple_asr.sh; bundled as a sidecar next to the RevFly executable.
import PackageDescription

let package = Package(
    name: "revfly-asr",
    platforms: [.macOS(.v14)],
    dependencies: [
        // Pinned: RevFly downloads the model files this version loads (see parakeet_ane.rs).
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.18.0")
    ],
    targets: [
        .executableTarget(
            name: "revfly-asr",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")],
            path: "Sources"
        )
    ]
)
