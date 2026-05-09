// Tauri 2 build script. Invokes `tauri_build::build()`, which:
//   - parses `tauri.conf.json`,
//   - generates the `tauri::generate_context!()` inputs (resource
//     manifests, capabilities, embedded `frontendDist` files),
//   - sets `OUT_DIR` so the macro can find the generated context at
//     compile time.
//
// Without this script the macro errors with "OUT_DIR env var is not
// set, do you have a build script?".

fn main() {
    tauri_build::build()
}
