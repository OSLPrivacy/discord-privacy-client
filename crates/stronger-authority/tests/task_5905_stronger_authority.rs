use std::process::Command;

fn run(mutant: Option<&str>) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_task-5905"));
    if let Some(mutant) = mutant {
        command.env("OSL_TASK_5905_MUTANT", mutant);
    }
    command.output().expect("run task-5905 verifier")
}

#[test]
fn task_5905_green_and_all_required_red_controls() {
    let green = run(None);
    let stdout = String::from_utf8_lossy(&green.stdout);
    let stderr = String::from_utf8_lossy(&green.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    assert!(green.status.success(), "green verifier failed: {stderr}");
    for required in [
        "TASK5905 stronger_messages=10 dm=4 group=3 server=3 fresh_draws=10 reused=0 person_chosen=0 deterministic=0 below_128=0",
        "TASK5905 offline_recovery_work_bits=128 forgery_work_bits=128",
        "TASK5905 finish=PASS",
    ] {
        assert!(stdout.contains(required), "missing green output: {required}");
    }

    let mutants = [
        (
            "weak_one_bit",
            "entropy_bound_bits=1 guesses=2 affected_path=dm",
        ),
        (
            "weak_20_bit",
            "entropy_bound_bits=20 guesses=4096 affected_path=group",
        ),
        ("entropy_provenance", "missing_edge=entropy_provenance"),
        ("work_factor_2^128", "missing_edge=work_factor_2^128"),
        ("production_kdf", "missing_edge=production_kdf"),
        (
            "encryption_dependency",
            "missing_edge=encryption_dependency",
        ),
        (
            "authentication_dependency",
            "missing_edge=authentication_dependency",
        ),
        ("failure_control", "missing_edge=failure_control"),
        ("named_path_dm", "missing_edge=named_path affected_path=dm"),
        (
            "named_path_group",
            "missing_edge=named_path affected_path=group",
        ),
        (
            "named_path_server",
            "missing_edge=named_path affected_path=server",
        ),
        ("width_only", "entropy_bound_bits=20"),
        ("hash_guessable", "entropy_bound_bits=20"),
        ("fixture_crypto", "missing_edge=entropy_provenance"),
        (
            "generic_failure_both_modes",
            "generic_failure_of_both_modes affected_path=ordinary",
        ),
    ];

    for (mutant, required) in mutants {
        let red = run(Some(mutant));
        let stderr = String::from_utf8_lossy(&red.stderr);
        println!(
            "TASK5905 red_control={} exit_code={} stderr={}",
            mutant,
            red.status.code().unwrap_or(-1),
            stderr.trim()
        );
        assert_eq!(red.status.code(), Some(1), "{mutant} did not exit 1");
        assert!(stderr.contains(required), "{mutant} missing: {required}");
    }
}
