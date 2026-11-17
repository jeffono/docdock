fn main() {
    // Mock-app tests link Tauri's menu code, which imports comctl32 v6; only the app binary gets
    // Tauri's manifest. Without one, the test binary fails with STATUS_ENTRYPOINT_NOT_FOUND.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
    // `include_image!` in `tray.rs` reads these at compile time without telling Cargo.
    println!("cargo:rerun-if-changed=icons/tray");
    tauri_build::build()
}
