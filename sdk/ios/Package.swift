// swift-tools-version: 6.0
import PackageDescription

// Names are placeholders (docs/sdk/docs/01-overview.md, open decisions).
let package = Package(
    name: "ProxyInspector",
    platforms: [.iOS(.v15), .macOS(.v12)],
    products: [
        .library(name: "ProxyInspector", targets: ["ProxyInspector"]),
        .library(name: "ProxyInspectorAlamofire", targets: ["ProxyInspectorAlamofire"]),
        .library(name: "ProxyInspectorAutoStart", targets: ["ProxyInspectorAutoStart"]),
    ],
    dependencies: [
        .package(url: "https://github.com/Alamofire/Alamofire.git", from: "5.10.0"),
    ],
    targets: [
        // Real code compiles only under `#if DEBUG`. In Release the same public symbols are empty stubs.
        .target(name: "ProxyInspector", swiftSettings: [.swiftLanguageMode(.v5)]),
        .target(
            name: "ProxyInspectorAlamofire",
            dependencies: ["ProxyInspector", .product(name: "Alamofire", package: "Alamofire")],
            swiftSettings: [.swiftLanguageMode(.v5)]
        ),
        // Linking this product is enough: it starts the SDK from an Objective-C `+load`.
        .target(name: "ProxyInspectorAutoStart", dependencies: ["ProxyInspector"]),
        .testTarget(name: "ProxyInspectorTests", dependencies: ["ProxyInspector"], swiftSettings: [.swiftLanguageMode(.v5)]),
    ]
)
