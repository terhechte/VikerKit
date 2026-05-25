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
            dependencies: [
                "VikerKitFFI",
                .target(name: "CLibgit2", condition: .when(platforms: [.macOS]))
            ],
            path: "Sources/VikerKit"
        ),
        .systemLibrary(
            name: "CLibgit2",
            path: "Sources/CLibgit2",
            pkgConfig: "libgit2",
            providers: [
                .brew(["libgit2"]),
                .apt(["libgit2-dev"])
            ]
        ),
        .binaryTarget(
            name: "VikerKitFFI",
            path: "VikerKitFFI.xcframework"
        )
    ]
)
