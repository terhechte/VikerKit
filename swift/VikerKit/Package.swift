// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "VikerKit",
    platforms: [
        .iOS(.v15),
        .macOS(.v13)
    ],
    products: [
        .library(name: "VikerKit", targets: ["VikerKit"])
    ],
    targets: [
        .target(
            name: "VikerKit",
            dependencies: ["VikerKitFFI"],
            path: "Sources/VikerKit"
        ),
        .binaryTarget(
            name: "VikerKitFFI",
            path: "VikerKitFFI.xcframework"
        )
    ]
)
