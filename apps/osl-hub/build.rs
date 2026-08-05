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
        // Discard fs::copy's byte count so both arms are (); otherwise the
        // match arms have incompatible types (u64 vs ()) and the BUILD SCRIPT
        // fails, which stops the product binary compiling at all.
        match std::env::var_os(variable) {
            Some(source) => {
                std::fs::copy(source, destination).expect("copy signed build-hash asset");
            }
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
        refuse_without_the_embedded_frontend();
        tauri_build::build()
    }
}

/// D-267, the false-red half.
///
/// `--features desktop` embeds `../osl-hub-ui/dist` at COMPILE time, inside
/// `tauri::generate_context!()` in `src/main.rs`. With that directory absent the
/// build did not fail here: it compiled the whole crate first and then died on
/// the last line of the last unit as
///
///   error: proc macro panicked
///        --> src/main.rs:9856:16
///        = help: message: The `frontendDist` configuration is set to
///                "../osl-hub-ui/dist" but this path doesn't exist
///
/// with exit 101 -- **the same exit code, and the same "error:" shape, as a real
/// compile error in the binary.** That is why a mutation lane's control run and
/// its mutant run were indistinguishable by exit code, and why the lane had to
/// fall back to diffing error counts (control 2, mutant 3) to learn anything.
///
/// So the build script refuses FIRST, before a single unit is compiled, and says
/// what to run. The failure now names itself in one line instead of arriving
/// minutes later underneath 70+ warnings.
///
/// This REFUSES rather than building the frontend itself. Shelling out to npm
/// from a build script would put node, a lockfile install and (for `npm ci`) the
/// network on the critical path of every `cargo build` of this crate, in a
/// repository where the frontend build is a deliberate, separately graded step:
/// the release workflow builds it and then asserts the embedding contract with
/// `frontend_dist_is_embedded_after_frontend_build`. A build script that quietly
/// produced its own frontend would make that gate grade an artifact nobody
/// asked for. Naming the missing input is the whole fix; producing it is not
/// this file's job.
fn refuse_without_the_embedded_frontend() {
    let manifest = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is always set by cargo"),
    );
    let dist = manifest.join("../osl-hub-ui/dist");
    let index = dist.join("index.html");
    if index.is_file() {
        return;
    }
    let detail = if dist.is_dir() {
        "apps/osl-hub-ui/dist exists but has no index.html, so the last frontend build did not \
         finish"
    } else {
        "apps/osl-hub-ui/dist does not exist"
    };
    panic!(
        "the embedded frontend has not been built -- {detail}.\n\
         `--features desktop` embeds that directory into the binary at COMPILE time \
         (tauri::generate_context!), so this build cannot produce a hub binary without it.\n\
         Run:  (cd apps/osl-hub-ui && npm ci && npm run build)\n\
         Refusing here on purpose (D-267): without this refusal the build compiles the entire \
         crate and then fails as `error: proc macro panicked` with exit 101 -- the same exit code \
         a genuine compile error in the binary produces, which is how a mutation run's control \
         and its mutant became indistinguishable."
    );
}
