// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "PausIOWatch",
    defaultLocalization: "en",
    platforms: [.macOS(.v14)],
    products: [.library(name: "PausIOWatch", targets: ["PausIOWatch"])],
    targets: [
        .target(name: "PausIOWatch", path: "Sources/PausIOWatch", exclude: ["PausIOWatch.entitlements"]),
        .testTarget(name: "PausIOWatchTests", dependencies: ["PausIOWatch"], path: "Tests/PausIOWatchTests")
    ]
)
