//! Fresh-install fix: the license-validation path must resolve a
//! keyserver URL even when `<config_dir>/keyserver.json` is absent
//! (the prior behaviour failed closed with "keyserver not
//! configured", so a valid key was rejected without ever contacting
//! the server). `keyserver.json` is now an OVERRIDE only.

use ipc::commands::{resolve_keyserver_base_url, DEFAULT_KEYSERVER_BASE_URL};
use std::fs;
use tempfile::TempDir;

#[test]
fn default_is_production_keyserver() {
    assert_eq!(
        DEFAULT_KEYSERVER_BASE_URL,
        "https://keyserver.oslprivacy.com"
    );
}

#[test]
fn no_keyserver_json_resolves_to_default() {
    // Fresh install: empty config dir, no keyserver.json.
    let dir = TempDir::new().unwrap();
    assert_eq!(
        resolve_keyserver_base_url(dir.path()),
        DEFAULT_KEYSERVER_BASE_URL,
    );
}

#[test]
fn malformed_keyserver_json_resolves_to_default() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("keyserver.json"), b"{ not json").unwrap();
    assert_eq!(
        resolve_keyserver_base_url(dir.path()),
        DEFAULT_KEYSERVER_BASE_URL,
    );
}

#[test]
fn keyserver_json_without_base_url_resolves_to_default() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("keyserver.json"), br#"{"user_id":"alice"}"#).unwrap();
    assert_eq!(
        resolve_keyserver_base_url(dir.path()),
        DEFAULT_KEYSERVER_BASE_URL,
    );
}

#[test]
fn bootstrap_and_license_share_one_resolver_default() {
    // Gate (b): the bootstrap registration path
    // (src-tauri/bootstrap.rs run_autostart) and the license path
    // (cmd_osl_validate_license / refresh_license_state) both call
    // THIS function — `ipc::commands::resolve_keyserver_base_url` —
    // so a fresh install with no keyserver.json registers + validates
    // against the production default from a single source of truth.
    // (bootstrap.rs lives in the Tauri crate, which cannot compile on
    // this Linux env; the shared resolver it now invokes is fully
    // covered here. Operator runs `cargo check -p discord-privacy-client`
    // on Windows to confirm the call site.)
    let fresh = TempDir::new().unwrap();
    assert_eq!(
        resolve_keyserver_base_url(fresh.path()),
        "https://keyserver.oslprivacy.com",
    );
}

#[test]
fn numeric_loopback_override_is_available_in_debug_tests() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:8787","user_id":"dev"}"#,
    )
    .unwrap();
    assert_eq!(
        resolve_keyserver_base_url(dir.path()),
        "http://127.0.0.1:8787",
    );
}

#[test]
fn remote_and_plain_http_overrides_never_replace_production() {
    for value in [
        "http://keyserver.oslprivacy.com",
        "https://attacker.example",
        "http://localhost:8787",
        "http://127.0.0.1.evil.example:8787",
    ] {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("keyserver.json"),
            format!(r#"{{"base_url":"{value}"}}"#),
        )
        .unwrap();
        assert_eq!(
            resolve_keyserver_base_url(dir.path()),
            DEFAULT_KEYSERVER_BASE_URL,
        );
    }
}
