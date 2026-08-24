fn main() {
    // The wdio E2E bridge's capability grant lives outside `capabilities/`
    // (which tauri-build always scans) so a default build never even sees
    // permissions that only exist when the `e2e-webdriver` feature compiles
    // the wdio plugin crates in. When the feature is on, copy it into a
    // gitignored file inside `capabilities/`; otherwise make sure that file
    // is absent.
    let generated = std::path::Path::new("capabilities/wdio-desktop.generated.json");
    if std::env::var_os("CARGO_FEATURE_E2E_WEBDRIVER").is_some() {
        let _ = std::fs::copy("capabilities-e2e/wdio-desktop.json", generated);
    } else {
        let _ = std::fs::remove_file(generated);
    }
    println!("cargo:rerun-if-changed=capabilities-e2e/wdio-desktop.json");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_E2E_WEBDRIVER");
    tauri_build::build()
}
