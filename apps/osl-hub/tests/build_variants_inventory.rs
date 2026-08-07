#![cfg(feature = "core")]

use std::path::{Path, PathBuf};

#[derive(Debug)]
struct BuildVariant {
    name: &'static str,
    command: &'static str,
    required_features: &'static [&'static str],
    forbidden_features: &'static [&'static str],
    compile_time_differences: &'static [&'static str],
    prevents_test_work: &'static [&'static str],
    password_screen: &'static str,
}

fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_repo_file(relative: &str) -> String {
    std::fs::read_to_string(crate_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn current_build_variants() -> Vec<BuildVariant> {
    vec![
        BuildVariant {
            name: "production desktop build",
            command: "cargo build --manifest-path apps/osl-hub/Cargo.toml --features desktop --bin osl-privacy-hub",
            required_features: &["desktop", "core", "whatsapp-qa-shell", "native-cover-writer"],
            forbidden_features: &["discord-qa-shell", "whatsapp-qa-identity"],
            compile_time_differences: &[
                "desktop compiles the Tauri binary and keeps the default core plus shipped WhatsApp protected surface",
                "discord-qa-shell is absent, so the main password gate follows the real password marker",
                "whatsapp-qa-identity is absent, so first-run account creation stays user-owned",
            ],
            prevents_test_work: &[
                "cannot run disposable Discord two-VM QA helpers or their passwordless pairing path",
                "cannot run WhatsApp lab identity auto-provisioning or offer-file pairing",
            ],
            password_screen: "available",
        },
        BuildVariant {
            name: "old testing build",
            command: "cargo build --manifest-path apps/osl-hub/Cargo.toml --features desktop,discord-qa-shell --bin osl-privacy-hub",
            required_features: &["desktop", "core", "whatsapp-qa-shell", "native-cover-writer", "discord-qa-shell"],
            forbidden_features: &["whatsapp-qa-identity"],
            compile_time_differences: &[
                "discord-qa-shell remaps config and local-data roots to discord-qa-shell-v1",
                "discord-qa-shell installs a device-bound file-storage key and disposable Discord identity",
                "discord-qa-shell compiles Discord QA commands; the password gate choice moved to password_screen_access at startup",
            ],
            prevents_test_work: &[
                "previously prevented main-password screen, unlock, and returning-user passwordRequired regression work when the runtime switch selected skip-password-screen-for-test",
                "prevents production screenshot-protection and header-proof enforcement from being accepted from this build",
                "prevents real first-run onboarding evidence because the QA identity path owns startup",
            ],
            password_screen: "runtime switch: skip-password-screen-for-test",
        },
        BuildVariant {
            name: "WhatsApp lab identity build",
            command: "cargo build --manifest-path apps/osl-hub/Cargo.toml --features desktop,whatsapp-qa-identity --bin osl-privacy-hub",
            required_features: &["desktop", "core", "whatsapp-qa-shell", "native-cover-writer", "whatsapp-qa-identity"],
            forbidden_features: &["discord-qa-shell"],
            compile_time_differences: &[
                "whatsapp-qa-identity implies whatsapp-qa-shell and enables the disposable WhatsApp lab identity startup hook",
                "the startup hook creates or opens a machine password and zeroizes the recovery phrase",
                "the offer-file pairing exchange is compiled only for the lab identity build",
            ],
            prevents_test_work: &[
                "prevents real first-run onboarding, recovery phrase, and user password setup evidence",
                "prevents treating WhatsApp two-VM pairing as evidence for the production install path",
            ],
            password_screen: "bypassed by lab identity startup",
        },
    ]
}

fn manifest_feature_line(manifest: &str, feature: &str) -> String {
    manifest
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(feature) && line.contains('='))
        .unwrap_or_else(|| panic!("{feature} is declared in apps/osl-hub/Cargo.toml"))
        .to_owned()
}

fn default_features(manifest: &str) -> Vec<String> {
    manifest_feature_line(manifest, "default")
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn print_variant_inventory() {
    let variants = current_build_variants();
    println!("BUILD VARIANT COUNT: {}", variants.len());
    for (index, variant) in variants.iter().enumerate() {
        println!("{}. {}", index + 1, variant.name);
        println!("   command: {}", variant.command);
        println!(
            "   compile-time differences: {}",
            variant.compile_time_differences.join("; ")
        );
        println!(
            "   prevents test work: {}",
            variant.prevents_test_work.join("; ")
        );
        println!("   password screen: {}", variant.password_screen);
    }
}

#[test]
fn prints_one_named_list_of_the_three_current_build_variants() {
    let variants = current_build_variants();

    assert_eq!(
        variants.len(),
        3,
        "the inventory must list exactly three builds"
    );
    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.name)
            .collect::<Vec<_>>(),
        [
            "production desktop build",
            "old testing build",
            "WhatsApp lab identity build",
        ],
        "an unnamed or renamed build makes the list check fail"
    );

    for variant in &variants {
        assert!(
            !variant.name.trim().is_empty(),
            "build name must not be empty"
        );
        assert_ne!(variant.name, "unnamed", "build name must not be unnamed");
        assert!(
            !variant.compile_time_differences.is_empty(),
            "{} must record compile-time differences",
            variant.name
        );
        assert!(
            !variant.prevents_test_work.is_empty(),
            "{} must record prevented test work",
            variant.name
        );
    }

    let old_testing = variants
        .iter()
        .find(|variant| variant.name == "old testing build")
        .expect("old testing build is named");
    assert_eq!(
        old_testing.password_screen, "runtime switch: skip-password-screen-for-test",
        "the old testing build must identify the password screen's old unavailable behavior by its runtime switch value"
    );

    print_variant_inventory();
}

