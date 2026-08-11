use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const LIMITS: [(&str, f64); 8] = [
    ("boundary_displacement_px_max", 0.0),
    ("baseline_displacement_px_max", 0.0),
    ("flat_fill_delta_e00_median_lt", 1.0),
    ("flat_fill_delta_e00_p99_lt", 2.3),
    ("structure_mismatch_pct_max", 0.5),
    ("tile_size_physical_px", 16.0),
    ("tile_16x16_mismatch_pct_max", 5.0),
    ("exact_probe_raw_mismatch_pct_max", 0.5),
];

#[test]
fn exact_5103_envelope_is_the_immutable_release_bar() {
    let output = run(&policy_path(), None);
    assert!(output.status.success(), "{}", combined(&output));
    let text = combined(&output);
    assert!(text.contains("limits=8 boundary=0px baseline=0px flat_median<1.0 flat_p99<2.3 structure<=0.5% tile_size=16px tile16<=5.0% exact_raw<=0.5%"), "{text}");
    assert!(
        text.contains(
            "scope=per-state-per-channel averaging=none immutable=true calibration=agent-forbidden"
        ),
        "{text}"
    );
    println!("{}", text.trim());
}

#[test]
fn starving_any_provisional_limit_exits_one_and_names_it() {
    let original: Value = serde_json::from_slice(&fs::read(policy_path()).unwrap()).unwrap();
    let temp = TempDir::new().unwrap();
    for (name, _) in LIMITS {
        let mut starved = original.clone();
        starved["limits"].as_object_mut().unwrap().remove(name);
        let path = temp.path().join(format!("missing-{name}.json"));
        fs::write(&path, serde_json::to_vec_pretty(&starved).unwrap()).unwrap();
        let output = run(&path, None);
        let text = combined(&output);
        assert_eq!(output.status.code(), Some(1), "{name}: {text}");
        assert!(
            text.contains(name),
            "missing limit was not named: {name}: {text}"
        );
        println!("starved_limit={name} exit=1 named={name}");
    }
}

#[test]
fn no_policy_value_can_be_calibrated_loosened_or_replaced() {
    let original: Value = serde_json::from_slice(&fs::read(policy_path()).unwrap()).unwrap();
    let temp = TempDir::new().unwrap();
    for (name, value) in LIMITS {
        let mut changed = original.clone();
        changed["limits"][name] = Value::from(value + 0.1);
        let path = temp.path().join(format!("changed-{name}.json"));
        fs::write(&path, serde_json::to_vec_pretty(&changed).unwrap()).unwrap();
        let output = run(&path, None);
        let text = combined(&output);
        assert_eq!(output.status.code(), Some(1), "{name}: {text}");
        assert!(
            text.contains(&format!("provisional limit {name} must remain exactly")),
            "{text}"
        );
        println!("changed_limit={name} exit=1 named={name}");
    }
}

#[test]
fn automated_agent_candidate_and_runtime_noticeability_claims_are_rejected() {
    let original: Value = serde_json::from_slice(&fs::read(calibration_path()).unwrap()).unwrap();
    let temp = TempDir::new().unwrap();
    for kind in ["automated", "agent", "candidate", "runtime_repair"] {
        let mut claim = original.clone();
        claim["observer_kind"] = Value::from(kind);
        let path = temp.path().join(format!("{kind}.json"));
        fs::write(&path, serde_json::to_vec_pretty(&claim).unwrap()).unwrap();
        let output = run(&policy_path(), Some(&path));
        let text = combined(&output);
        assert_eq!(output.status.code(), Some(1), "{kind}: {text}");
        assert!(
            text.contains("automated/agent-generated noticeability claim rejected"),
            "{text}"
        );
        println!("noticeability_claim={kind} exit=1 rejected=automated/agent-generated");
    }
}

#[test]
fn only_separately_authorized_signed_human_tightening_is_accepted() {
    let accepted = run(&policy_path(), Some(&calibration_path()));
    let accepted_text = combined(&accepted);
    assert!(accepted.status.success(), "{accepted_text}");
    assert!(accepted_text.contains("HUMAN CALIBRATION accepted authorization=human-study-2026-08-11 observer_record=observer-record-001 tightened=5 loosened=0"), "{accepted_text}");
    println!("{}", accepted_text.trim());

    let original: Value = serde_json::from_slice(&fs::read(calibration_path()).unwrap()).unwrap();
    let temp = TempDir::new().unwrap();

    let mut loosened = original.clone();
    loosened["tightened_limits"]["structure_mismatch_pct_max"] = Value::from(0.6);
    let loosened_path = write_json(temp.path(), "loosened.json", &loosened);
    let output = run(&policy_path(), Some(&loosened_path));
    let text = combined(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("may not loosen provisional limit structure_mismatch_pct_max: 0.6 > 0.5"),
        "{text}"
    );
    println!("human_loosen exit=1 named=structure_mismatch_pct_max 0.6>0.5");

    let mut replaced_localization = original.clone();
    replaced_localization["tightened_limits"]["tile_size_physical_px"] = Value::from(15.0);
    let replaced_path = write_json(
        temp.path(),
        "replaced-localization.json",
        &replaced_localization,
    );
    let output = run(&policy_path(), Some(&replaced_path));
    let text = combined(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("may not replace provisional localization tile_size_physical_px: 15 != 16"),
        "{text}"
    );
    println!("human_replace_tile_size exit=1 named=tile_size_physical_px 15!=16");

    let mut unauthorized = original.clone();
    unauthorized["authorization_signature_hex"] = Value::from("00".repeat(64));
    let unauthorized_path = write_json(temp.path(), "unauthorized.json", &unauthorized);
    let output = run(&policy_path(), Some(&unauthorized_path));
    let text = combined(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("separate authorization signature verification failed"),
        "{text}"
    );
    println!(
        "human_unauthorized exit=1 named=separate authorization signature verification failed"
    );

    let mut unsigned = original;
    unsigned["observer_signature_hex"] = Value::from("00".repeat(64));
    let unsigned_path = write_json(temp.path(), "unsigned.json", &unsigned);
    let output = run(&policy_path(), Some(&unsigned_path));
    let text = combined(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("human observer signature verification failed"),
        "{text}"
    );
    println!("human_unsigned exit=1 named=human observer signature verification failed");
}

fn policy_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("release-policy/task-5135.json")
}

fn calibration_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("release-policy/signed-human-tightening-fixture.json")
}

fn run(policy: &Path, calibration: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_carrier-release-policy"));
    command.arg(policy);
    if let Some(path) = calibration {
        command.arg(path);
    }
    command.output().unwrap()
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn write_json(directory: &Path, name: &str, value: &Value) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path
}
