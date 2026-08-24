fn main() {
    println!("cargo::rustc-check-cfg=cfg(mobile)");
    tauri_plugin::Builder::new(&[])
        .android_path("android")
        .ios_path("ios")
        .try_build()
        .expect("mobile plugin metadata should build");
}
