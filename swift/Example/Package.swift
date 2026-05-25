// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "VikerExample",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "VikerExample", targets: ["VikerExample"])
    ],
    dependencies: [
        .package(path: "../VikerKit")
    ],
    targets: [
        .executableTarget(
            name: "VikerExample",
            dependencies: [
                .product(name: "VikerKit", package: "VikerKit")
            ]
        )
    ]
)
