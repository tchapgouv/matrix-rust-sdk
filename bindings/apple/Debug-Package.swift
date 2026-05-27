// swift-tools-version:5.7

// A package manifest for local development. This file will be copied
// into the root of the repo when generating an XCFramework.

import PackageDescription

let package = Package(
    name: "MatrixRustSDK",
    platforms: [
        .iOS(.v16),
        .macOS(.v12)
    ],
    products: [
        .library(name: "MatrixRustSDK",
                // BWI-specific
                // bwi 6881 our local build does not work on device with dynamic binding
                // type: .dynamic,
                // end BWI-specific
                targets: ["MatrixRustSDK"]),
    ],
    targets: [
        .binaryTarget(name: "MatrixSDKFFI", path: "bindings/apple/generated/MatrixSDKFFI.xcframework"),
        .target(name: "MatrixRustSDK",
                dependencies: [.target(name: "MatrixSDKFFI")],
                path: "bindings/apple/generated/swift")
    ]
)
