use release_trust::{verify_repository, TrustError};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const NOW: u64 = 1_786_435_200;
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn copy() -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release-trust");
        let destination = std::env::temp_dir().join(format!(
            "osl-task-5169-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        copy_tree(&source, &destination);
        Self(destination)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn mutate_json(path: &Path, mutation: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    mutation(&mut value);
    fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

#[test]
fn task_5169_complete_role_chain_verifies_all_three_artifacts() {
    let trust = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release-trust");
    let report = verify_repository(&trust, NOW).expect("committed release trust must verify");

    assert_eq!(report.root_keys, 3);
    assert_eq!(report.root_threshold, 2);
    assert_eq!(
        report.verified_roles,
        [
            "root",
            "timestamp",
            "snapshot",
            "targets",
            "update",
            "build-proof",
            "carrier-table"
        ]
    );
    assert_eq!(report.artifacts.len(), 3);
    assert_eq!(
        report
            .artifacts
            .iter()
            .map(|artifact| artifact.role.as_str())
            .collect::<Vec<_>>(),
        ["update", "build-proof", "carrier-table"]
    );
    println!(
        "TASK5169 root_keys={} threshold={} artifacts={} roles={}",
        report.root_keys,
        report.root_threshold,
        report.artifacts.len(),
        report.verified_roles.join(",")
    );
}

#[test]
fn task_5169_root_names_exactly_three_keys_and_disjoint_subordinate_roles() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release-trust/metadata/root.json");
    let value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let roles = value["signed"]["roles"].as_object().unwrap();
    let root_keyids = roles["root"]["keyids"].as_array().unwrap();
    assert_eq!(roles.len(), 4);
    assert_eq!(root_keyids.len(), 3);
    assert_eq!(roles["root"]["threshold"], 2);

    let mut all = std::collections::BTreeSet::new();
    for role in ["root", "targets", "snapshot", "timestamp"] {
        for keyid in roles[role]["keyids"].as_array().unwrap() {
            assert!(
                all.insert(keyid.as_str().unwrap()),
                "role key reused: {keyid}"
            );
        }
    }
    assert_eq!(all.len(), 6);
    let targets_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release-trust/metadata/targets.json");
    let targets: Value = serde_json::from_slice(&fs::read(targets_path).unwrap()).unwrap();
    let delegated_keys = targets["signed"]["delegations"]["keys"]
        .as_object()
        .unwrap();
    assert_eq!(delegated_keys.len(), 3);
    for keyid in delegated_keys.keys() {
        assert!(
            all.insert(keyid),
            "delegation reused a top-level key: {keyid}"
        );
    }
    assert_eq!(all.len(), 9);
    println!(
        "TASK5169 root_role_keys=3 threshold=2 distinct_top_level_role_keys=6 distinct_all_role_keys=9"
    );
}

#[test]
fn task_5169_wrong_role_root_signature_is_refused_by_name() {
    let fixture = Fixture::copy();
    let root_path = fixture.path().join("metadata/root.json");
    mutate_json(&root_path, |root| {
        let targets_keyid = root["signed"]["roles"]["targets"]["keyids"][0]
            .as_str()
            .unwrap()
            .to_owned();
        root["signatures"][0]["keyid"] = Value::String(targets_keyid);
    });
    let error = verify_repository(fixture.path(), NOW).unwrap_err();
    assert!(matches!(&error, TrustError::WrongRole { role, .. } if role == "root"));
    let text = error.to_string();
    assert!(text.contains("not authorized for role root"));
    println!("TASK5169 wrong_role_refusal={text}");
}

#[test]
fn task_5169b_starving_second_root_signature_refuses_threshold() {
    let fixture = Fixture::copy();
    let root_path = fixture.path().join("metadata/root.json");
    mutate_json(&root_path, |root| {
        root["signatures"].as_array_mut().unwrap().truncate(1);
    });
    let error = verify_repository(fixture.path(), NOW).unwrap_err();
    assert!(matches!(
        error,
        TrustError::Threshold {
            role,
            valid: 1,
            threshold: 2
        } if role == "root"
    ));
    println!("TASK5169b starved_root_signatures=1 required=2 refused=true");
}

#[test]
fn task_5169_artifact_tamper_is_refused_through_delegated_role() {
    let fixture = Fixture::copy();
    let artifact = fixture.path().join("artifacts/carrier-table/carriers.json");
    fs::write(&artifact, b"{\"carriers\":[\"untrusted\"]}\n").unwrap();
    let error = verify_repository(fixture.path(), NOW).unwrap_err();
    assert!(
        matches!(&error, TrustError::Length { name, .. } if name == "carrier-table/carriers.json")
    );
    println!("TASK5169 artifact_tamper_refused_by=carrier-table");
}
