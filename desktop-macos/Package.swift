// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "NekoMac",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "Neko", targets: ["NekoApp"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.28.2"),
    ],
    targets: [
        // Wire client for nekod: generated protobuf, WebSocket over the Unix socket.
        .target(
            name: "NekoAPI",
            dependencies: [.product(name: "SwiftProtobuf", package: "swift-protobuf")]
        ),
        .executableTarget(
            name: "NekoApp",
            dependencies: ["NekoAPI"],
            resources: [.copy("Resources/Fonts")]
        ),
        .testTarget(name: "NekoAPITests", dependencies: ["NekoAPI", "NekoApp"]),
    ]
)
