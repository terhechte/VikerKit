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
            url: "https://github.com/terhechte/VikerKit/releases/download/0.1.10/VikerKitFFI.xcframework.zip",
            checksum: "c1198487da65de049bda6d5e712509452320af14abada915ef0227be2ffa3285"
        )
    ]
)
