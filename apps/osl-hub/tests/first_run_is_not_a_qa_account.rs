//! D1: the shipped build must have no first-run account of its own.
//!
//! `whatsapp-qa-shell` was in the DEFAULT feature set, and it carried a
//! startup hook — `bootstrap_whatsapp_qa_device_identity` — that fires on
//! exactly the empty-profile case. On a real user's first launch it created
//! `identity.json`, installed a random machine password and zeroized the
//! 12-word recovery phrase before the window was drawn, so onboarding resumed
//! at step 3 and the recovery step reported "No recovery secret is available".
//! Observed on a wiped profile; rebuilding without the feature landed on
//! `welcome`.
//!
//! The fix splits the shipped WhatsApp *surface* (still default) from the
//! disposable lab *identity* (`whatsapp-qa-identity`, never default). This
//! test is the ratchet on that split. It reads the manifest and the binary's
//! source rather than running the app, because the defect is a build
//! configuration: nothing observable inside a single build can tell you which
//! features the shipping build was compiled with.

use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn manifest() -> String {
    std::fs::read_to_string(crate_root().join("Cargo.toml")).expect("read apps/osl-hub/Cargo.toml")
}

/// The feature names inside `default = [...]`.
fn default_features(manifest: &str) -> Vec<String> {
    let line = manifest
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("default") && line.contains('='))
        .expect("Cargo.toml declares a default feature set");
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

/// The lines of `main.rs`, so a gate can be read in its real context.
fn main_rs() -> Vec<String> {
    std::fs::read_to_string(crate_root().join("src").join("main.rs"))
        .expect("read apps/osl-hub/src/main.rs")
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The nearest `#[cfg(...)]` attribute above `index`, if any.
fn governing_cfg(lines: &[String], index: usize) -> Option<String> {
    lines[..index]
        .iter()
        .rev()
        .map(|line| line.trim())
        .find(|line| line.starts_with("#[cfg("))
        .map(str::to_owned)
}

fn sites_containing(lines: &[String], needle: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains(needle) && !line.trim_start().starts_with("//"))
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn the_default_feature_set_provisions_no_disposable_qa_account() {
    let manifest = manifest();
    let default = default_features(&manifest);

    // The surface ships; that is the intended half of the old feature.
    assert!(
        default.contains(&"whatsapp-qa-shell".to_owned()),
        "the WhatsApp protected surface is part of the product; default = {default:?}"
    );

    for provisioning_feature in [
        "whatsapp-qa-identity",
        "discord-qa-shell",
        "signal-qa-shell",
    ] {
        assert!(
            !default.contains(&provisioning_feature.to_owned()),
            "{provisioning_feature} provisions a disposable QA account at startup and must never \
             be a default feature: a default build is what a real user installs, and this hook \
             fires on exactly the empty-profile case. default = {default:?}"
        );
    }
}

#[test]
fn the_qa_identity_path_still_exists_behind_its_own_feature() {
    // VM QA depends on this path. The fix is a split, not a deletion: it must
    // remain buildable with `--features desktop,whatsapp-qa-identity`.
    let manifest = manifest();
    let declaration = manifest
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("whatsapp-qa-identity"))
        .expect("whatsapp-qa-identity is still a declared feature");
    assert!(
        declaration.contains("whatsapp-qa-shell"),
        "the lab identity is only useful with the surface it pairs against: {declaration}"
    );
}

#[test]
fn every_qa_identity_startup_hook_is_gated_on_the_non_default_feature() {
    let lines = main_rs();
    let gated = "whatsapp-qa-identity";

    // The definition, its call site, and the offer-file pairing exchange that
    // needs the identity the bootstrap creates. `publish_and_consume` exports
    // a friend code, so leaving it in the default set would have converted a
    // silently wrong first run into a refused startup once the bootstrap left.
    for needle in [
        "fn bootstrap_whatsapp_qa_device_identity",
        "bootstrap_whatsapp_qa_device_identity(&core",
        "whatsapp_qa_pairing::publish_and_consume(",
    ] {
        let sites = sites_containing(&lines, needle);
        assert!(!sites.is_empty(), "{needle} was not found in main.rs");
        for site in sites {
            let cfg = governing_cfg(&lines, site).unwrap_or_default();
            assert!(
                cfg.contains(gated),
                "line {} ({}) is governed by `{cfg}`, not by `{gated}`; a default build would \
                 provision a disposable account on an empty profile again",
                site + 1,
                lines[site].trim()
            );
        }
    }
}
