fn main() {
    // A promoted release may supply its signed build-hash record to embed in the
    // client. Development and CI builds deliberately embed empty bytes, which the
    // runtime reports as Unknown rather than fabricating a verified status.
    for (variable, output) in [
        ("OSL_BUILD_HASH_MANIFEST", "build-hashes.json"),
        ("OSL_BUILD_HASH_MANIFEST_SIGNATURE", "build-hashes.json.sig"),
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
        let destination = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join(output);
        match std::env::var_os(variable) {
            Some(source) => std::fs::copy(source, destination).expect("copy signed build-hash asset"),
            None => std::fs::write(destination, []).expect("write empty build-hash asset"),
        }
    }
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
