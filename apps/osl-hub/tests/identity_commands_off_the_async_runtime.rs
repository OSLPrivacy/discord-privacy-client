//! NEW-3 — no identity-registry command may run the account lifecycle on the
//! Tauri async runtime.
//!
//! `list_hub_identities` looks like a getter, which is why it was the one that
//! did not hop. It is not a getter: on a first run it performs the whole
//! flat-account migration — every account artifact moved, the SQLite message
//! store closed and its directory renamed, then `run_autostart`, which
//! re-registers the identity against the key server over blocking HTTP. All of
//! that ran on the runtime every other IPC command shares.
//!
//! This is a source contract because there is no way to assert it from a test
//! process: `spawn_blocking` needs a Tauri `AppHandle`.

use std::path::Path;

fn command_body(source: &str, name: &str) -> String {
    let signature = format!("async fn {name}(");
    let start = source
        .find(&signature)
        .unwrap_or_else(|| panic!("{name} should be an async tauri command"));
    let rest = &source[start..];
    let end = rest
        .find("\n}\n")
        .unwrap_or_else(|| panic!("{name} should have a body"));
    rest[..end].to_owned()
}

#[test]
fn every_identity_registry_command_hops_off_the_async_runtime() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"))
        .expect("read the hub command surface");

    for command in [
        "list_hub_identities",
        "create_hub_identity_slot",
        "recover_hub_identity_slot",
    ] {
        let body = command_body(&source, command);
        assert!(
            body.contains("spawn_blocking"),
            "{command} runs the account lifecycle on the Tauri async runtime; \
             it must hop through tauri::async_runtime::spawn_blocking like its siblings",
        );
        assert!(
            body.contains("identity_registry::"),
            "{command} should still call into the identity registry",
        );
    }
}
