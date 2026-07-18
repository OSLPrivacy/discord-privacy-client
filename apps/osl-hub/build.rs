fn main() {
    // Core models and persistence tests do not need a native webview. Keeping
    // Tauri generation behind the desktop feature lets CI test that core on
    // hosts without GTK/WebKit development packages.
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build()
    }
}
