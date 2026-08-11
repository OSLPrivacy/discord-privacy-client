use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use task_5123_x_composer_contract::{
    audit_repository, default_repo_root, finding_map, CONTRACT_RELATIVE_PATH, GEOMETRY_KEYS,
    STYLE_KEYS, TYPE_KEYS,
};

const CHECK: &str = env!("CARGO_BIN_EXE_task-5123-x-composer-contract");
const CONTRACT_BYTES: &[u8] = include_bytes!("../fixtures/x-protected-composer-contract.json");

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

struct FixtureRepo {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    contract: PathBuf,
}

impl FixtureRepo {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().to_path_buf();
        write(&root, "apps/osl-hub/src/lib.rs", "pub mod adapters;\n");
        write(
            &root,
            "apps/osl-hub-ui/src/main.ts",
            "const carrier = 'native_discord_overlay';\n",
        );
        write(&root, "src-tauri/src/main.rs", "fn main() {}\n");
        write(&root, "crates/ipc/src/lib.rs", "pub mod commands;\n");
        write(
            &root,
            "scripts/build-release-installer.mjs",
            "export function buildReleaseInstaller() {}\n",
        );
        write(&root, "apps/osl-hub/Cargo.toml", "[package]\nname='hub'\n");
        write(
            &root,
            "apps/osl-hub/tauri.conf.json",
            "{\"bundle\":{\"active\":true}}\n",
        );
        write(
            &root,
            ".github/workflows/osl-hub-release.yml",
            "name: release\n",
        );
        write(
            &root,
            "apps/osl-hub/capabilities/hub.json",
            "{\"permissions\":[\"allow-set-native-discord-protected-overlay-open\"]}\n",
        );
        write(
            &root,
            "apps/osl-hub/permissions/hub.toml",
            "identifier='hub'\n",
        );
        write(
            &root,
            "src-tauri/capabilities/main.json",
            "{\"identifier\":\"main\"}\n",
        );
        write(
            &root,
            "src-tauri/permissions/main.toml",
            "identifier='main'\n",
        );
        write(&root, "data/pricing.json", "{\"capability_registry\":[]}\n");
        write(
            &root,
            "data/public-surface-manifest.json",
            "{\"schema_version\":1}\n",
        );
        let contract = root.join(CONTRACT_RELATIVE_PATH);
        fs::create_dir_all(contract.parent().unwrap()).unwrap();
        fs::write(&contract, CONTRACT_BYTES).unwrap();
        Self {
            _temporary: temporary,
            root,
            contract,
        }
    }

    fn run(&self) -> Output {
        Command::new(CHECK)
            .args(["--root", self.root.to_str().unwrap()])
            .output()
            .expect("run task 5123 gate")
    }

    fn run_with_contract(&self, contract: &Path) -> Output {
        Command::new(CHECK)
            .args([
                "--root",
                self.root.to_str().unwrap(),
                "--contract",
                contract.to_str().unwrap(),
            ])
            .output()
            .expect("run task 5123 gate with mutant contract")
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn unchanged_check_validates_nonempty_contract_and_both_zero_x_inventories() {
    let root = default_repo_root();
    let contract = root.join(CONTRACT_RELATIVE_PATH);
    let report = audit_repository(&root, &contract).unwrap();
    assert_eq!(report.geometry_keys, 11);
    assert_eq!(report.type_keys, 10);
    assert_eq!(report.style_keys, 14);
    assert_eq!(report.controls, 7);
    assert!(report.production.scanned_files > 100);
    assert!(report.installed.installer_files > 0);
    assert!(report.installed.action_files > 0);
    assert!(report.installed.release_manifest_files > 0);
    assert!(finding_map(&report).values().all(|count| *count == 0));

    let output = Command::new(CHECK)
        .args(["--root", root.to_str().unwrap()])
        .output()
        .expect("run unchanged task 5123 check");
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("geometry_keys=11 type_keys=10 style_keys=14 controls=7"));
    assert!(stdout.contains("TASK5123_PRODUCTION_INVENTORY"));
    assert!(stdout.contains("imports=0 painters=0 actions=0 release_capability_rows=0"));
    assert!(stdout.contains("TASK5123_INSTALLED_INVENTORY"));
    assert!(stdout.contains("TASK5123_FINISH contract_only=true x_shippable=false"));
    println!("{stdout}");
}

