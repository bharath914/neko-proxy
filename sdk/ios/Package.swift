// swift-tools-version: 6.0
import PackageDescription

// Names are placeholders (docs/sdk/docs/01-overview.md, open decisions).
let package = Package(
    name: "NekoInspector",
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "NekoInspector", targets: ["NekoInspector"]),
        .library(name: "NekoInspectorAlamofire", targets: ["NekoInspectorAlamofire"]),
        .library(name: "NekoInspectorAutoStart", targets: ["NekoInspectorAutoStart"]),
    ],
    dependencies: [
        .package(url: "https://github.com/Alamofire/Alamofire.git", from: "5.10.0"),
    ],
    targets: [
        // Real code compiles only under `#if DEBUG`. In Release the same public symbols are empty stubs.
        .target(name: "NekoInspector", swiftSettings: [.swiftLanguageMode(.v5)]),
        .target(
            name: "NekoInspectorAlamofire",
            dependencies: ["NekoInspector", .product(name: "Alamofire", package: "Alamofire")],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        // Linking this product is enough: it starts the SDK from an Objective-C `+load`.
        .target(name: "NekoInspectorAutoStart", dependencies: ["NekoInspector"]),
        .testTarget(name: "NekoInspectorTests", dependencies: ["NekoInspector"], swiftSettings: [.swiftLanguageMode(.v5)]),
    ]
)
