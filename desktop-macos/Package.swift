// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ProxyMac",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "Proxy", targets: ["ProxyApp"]),
    ],
    dependencies: [
        .package(url: "https://github.com/apple/swift-protobuf.git", exact: "1.28.2"),
    ],
    targets: [
        // Wire client for proxyd: generated protobuf, WebSocket over the Unix socket.
        .target(
            name: "ProxyAPI",
            dependencies: [.product(name: "SwiftProtobuf", package: "swift-protobuf")]
        ),
        .executableTarget(
            name: "ProxyApp",
            dependencies: ["ProxyAPI"],
            resources: [.copy("Resources/Fonts")]
        ),
        .testTarget(name: "ProxyAPITests", dependencies: ["ProxyAPI"]),
    ]
)
