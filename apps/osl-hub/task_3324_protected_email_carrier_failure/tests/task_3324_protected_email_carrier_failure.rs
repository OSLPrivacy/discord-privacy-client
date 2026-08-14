use std::path::PathBuf;
use std::process::Command;

use task_3324_protected_email_carrier_failure::{
    inject_oracle_fault, run_shipping_oracle, verify, CARRIER_COVER, DUE_LOCATOR, NEIGHBOR_LOCATOR,
};

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("task-3324-{name}-{}", std::process::id()))
}

fn command_path() -> PathBuf {
    let mut path = PathBuf::from(env!(
        "CARGO_BIN_EXE_task-3324-protected-email-carrier-failure"
    ));
    path.set_extension(std::env::consts::EXE_EXTENSION);
    path
}

#[test]
fn task_3324_forced_email_carrier_failure_still_destroys_only_the_due_protected_object() {
    let directory = scratch("shipping");
    let outcome = run_shipping_oracle(&directory).expect("shipping oracle");
    let _ = std::fs::remove_dir_all(&directory);

    assert!(verify(&outcome).is_empty(), "{:?}", verify(&outcome));
    assert_eq!(outcome.due_part_before, 1);
    assert_eq!(outcome.due_part_after, 0);
    assert_eq!(outcome.neighbor_part_before, 1);
    assert_eq!(outcome.neighbor_part_after, 1);
    assert!(outcome.cover_before && outcome.cover_after && outcome.neighbor_cover_after);
    assert_eq!(
        outcome.carrier_delete_attempts, 0,
        "email must not call a carrier delete"
    );
    assert!(outcome.carrier_forced_failure);
    assert_eq!(outcome.records_left, vec![NEIGHBOR_LOCATOR.to_owned()]);
    assert_eq!(outcome.keep_set[0], CARRIER_COVER);
    assert!(outcome.destroy_set[0].contains(DUE_LOCATOR));
}

#[test]
fn task_3324_direct_oracle_names_the_due_destroy_set_keep_set_and_exact_email_words() {
    let output = Command::new(command_path()).output().expect("run oracle");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains(
            "TASK3324_RESULT result=PASS due_object=1->0 pointer=refused carrier_delete_count=0"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("TASK3324_SETS destroy=[\"osl-protected-part:email/mailbox:protected-email-3324/osl-object-email-3324-due\"]"), "{stdout}");
    assert!(stdout.contains("The private part stops working. The cover email stays in their inbox and cannot be recalled."), "{stdout}");
    assert!(
        stdout.contains("(\"x\", 0), (\"instagram\", 0), (\"messenger\", 0)"),
        "{stdout}"
    );
}

#[test]
fn task_3324_unchanged_check_rejects_under_deletion_over_deletion_and_scope_breaches() {
    for (fault, label) in [
        ("leave-object", "under-deletion"),
        ("delete-keep", "over-deletion"),
        ("enable-ordinary", "scope breach"),
        ("promote-adapter", "scope breach"),
    ] {
        let output = Command::new(command_path())
            .env("OSL_TASK3324_FAULT", fault)
            .output()
            .expect("run changed observation");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            !output.status.success(),
            "fault={fault} unexpectedly passed\n{stdout}"
        );
        assert!(
            stdout.contains(label),
            "fault={fault} expected {label}\n{stdout}"
        );
    }

    // The verifier itself also rejects all four independently, so the command
    // exit status is not the only assertion doing this work.
    for fault in [
        "leave-object",
        "delete-keep",
        "enable-ordinary",
        "promote-adapter",
    ] {
        let directory = scratch(fault);
        let mut outcome = run_shipping_oracle(&directory).expect("shipping oracle");
        inject_oracle_fault(&mut outcome, fault).expect("known fault");
        assert!(!verify(&outcome).is_empty(), "fault={fault}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
