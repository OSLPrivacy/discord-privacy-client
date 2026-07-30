fn main() {
    // The desktop binary embeds the already-built frontend. Rebuild when its
    // hashed assets change so a QA executable cannot silently retain the
    // previous renderer while reporting a new source-level fix.
    println!("cargo:rerun-if-changed=../osl-hub-ui/dist");
    // Core models and persistence tests do not need a native webview. Keeping
    // Tauri generation behind the desktop feature lets CI test that core on
    // hosts without GTK/WebKit development packages.
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build()
    }
}