#[test]
fn inventory_matches_current_feature_declarations_and_cfg_branches() {
    let manifest = read_repo_file("Cargo.toml");
    let defaults = default_features(&manifest);
    assert!(defaults.contains(&"core".to_owned()));
    assert!(defaults.contains(&"whatsapp-qa-shell".to_owned()));
    assert!(defaults.contains(&"native-cover-writer".to_owned()));
    assert!(!defaults.contains(&"discord-qa-shell".to_owned()));
    assert!(!defaults.contains(&"whatsapp-qa-identity".to_owned()));

    assert_eq!(manifest_feature_line(&manifest, "desktop"), "desktop = [\"dep:runtime\", \"dep:tauri\", \"dep:tauri-plugin-dialog\", \"dep:tauri-plugin-single-instance\", \"dep:tauri-plugin-updater\", \"dep:tokio\", \"core\"]");
    assert_eq!(
        manifest_feature_line(&manifest, "native-cover-writer"),
        "native-cover-writer = [\"cover-ai/local-model\"]"
    );
    assert_eq!(
        manifest_feature_line(&manifest, "discord-qa-shell"),
        "discord-qa-shell = []"
    );
    assert_eq!(
        manifest_feature_line(&manifest, "whatsapp-qa-shell"),
        "whatsapp-qa-shell = []"
    );
    assert_eq!(
        manifest_feature_line(&manifest, "whatsapp-qa-identity"),
        "whatsapp-qa-identity = [\"whatsapp-qa-shell\"]"
    );

    let core_bridge = read_repo_file("src/core_bridge.rs");
    assert!(
        core_bridge.contains("state.runtime_switches().password_screen_access")
            && core_bridge.contains("PASSWORD_SCREEN_ACCESS_SKIP_FOR_TEST"),
        "the old password-screen build choice must now read the startup runtime switch"
    );
    assert!(
        !core_bridge
            .contains("let password_gate_required = if cfg!(feature = \"discord-qa-shell\")"),
        "the password screen must not be selected by the old testing build feature"
    );

    let main_rs = read_repo_file("src/main.rs");
    for required in [
        "#[cfg(feature = \"discord-qa-shell\")]\n        let config_dir = if config_dir",
        "#[cfg(feature = \"discord-qa-shell\")]\n        osl_privacy_hub::discord_qa_identity::install_device_bound_storage_key",
        "#[cfg(feature = \"discord-qa-shell\")]\n        osl_privacy_hub::discord_qa_identity::ensure_disposable_identity",
        "#[cfg(feature = \"whatsapp-qa-identity\")]\nfn bootstrap_whatsapp_qa_device_identity",
        "#[cfg(feature = \"whatsapp-qa-identity\")]\n        bootstrap_whatsapp_qa_device_identity",
        "#[cfg(feature = \"whatsapp-qa-identity\")]\n        osl_privacy_hub::whatsapp_qa_pairing::publish_and_consume",
    ] {
        assert!(main_rs.contains(required), "missing cfg branch: {required}");
    }

    let variants = current_build_variants();
    for variant in &variants {
        for feature in variant.required_features {
            assert!(
                manifest.contains(&format!("{feature} = ["))
                    || manifest.contains(&format!("{feature} = []")),
                "{} names missing required feature {feature}",
                variant.name
            );
        }
        for forbidden in variant.forbidden_features {
            assert!(
                !variant.required_features.contains(forbidden),
                "{} both requires and forbids {forbidden}",
                variant.name
            );
        }
    }
}
