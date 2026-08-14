use std::path::PathBuf;
use std::process::ExitCode;

use task_3324_protected_email_carrier_failure::{
    inject_oracle_fault, run_shipping_oracle, verify, CARRIER_COVER, DUE_LOCATOR, EMAIL_APP,
    NEIGHBOR_LOCATOR,
};

fn scratch_directory() -> PathBuf {
    std::env::temp_dir().join(format!("task-3324-protected-email-{}", std::process::id()))
}

fn main() -> ExitCode {
    let directory = scratch_directory();
    let mut outcome = match run_shipping_oracle(&directory) {
        Ok(outcome) => outcome,
        Err(error) => {
            println!("TASK3324_RESULT result=FAIL setup={error}");
            return ExitCode::FAILURE;
        }
    };
    if let Ok(fault) = std::env::var("OSL_TASK3324_FAULT") {
        if let Err(error) = inject_oracle_fault(&mut outcome, &fault) {
            println!("TASK3324_RESULT result=FAIL setup={error}");
            return ExitCode::FAILURE;
        }
    }

    println!("TASK3324_ORACLE app={EMAIL_APP} due={DUE_LOCATOR} neighbor={NEIGHBOR_LOCATOR}");
    println!(
        "TASK3324_SETS destroy={:?} keep={:?}",
        outcome.destroy_set, outcome.keep_set
    );
    println!(
        "TASK3324_INDEPENDENT due_object={}->{} neighbor_object={}->{} pointer={:?}",
        outcome.due_part_before,
        outcome.due_part_after,
        outcome.neighbor_part_before,
        outcome.neighbor_part_after,
        outcome.due_pointer_after
    );
    println!("TASK3324_KEEP carrier_cover={} before={} after={} neighbor_cover_after={} carrier_delete_count={} forced_failure={}", CARRIER_COVER, outcome.cover_before, outcome.cover_after, outcome.neighbor_cover_after, outcome.carrier_delete_attempts, outcome.carrier_forced_failure);
    println!("TASK3324_EMAIL_WORDS={}", outcome.email_words);
    println!(
        "TASK3324_PATHS={:?} ordinary_refusal={}",
        outcome.uninstalled_timer_paths, outcome.ordinary_refusal
    );

    let failures = verify(&outcome);
    let _ = std::fs::remove_dir_all(&directory);
    if failures.is_empty() {
        println!(
            "TASK3324_RESULT result=PASS due_object=1->0 pointer=refused carrier_delete_count=0"
        );
        ExitCode::SUCCESS
    } else {
        for failure in failures {
            println!("TASK3324_FAILURE {failure}");
        }
        println!("TASK3324_RESULT result=FAIL");
        ExitCode::FAILURE
    }
}
