use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};
use task_5131_messenger_contract::{
    check, Attacks, COMPOSER_KEYS, CONTRACT_JSON, GEOMETRY_KEYS, INVENTORY_IDS, STYLE_KEYS,
    TYPE_KEYS,
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("tool is nested under repository tools directory")
        .to_path_buf()
}

fn run_check(environment: Option<(&str, &str)>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_task-5131-messenger-contract-check"));
    command.arg(repo_root());
    if let Some((name, value)) = environment {
        command.env(name, value);
    }
    command.output().expect("run unchanged TASK 5131 check")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

struct ShippingFixture {
    root: PathBuf,
}

impl ShippingFixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "task5131-shipping-fixture-{}-{nonce}",
            std::process::id()
        ));
        for directory in [
            "apps/osl-hub/src",
            "apps/osl-hub-ui/src",
            "apps/osl-hub/nsis",
            "apps/osl-hub/windows",
            "crates/adapter-profile/src",
            "crates/ipc/src",
            "scripts",
            "data",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in [
            "apps/osl-hub/build.rs",
            "apps/osl-hub/tauri.conf.json",
            "scripts/build-release-installer.mjs",
            "scripts/installer_recipe.py",
            "data/public-surface-manifest.json",
            "apps/osl-hub/src/service_host.rs",
            "apps/osl-hub/src/services.rs",
            "crates/adapter-profile/src/defaults_web.rs",
            "crates/adapter-profile/src/lib.rs",
        ] {
            fs::write(root.join(file), "\n").unwrap();
        }
        Self { root }
    }

    fn write(&self, relative: &str, content: &str) {
        fs::write(self.root.join(relative), content).unwrap();
    }
}

impl Drop for ShippingFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove exact TASK 5131 temporary fixture");
    }
}

#[test]
fn non_empty_contract_and_all_independent_shipping_inventories_are_green() {
    let summary = check(&repo_root(), CONTRACT_JSON, &Attacks::default())
        .expect("complete development contract and empty shipping inventories");
    assert_eq!(summary.composer_keys, COMPOSER_KEYS.len());
    assert_eq!(summary.geometry_keys, GEOMETRY_KEYS.len());
    assert_eq!(summary.type_keys, TYPE_KEYS.len());
    assert_eq!(summary.style_keys, STYLE_KEYS.len());
    for inventory in INVENTORY_IDS {
        assert_eq!(summary.inventory_counts[inventory], 0, "{inventory}");
    }
    println!(
        "TASK5131_TEST_GREEN composer_keys={} geometry_keys={} type_keys={} style_keys={}",
        summary.composer_keys, summary.geometry_keys, summary.type_keys, summary.style_keys
    );
    println!(
        "TASK5131_TEST_GREEN live_providers=0 production_imports=0 installer_files=0 release_manifest_rows=0 painters=0 installed_actions=0"
    );
    println!(
        "TASK5131_TEST_GREEN live_providers=0 imports=0 painters=0 actions=0 release_claims=0"
    );
}

#[test]
fn emptying_contract_makes_the_unchanged_check_exit_one_and_names_scope() {
    let output = run_check(Some(("TASK5131_EMPTY_CONTRACT", "1")));
    assert_eq!(output.status.code(), Some(1));
    let error = stderr(&output);
    assert!(error.contains("TASK5131_SCOPE_BREACH"), "{error}");
    assert!(error.contains("scope=composer_keys"), "{error}");
    println!("TASK5131_RED empty_contract_exit=1 error={}", error.trim());
}

#[test]
fn starving_every_required_geometry_type_style_key_is_named() {
    for (category, keys) in [
        ("geometry", GEOMETRY_KEYS.as_slice()),
        ("type", TYPE_KEYS.as_slice()),
        ("style", STYLE_KEYS.as_slice()),
    ] {
        for key in keys {
            let target = format!("{category}.{key}");
            let result = check(
                &repo_root(),
                CONTRACT_JSON,
                &Attacks {
                    starve_contract: Some(target.clone()),
                    ..Attacks::default()
                },
            );
            let error = result.expect_err("starved key must fail closed");
            assert!(error.contains(&format!("scope={target}")), "{error}");
        }
    }
    println!(
        "TASK5131_RED named_starved_contract_keys={} exit=1",
        GEOMETRY_KEYS.len() + TYPE_KEYS.len() + STYLE_KEYS.len()
    );
}

