use std::{collections::BTreeSet, process::Command};

const ATTACKS: &[(&str, &str)] = &[
    ("route-unreachable", "missing route"),
    ("bypass-reauthorization", "reauthorization"),
    ("skip-archive-save", "archive native save cancelled"),
    ("skip-key-save", "key native save cancelled"),
    ("remove-warning", "exact warning missing"),
    ("soften-warning", "exact warning missing"),
    (
        "remove-independent-copy-warning",
        "independent-copy sentence",
    ),
    ("bypass-full-readback", "false receipt"),
    ("manifest-sample-only", "integrity failure"),
    ("skip-selected-block", "reordered authenticated block"),
    ("skip-final-block", "unread authenticated final block"),
    (
        "success-after-key-deleted",
        "false receipt after deleting only key",
    ),
    ("archive-cancel", "archive native save cancelled"),
    ("key-cancel", "key native save cancelled"),
    ("archive-write-failure", "archive failed write"),
    ("key-write-failure", "key failed write"),
    ("disk-full", "disk-full after OS reported success"),
    ("short-write", "short-write after OS reported success"),
    (
        "torn-final-block",
        "torn-final-block after OS reported success",
    ),
    (
        "post-write-corruption",
        "post-write-corruption after OS reported success",
    ),
    ("lost-key", "lost key"),
    ("unreadable-key", "unreadable key"),
    ("unreadable-archive", "unreadable archive"),
    ("stop-after-first-page", "page boundary"),
    ("truncate-message-41", "message 41/messages-041"),
    ("drop-attachment-7", "attachment 7/attachments-007"),
    ("drop-production-class", "missing class: activity_receipts"),
    ("osl-held-key", "unavailable key"),
    ("missing-format-field", "missing required format field"),
    ("undocumented-field", "undocumented field"),
    ("foreign-owner", "foreign owner"),
    ("flip-ciphertext-bit", "integrity failure"),
    (
        "reorder-authenticated-blocks",
        "reordered authenticated block",
    ),
    (
        "truncate-authenticated-final-block",
        "unread authenticated final block",
    ),
    ("nonce-reuse", "nonce reuse"),
    ("wrong-key", "integrity failure"),
];

fn probe() -> &'static str {
    env!("CARGO_BIN_EXE_task_5200b_probe")
}

#[test]
fn every_throwaway_attack_exits_one_and_the_complete_journey_stays_portable() {
    let baseline = Command::new(probe()).arg("baseline").output().unwrap();
    assert_eq!(
        baseline.status.code(),
        Some(0),
        "baseline stderr: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let baseline_text = String::from_utf8_lossy(&baseline.stdout);
    assert!(baseline_text.contains("files_saved=2"), "{baseline_text}");
    assert!(
        baseline_text.contains("wrong_key_released=0"),
        "{baseline_text}"
    );
    assert!(
        baseline_text.contains("tamper_released=0"),
        "{baseline_text}"
    );
    assert!(baseline_text.contains("discarded=2"), "{baseline_text}");
    print!("{baseline_text}");

    let omitted = std::env::var("TASK5200B_OMIT_ATTACK").ok();
    if let Some(ref name) = omitted {
        assert!(
            ATTACKS.iter().any(|(attack, _)| attack == name),
            "unknown TASK5200B_OMIT_ATTACK={name}"
        );
    }
    let mut executed = BTreeSet::new();
    for (attack, required) in ATTACKS {
        if omitted.as_deref() == Some(*attack) {
            continue;
        }
        let output = Command::new(probe())
            .args(["attack", attack])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "attack {attack} did not exit 1; stdout={} stderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(stderr.contains("TASK5200B_REJECT"), "{attack}: {stderr}");
        assert!(stderr.contains(required), "{attack}: {stderr}");
        assert!(
            stderr.contains("plaintext_released=0"),
            "{attack}: {stderr}"
        );
        assert!(stderr.contains("discarded=true"), "{attack}: {stderr}");
        executed.insert(*attack);
        print!("{stderr}");
    }
    for (required, _) in ATTACKS {
        assert!(
            executed.contains(required),
            "absent attack: {required}; red proof requires every generated mutant"
        );
    }
    println!(
        "TASK5200B_FINISH baseline=1 attacks={}/{} exit_one={} plaintext_released=0 throwaway_sets_discarded={} missing_attack_checks={} warning_exact=true offline_without_osl=true",
        executed.len(),
        ATTACKS.len(),
        executed.len(),
        executed.len(),
        ATTACKS.len(),
    );
}
