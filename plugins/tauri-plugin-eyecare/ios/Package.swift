// swift-tools-version:5.5
import PackageDescription

let package = Package(
  name: "tauri-plugin-eyecare",
  platforms: [.iOS(.v14)],
  products: [.library(name: "tauri-plugin-eyecare", type: .static, targets: ["tauri-plugin-eyecare"])],
  dependencies: [.package(name: "Tauri", path: "../.tauri/tauri-api")],
  targets: [.target(name: "tauri-plugin-eyecare", dependencies: [.byName(name: "Tauri")], path: "Sources")]
)