#[test]
fn every_required_geometry_type_and_style_key_is_named_when_starved() {
    let fixture = FixtureRepo::new();
    for (group, keys) in [
        ("geometry", GEOMETRY_KEYS.as_slice()),
        ("type", TYPE_KEYS.as_slice()),
        ("style", STYLE_KEYS.as_slice()),
    ] {
        for key in keys {
            let mut contract: Value = serde_json::from_slice(CONTRACT_BYTES).unwrap();
            contract[group].as_object_mut().unwrap().remove(*key);
            let mutant = fixture.root.join(format!("missing-{group}-{key}.json"));
            fs::write(&mutant, serde_json::to_vec_pretty(&contract).unwrap()).unwrap();
            let output = fixture.run_with_contract(&mutant);
            assert_eq!(output.status.code(), Some(1));
            let failure = stderr(&output);
            assert!(
                failure.contains(&format!("missing {group}.{key}")),
                "wrong failure for {group}.{key}: {failure}"
            );
        }
    }
    println!(
        "TASK5123_STARVED_KEYS_REJECTED geometry={} type={} style={}",
        GEOMETRY_KEYS.len(),
        TYPE_KEYS.len(),
        STYLE_KEYS.len()
    );
}

#[test]
fn empty_contract_and_starved_inventories_exit_one_by_name() {
    let fixture = FixtureRepo::new();
    fs::write(&fixture.contract, "").unwrap();
    let output = fixture.run();
    assert_eq!(output.status.code(), Some(1));
    let empty_failure = stderr(&output);
    assert!(empty_failure.contains("X composer contract is empty"));
    println!("TASK5123_MUTANT empty-contract exit=1 error={empty_failure:?}");
    fs::write(&fixture.contract, CONTRACT_BYTES).unwrap();

    fs::remove_file(fixture.root.join("apps/osl-hub/src/lib.rs")).unwrap();
    let output = fixture.run();
    assert_eq!(output.status.code(), Some(1));
    let production_failure = stderr(&output);
    assert!(production_failure.contains("production inventory starved"));
    println!("TASK5123_MUTANT starved-production exit=1 error={production_failure:?}");
    write(
        &fixture.root,
        "apps/osl-hub/src/lib.rs",
        "pub mod adapters;\n",
    );

    fs::remove_file(fixture.root.join("scripts/build-release-installer.mjs")).unwrap();
    let output = fixture.run();
    assert_eq!(output.status.code(), Some(1));
    let installed_failure = stderr(&output);
    assert!(installed_failure.contains("installed inventory starved"));
    println!("TASK5123_MUTANT starved-installed exit=1 error={installed_failure:?}");
    println!("TASK5123_EMPTY_CONTRACT=REJECTED INVENTORY_STARVATION_SCOPES=2");
}

#[test]
fn every_shipping_scope_breach_exits_one_and_names_its_scope() {
    let cases = [
        (
            "apps/osl-hub/src/contract_import.rs",
            "use task_5123_x_composer_contract::x_protected_composer_contract;\n",
            "production scope breach: X imports=1",
        ),
        (
            "apps/osl-hub/src/x_painter.rs",
            "struct XProtectedComposerPainter;\n",
            "production scope breach: X runtime painters=1",
        ),
        (
            "apps/osl-hub/Cargo.toml",
            "[package]\nname='hub'\n# package x-protected-composer-painter\n",
            "installed scope breach: X runtime painters=1",
        ),
        (
            "apps/osl-hub/capabilities/hub.json",
            "{\"permissions\":[\"allow-set-native-discord-protected-overlay-open\",\"allow-open-x-protected-composer\"]}\n",
            "installed scope breach: X installed actions=1",
        ),
        (
            "data/pricing.json",
            "{\"capability_registry\":[{\"id\":\"x-protected-composer\"}]}\n",
            "installed scope breach: X release-capability rows=1",
        ),
    ];

    for (relative, body, expected) in cases {
        let fixture = FixtureRepo::new();
        write(&fixture.root, relative, body);
        let output = fixture.run();
        assert_eq!(output.status.code(), Some(1), "mutant passed: {relative}");
        let failure = stderr(&output);
        assert!(
            failure.contains(expected),
            "{relative} did not name {expected:?}: {failure}"
        );
        println!(
            "TASK5123_MUTANT path={relative} exit=1 error={:?}",
            failure.trim()
        );
    }
    println!("TASK5123_SCOPE_BREACH_MUTANTS_REJECTED=5");
}
