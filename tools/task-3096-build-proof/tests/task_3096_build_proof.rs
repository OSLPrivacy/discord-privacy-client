use std::process::{Command, Output};

use serde_json::Value;

const FINGERPRINT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const DEVICE: &str = "device:qa-laptop-3096";
const PERSON: &str = "person:liam-3096";
const MADE_AT: &str = "1786399230";
const STOPS_AT: &str = "1789077630";

const FLAGS: [(&str, &str); 5] = [
    ("--build-fingerprint", FINGERPRINT),
    ("--device-id", DEVICE),
    ("--person-id", PERSON),
    ("--made-at-unix-seconds", MADE_AT),
    ("--stops-counting-at-unix-seconds", STOPS_AT),
];

fn invoke(omitted: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_osl-build-proof"));
    for (flag, value) in FLAGS {
        if omitted != Some(flag) {
            command.args([flag, value]);
        }
    }
    command.output().expect("run osl-build-proof")
}

#[test]
fn direct_command_binds_all_five_values_into_one_proof() {
    let output = invoke(None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let proof: Value =
        serde_json::from_slice(&output.stdout).expect("stdout is exactly one JSON proof");

    assert_eq!(proof["buildFingerprint"], FINGERPRINT);
    assert_eq!(proof["deviceId"], DEVICE);
    assert_eq!(proof["personId"], PERSON);
    assert_eq!(proof["madeAtUnixSeconds"], MADE_AT.parse::<u64>().unwrap());
    assert_eq!(
        proof["stopsCountingAtUnixSeconds"],
        STOPS_AT.parse::<u64>().unwrap()
    );
    assert_eq!(proof.as_object().map(|object| object.len()), Some(5));

    println!(
        "TASK3096_PROOF bound_value_count=5 proof_count=1 build_fingerprint={} device={} person={} made_at_unix_seconds={} stops_counting_at_unix_seconds={}",
        proof["buildFingerprint"].as_str().unwrap(),
        proof["deviceId"].as_str().unwrap(),
        proof["personId"].as_str().unwrap(),
        proof["madeAtUnixSeconds"].as_u64().unwrap(),
        proof["stopsCountingAtUnixSeconds"].as_u64().unwrap(),
    );
}

#[test]
fn direct_command_refuses_each_of_the_five_missing_values() {
    let mut refused = 0;
    for (flag, _) in FLAGS {
        let output = invoke(Some(flag));
        assert_eq!(
            output.status.code(),
            Some(2),
            "omitting {flag} unexpectedly succeeded"
        );
        assert!(output.stdout.is_empty(), "omitting {flag} printed a proof");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("missing required"),
            "wrong error for {flag}: {stderr}"
        );
        assert!(stderr.contains(flag), "error did not name {flag}: {stderr}");
        refused += 1;
        println!("TASK3096_MISSING flag={flag} exit=2 proof_count=0");
    }
    assert_eq!(refused, 5);
    println!("TASK3096_MISSING_SUMMARY refused=5 required_values=5");
}

#[test]
fn command_refuses_values_that_are_present_but_not_valid_bindings() {
    let invalid_cases = [
        ("--build-fingerprint", "abc"),
        ("--device-id", ""),
        ("--person-id", "   "),
        ("--made-at-unix-seconds", "not-a-time"),
        ("--stops-counting-at-unix-seconds", MADE_AT),
    ];
    for (changed_flag, replacement) in invalid_cases {
        let mut command = Command::new(env!("CARGO_BIN_EXE_osl-build-proof"));
        for (flag, value) in FLAGS {
            command.args([
                flag,
                if flag == changed_flag {
                    replacement
                } else {
                    value
                },
            ]);
        }
        let output = command.output().expect("run invalid proof command");
        assert_eq!(
            output.status.code(),
            Some(2),
            "invalid {changed_flag} unexpectedly succeeded"
        );
        assert!(
            output.stdout.is_empty(),
            "invalid {changed_flag} printed a proof"
        );
    }
}