#[test]
fn starving_every_composer_key_is_named() {
    for key in COMPOSER_KEYS {
        let mut contract: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
        contract["composerKeys"]
            .as_array_mut()
            .unwrap()
            .retain(|candidate| candidate.as_str() != Some(key));
        let result = check(
            &repo_root(),
            &serde_json::to_string(&contract).unwrap(),
            &Attacks::default(),
        );
        let error = result.expect_err("starved composer key must fail closed");
        assert!(error.contains(&format!("scope={key}")), "{error}");
    }
    println!(
        "TASK5131_RED named_starved_composer_keys={} exit=1",
        COMPOSER_KEYS.len()
    );
}

#[test]
fn starving_each_shipping_inventory_makes_unchanged_check_exit_one_and_names_it() {
    for inventory in INVENTORY_IDS {
        let output = run_check(Some(("TASK5131_STARVE_INVENTORY", inventory)));
        assert_eq!(output.status.code(), Some(1), "{inventory}");
        let error = stderr(&output);
        assert!(error.contains("TASK5131_SCOPE_BREACH"), "{error}");
        assert!(error.contains(&format!("scope={inventory}")), "{error}");
        assert!(error.contains("inventory is starved"), "{error}");
    }
    println!(
        "TASK5131_RED named_starved_shipping_inventories={} exit=1",
        INVENTORY_IDS.len()
    );
}

#[test]
fn promoting_each_shipping_component_makes_unchanged_check_exit_one_and_names_it() {
    for inventory in INVENTORY_IDS {
        let output = run_check(Some(("TASK5131_PROMOTE", inventory)));
        assert_eq!(output.status.code(), Some(1), "{inventory}");
        let error = stderr(&output);
        assert!(error.contains("TASK5131_SCOPE_BREACH"), "{error}");
        assert!(error.contains(&format!("scope={inventory}")), "{error}");
        assert!(error.contains("component count=1"), "{error}");
    }
    println!(
        "TASK5131_RED named_promoted_shipping_components={} exit=1",
        INVENTORY_IDS.len()
    );
}

#[test]
fn real_source_promotions_are_found_by_each_independent_inventory() {
    let cases = [
        (
            "production_imports",
            "apps/osl-hub-ui/src/promoted.ts",
            "import { MessengerComposer } from './messenger-composer';\n",
        ),
        (
            "installer_files",
            "apps/osl-hub/build.rs",
            "const MESSENGER_COMPOSER_PROVIDER: &str = \"enabled\";\n",
        ),
        (
            "release_manifest_rows",
            "data/public-surface-manifest.json",
            "{\"messenger\": \"protected composer supported\"}\n",
        ),
        (
            "live_providers",
            "crates/adapter-profile/src/defaults_web.rs",
            "pub struct MessengerComposerProvider;\n",
        ),
        (
            "painters",
            "apps/osl-hub-ui/src/promoted.ts",
            "function renderMessengerComposerStyle() {}\n",
        ),
        (
            "installed_actions",
            "apps/osl-hub-ui/src/promoted.ts",
            "const messengerComposerAction = 'send command';\n",
        ),
    ];
    for (inventory, path, source) in cases {
        let fixture = ShippingFixture::new();
        fixture.write(path, source);
        let error = check(&fixture.root, CONTRACT_JSON, &Attacks::default())
            .expect_err("a real promoted shipping source must fail the unchanged checker");
        assert!(error.contains(&format!("scope={inventory}")), "{error}");
        assert!(error.contains("component count=1"), "{error}");
    }
    println!("TASK5131_RED real_source_promotions=6 exit=1 named_scopes=6");
}
