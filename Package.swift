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
            url: "https://github.com/terhechte/VikerKit/releases/download/0.1.1/VikerKitFFI.xcframework.zip",
            checksum: "fc973038eb46748a08bf28388697f04fde7239e69590a88156a80920ecb07ae7"
        )
    ]
)
