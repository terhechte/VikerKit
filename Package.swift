// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "Viker",
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
            path: "swift/VikerKit/Sources/VikerKit"
        ),
        .systemLibrary(
            name: "CLibgit2",
            path: "swift/VikerKit/Sources/CLibgit2",
            pkgConfig: "libgit2",
            providers: [
                .brew(["libgit2"]),
                .apt(["libgit2-dev"])
            ]
        ),
        .binaryTarget(
            name: "VikerKitFFI",
            url: "https://github.com/terhechte/VikerKit/releases/download/0.1.4/VikerKitFFI.xcframework.zip",
            checksum: "d4ba03b4b08555c8b89668b6aa52548bc6571f2a9937baa0ccb4a5128de32012"
        )
    ]
)
