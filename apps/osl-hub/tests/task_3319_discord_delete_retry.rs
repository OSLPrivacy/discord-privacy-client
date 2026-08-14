#[path = "../src/task_3319_discord_delete_retry.rs"]
mod retry;

use retry::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative),
    )
    .unwrap()
}

#[derive(Default)]
struct Provider {
    open: bool,
    absent: bool,
    calls: Vec<(DiscordDeleteTarget, [String; 2])>,
}

impl DiscordExactDeleteProvider for Provider {
    fn delete_exact(
        &mut self,
        target: &DiscordDeleteTarget,
        keep: &[String; 2],
    ) -> ProviderDeleteResult {
        self.calls.push((target.clone(), keep.clone()));
        if !self.open {
            ProviderDeleteResult::Unavailable
        } else if self.absent {
            ProviderDeleteResult::ConfirmedAbsent
        } else {
            ProviderDeleteResult::Deleted
        }
    }
}

fn path(label: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!(
            "osl-task-3319-{label}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join("job")
}

fn job(delete_at: i64) -> DiscordDeleteRetryJob {
    DiscordDeleteRetryJob::arm(
        DiscordDeleteTarget {
            account_id: "owner-3319".into(),
            channel_id: "channel-3319".into(),
            message_id: "target-3319".into(),
        },
        ["keep-3319-left".into(), "keep-3319-right".into()],
        delete_at,
        CarrierEligibility::frozen_discord_no_finite_limit(),
    )
    .unwrap()
}

#[test]
fn task_3319_closed_discord_retries_exact_persisted_target_across_restart_without_a_lifetime_ceiling(
) {
    let file = path("no-limit");
    let mut persisted = job(100);
    if std::env::var_os("OSL_TASK3319_SHORT_CEILING").is_some() {
        // Deliberately-red proof seam: a stale implementation-authored ceiling
        // would abandon the target even though Discord's frozen operation has
        // no finite limit.
        persisted.eligibility = CarrierEligibility::FiniteDeadline {
            eligible_through_unix_seconds: 109,
        };
    }
    store_job_at_path(&file, &persisted).unwrap();
    let mut provider = Provider::default();
    assert_eq!(
        run_due_discord_delete(&mut persisted, 100, &mut provider).unwrap(),
        SchedulerOutcome::Retried {
            next_attempt_unix_seconds: 105,
            retry_count: 1
        }
    );
    store_job_at_path(&file, &persisted).unwrap();
    let mut restarted = load_job_at_path(&file).unwrap();
    assert_eq!(restarted.state, DeleteJobState::Retrying);
    assert_eq!(
        run_due_discord_delete(&mut restarted, 104, &mut provider).unwrap(),
        SchedulerOutcome::NotDue
    );
    assert_eq!(
        run_due_discord_delete(&mut restarted, 105, &mut provider).unwrap(),
        SchedulerOutcome::Retried {
            next_attempt_unix_seconds: 115,
            retry_count: 2
        }
    );
    // Past the old arbitrary 10-second ceiling: no-limit remains armed.
    let past_old_ceiling = run_due_discord_delete(&mut restarted, 115, &mut provider).unwrap();
    assert!(
        restarted.state.is_armed(),
        "TASK3319 short ceiling mutation must not abandon an eligible target"
    );
    assert_eq!(
        past_old_ceiling,
        SchedulerOutcome::Retried {
            next_attempt_unix_seconds: 135,
            retry_count: 3
        }
    );
    assert!(restarted.state.is_armed());
    assert_eq!(restarted.visible_status(), "Discord unavailable; retrying exact target discord/owner-3319/channel-3319/target-3319 at 135 (no-finite-limit)");
    provider.open = true;
    assert_eq!(
        run_due_discord_delete(&mut restarted, 135, &mut provider).unwrap(),
        SchedulerOutcome::Deleted
    );
    assert_eq!(
        run_due_discord_delete(&mut restarted, 200, &mut provider).unwrap(),
        SchedulerOutcome::Finished
    );
    assert_eq!(provider.calls.len(), 4);
    assert!(provider
        .calls
        .iter()
        .all(|(target, keep)| target.message_id == "target-3319"
            && keep == &["keep-3319-left".to_owned(), "keep-3319-right".to_owned()]));
    assert_eq!(protected_message_ids(&restarted).len(), 3);
    println!("TASK3319_NO_LIMIT source={} probe={} attempts={} delete_count=1 keep_ids=2 restart_armed=true old_ceiling_armed=true", DISCORD_SINGLE_DELETE_SOURCE_ID, DISCORD_BOUNDARY_PROBE_ID, provider.calls.len());
    let _ = fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn task_3319_finite_deadline_misses_once_and_warns_after_the_exact_boundary() {
    let mut finite = DiscordDeleteRetryJob::arm(
        DiscordDeleteTarget {
            account_id: "owner-3319".into(),
            channel_id: "channel-3319".into(),
            message_id: "finite-target-3319".into(),
        },
        ["finite-keep-left".into(), "finite-keep-right".into()],
        100,
        CarrierEligibility::FiniteDeadline {
            eligible_through_unix_seconds: 110,
        },
    )
    .unwrap();
    let mut provider = Provider::default();
    assert_eq!(
        run_due_discord_delete(&mut finite, 100, &mut provider).unwrap(),
        SchedulerOutcome::Retried {
            next_attempt_unix_seconds: 105,
            retry_count: 1
        }
    );
    assert_eq!(
        run_due_discord_delete(&mut finite, 110, &mut provider).unwrap(),
        SchedulerOutcome::Retried {
            next_attempt_unix_seconds: 120,
            retry_count: 2
        }
    );
    assert_eq!(
        run_due_discord_delete(&mut finite, 111, &mut provider).unwrap(),
        SchedulerOutcome::NotDue
    );
    assert_eq!(
        run_due_discord_delete(&mut finite, 120, &mut provider).unwrap(),
        SchedulerOutcome::Missed {
            recorded_at_unix_seconds: 120
        }
    );
    assert_eq!(finite.state, DeleteJobState::Missed);
    assert!(finite
        .visible_status()
        .starts_with("Warning: Discord deletion window ended"));
    assert_eq!(
        run_due_discord_delete(&mut finite, 121, &mut provider).unwrap(),
        SchedulerOutcome::Finished
    );
    println!(
        "TASK3319_FINITE boundary=110 missed_once=true warning_visible=true attempts={}",
        provider.calls.len()
    );
}

#[test]
fn task_3319_refuses_tampered_target_keep_provider_or_evidence_records() {
    let file = path("tamper");
    store_job_at_path(&file, &job(100)).unwrap();
    let original = fs::read_to_string(&file).unwrap();
    for (name, from, to) in [
        ("target", "target-3319", "other-3319"),
        ("keep", "keep-3319-left", "other-keep-left"),
        (
            "provider",
            "provider=discord-single-message-delete",
            "provider=count-only-delete",
        ),
        (
            "source",
            DISCORD_SINGLE_DELETE_SOURCE_ID,
            "missing-independent-source",
        ),
        ("probe", DISCORD_BOUNDARY_PROBE_ID, "missing-boundary-probe"),
    ] {
        // String fields are hex on disk except the provider marker.  Mutating
        // bytes without re-tagging must trip the durable integrity boundary.
        let mut bytes = original.clone();
        if name == "provider" {
            bytes = bytes.replace(from, to);
        } else {
            bytes.push_str(&format!("\n{name}:{from}:{to}"));
        }
        fs::write(&file, bytes).unwrap();
        let err = load_job_at_path(&file).unwrap_err();
        assert!(
            err.contains("integrity check failed") || err.contains("malformed"),
            "{name}: {err}"
        );
    }
    println!("TASK3319_MUTATIONS target,keep,provider,source,probe=exit1");
    let _ = fs::remove_dir_all(file.parent().unwrap());
}

#[test]
fn task_3319_frozen_independent_source_and_windows_boundary_probe_are_required() {
    let source = read_repo_file("proof/research/discord-single-delete-eligibility-3319.md");
    let probe = read_repo_file("scripts/qa/task-3319-discord-delete-boundary-probe.ps1");
    assert!(source.contains("https://docs.discord.com/developers/resources/message"));
    assert!(source.contains("finiteDeadline=none"));
    assert!(source.contains(DISCORD_SINGLE_DELETE_SOURCE_ID));
    for required in [
        "tasklist.exe",
        "Windows PowerShell",
        "CopyFromScreen",
        "distinct RGB colours",
        "finiteDeadline",
        "no-finite-limit",
    ] {
        assert!(
            probe.contains(required),
            "TASK3319 missing required source/probe evidence: {required}"
        );
    }
    assert!(
        !probe.contains("ps -ef"),
        "TASK3319 rejects Linux-only observation"
    );
    println!("TASK3319_EVIDENCE source=official-discord-single-delete probe=windows-powershell-tasklist-copyfromscreen finite_deadline=none colour_floor=32");
}
