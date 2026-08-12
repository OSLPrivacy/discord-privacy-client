//! TASK 6860 scenario runner.
//!
//! Drives the production `story-privacy` engine through every case the finish
//! line names and writes one JSON report. It asserts nothing itself — the
//! numbers it prints are what `tools/task-6860-story-privacy/check.py` grades,
//! so starving the implementation shows up as a wrong number rather than as a
//! missing assertion.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use story_privacy::{
    observe_viewer_records, shield_state, shield_state_for, Directory, Relationship,
    StoryAudience, StoryLifetime, StoryPrivacyClient, ViewOutcome, SHIELD_DISCLOSURE,
    SHIELD_PRIMITIVE, SHIELD_UNAVAILABLE_COPY, VIEW_RECEIPT_OFF_VIEWER_COPY,
    VIEW_RECEIPT_VIEWER_COPY,
};

const T0: i64 = 1_754_000_000_000;
const PROFILE_KEY: [u8; 32] = [0x5a; 32];

/// Four distinct viewer identities, long enough that a byte scan for them in a
/// store or a log means something.
const VIEWERS: [&str; 4] = [
    "viewer-identity-a1b2c3d4e5f60718",
    "viewer-identity-29384756abcdef01",
    "viewer-identity-fedcba9876543210",
    "viewer-identity-0f1e2d3c4b5a6978",
];

fn people() -> (Directory, BTreeMap<String, [u8; 32]>) {
    let mut directory = Directory::new();
    let mut pairwise = BTreeMap::new();
    let roster: [(&str, bool, bool); 6] = [
        ("person-stranger-77aa11", false, false),
        (VIEWERS[0], true, false),
        (VIEWERS[1], true, false),
        (VIEWERS[2], true, true),
        (VIEWERS[3], true, true),
        ("person-verified-third-90cc22", true, true),
    ];
    for (index, (person, friend, verified)) in roster.iter().enumerate() {
        directory.insert(person, Relationship { friend: *friend, verified: *verified });
        pairwise.insert(person.to_string(), [(index as u8) + 1; 32]);
    }
    (directory, pairwise)
}

fn fresh(root: &Path, name: &str) -> PathBuf {
    let dir = root.join(name);
    if dir.exists() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir).expect("scenario dir");
    dir
}

fn open_at(dir: &Path, now: i64, supported: bool) -> (StoryPrivacyClient, story_privacy::BurnReport) {
    StoryPrivacyClient::open_with_shield_support(dir, PROFILE_KEY, now, supported)
        .expect("story client opens")
}

// ---------------------------------------------------------------------------
// 1. Every default is inherited by a real story
// ---------------------------------------------------------------------------

fn defaults_inheritance(root: &Path) -> Value {
    let (directory, pairwise) = people();
    let mut cells = Vec::new();
    for audience in StoryAudience::ALL {
        for lifetime in StoryLifetime::ALL {
            let name = format!("defaults-{}-{}", audience.stable_id(), lifetime.stable_id());
            let dir = fresh(root, &name);
            let (mut client, _) = open_at(&dir, T0, false);
            client.set_default_audience(audience).expect("audience default");
            client.set_default_lifetime(lifetime).expect("lifetime default");
            let body = format!("story body for {} {}", audience.stable_id(), lifetime.label());
            let published = client
                .publish_story(
                    "story-inherit-1",
                    "author-self",
                    body.as_bytes(),
                    None,
                    &directory,
                    &pairwise,
                    T0,
                )
                .expect("publish");
            drop(client);
            // Restart before reading anything back: a default that only lives
            // in memory is not a default.
            let (client, _) = open_at(&dir, T0 + 1_000, false);
            // A story that did not survive the restart is reported as nulls
            // rather than panicking here, so the check grades the loss instead
            // of never running.
            let after_restart = client.published("story-inherit-1");
            let field = |pick: fn(&story_privacy::PublishedStory) -> Value| {
                after_restart.as_ref().map(pick).unwrap_or(Value::Null)
            };
            let defaults = client.defaults();
            cells.push(json!({
                "default_audience": audience.stable_id(),
                "default_lifetime": lifetime.stable_id(),
                "story_survived_restart": after_restart.is_some(),
                "story_audience": field(|story| json!(story.audience)),
                "story_lifetime": field(|story| json!(story.lifetime)),
                "audience_source": field(|story| json!(story.audience_source)),
                "expires_at_ms": field(|story| json!(story.expires_at_ms)),
                "expected_expires_at_ms": published.created_at_ms + lifetime.seconds() * 1000,
                "audience_size": field(|story| json!(story.audience_size)),
                "expected_audience_size": directory.members_for(audience).len(),
                "defaults_after_restart_audience": defaults.audience.stable_id(),
                "defaults_after_restart_lifetime": defaults.lifetime.stable_id(),
            }));
        }
    }
    Value::Array(cells)
}

// ---------------------------------------------------------------------------
// 2. Auto-burn at every boundary, across offline and restart
// ---------------------------------------------------------------------------

fn burn_boundaries(root: &Path) -> Value {
    let (directory, pairwise) = people();
    let mut cells = Vec::new();
    for lifetime in StoryLifetime::ALL {
        let dir = fresh(root, &format!("burn-{}", lifetime.stable_id()));
        let backup = fresh(root, &format!("burn-backup-{}", lifetime.stable_id()));
        let (mut client, _) = open_at(&dir, T0, false);
        client.set_default_audience(StoryAudience::Everyone).expect("audience");
        client.set_default_lifetime(lifetime).expect("lifetime");
        let body = format!("burn body {}", lifetime.label());
        let published = client
            .publish_story(
                "story-burn-1",
                "author-self",
                body.as_bytes(),
                None,
                &directory,
                &pairwise,
                T0,
            )
            .expect("publish");
        let retained_ciphertext = client.sealed_body("story-burn-1").unwrap_or_default();
        drop(client);
        // An offline copy of the whole store, taken while the story is live.
        for entry in std::fs::read_dir(&dir).expect("store listing") {
            let entry = entry.expect("store entry");
            if entry.path().is_file() {
                std::fs::copy(entry.path(), backup.join(entry.file_name())).expect("backup copy");
            }
        }

        let deadline = published.expires_at_ms;
        let secret = pairwise.get(VIEWERS[0]).copied().expect("secret");

        // One second before the boundary, after a restart.
        let (client, before_sweep) = open_at(&dir, deadline - 1_000, false);
        let before = client.open_story("story-burn-1", VIEWERS[0], &secret, deadline - 1_000);
        let before_bytes = before.as_ref().map(|bytes| bytes.len()).unwrap_or(0);
        let before_identical = before.as_deref() == Ok(body.as_bytes());
        drop(client);

        // The app is closed across the boundary; the next start must burn it.
        let (client, at_sweep) = open_at(&dir, deadline, false);
        let at_boundary = client.open_story("story-burn-1", VIEWERS[0], &secret, deadline);
        let burned_at_boundary = client.is_burned("story-burn-1");
        let sealed_after_burn = client.sealed_body("story-burn-1");
        drop(client);

        // A second restart, far past the deadline: still burned, not resurrected.
        let (client, later_sweep) = open_at(&dir, deadline + 86_400_000, false);
        let after_second_restart =
            client.open_story("story-burn-1", VIEWERS[0], &secret, deadline + 86_400_000);
        let still_burned = client.is_burned("story-burn-1");
        drop(client);

        // The offline backup, restored after the boundary, recovers nothing.
        let (backup_client, backup_sweep) = open_at(&backup, deadline, false);
        let restored = backup_client.open_story("story-burn-1", VIEWERS[0], &secret, deadline);
        let restored_bytes = restored.as_ref().map(|bytes| bytes.len()).unwrap_or(0);

        cells.push(json!({
            "lifetime": lifetime.stable_id(),
            "lifetime_label": lifetime.label(),
            "lifetime_seconds": lifetime.seconds(),
            "created_at_ms": published.created_at_ms,
            "expires_at_ms": deadline,
            "before_sweep_burned": before_sweep.burned.len(),
            "before_readable": before.is_ok(),
            "before_bytes": before_bytes,
            "before_byte_identical": before_identical,
            "at_sweep_burned": at_sweep.burned.len(),
            "at_boundary_readable": at_boundary.is_ok(),
            "at_boundary_bytes": at_boundary.as_ref().map(|b| b.len()).unwrap_or(0),
            "burned_at_boundary": burned_at_boundary,
            "sealed_bytes_after_burn": sealed_after_burn.map(|b| b.len()).unwrap_or(0),
            "retained_ciphertext_bytes": retained_ciphertext.len(),
            "later_sweep_burned": later_sweep.burned.len(),
            "later_already_burned": later_sweep.already_burned,
            "after_second_restart_readable": after_second_restart.is_ok(),
            "still_burned": still_burned,
            "restored_backup_sweep_burned": backup_sweep.burned.len(),
            "restored_backup_readable": restored.is_ok(),
            "restored_backup_bytes": restored_bytes,
        }));
    }
    Value::Array(cells)
}

// ---------------------------------------------------------------------------
// 3. Per-story SEND TO overrides the default
// ---------------------------------------------------------------------------

fn send_to_override(root: &Path) -> Value {
    let (mut directory, pairwise) = people();
    let dir = fresh(root, "send-to");
    let (mut client, _) = open_at(&dir, T0, false);
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_default_lifetime(StoryLifetime::TwentyFourHours).expect("lifetime");

    let inherited = client
        .publish_story("story-inherited", "author-self", b"inherited body", None, &directory, &pairwise, T0)
        .expect("publish inherited");
    let overridden = client
        .publish_story(
            "story-overridden",
            "author-self",
            b"overridden body",
            Some(StoryAudience::Verified),
            &directory,
            &pairwise,
            T0,
        )
        .expect("publish overridden");

    // The reverse direction: a narrow default widened for one story.
    client.set_default_audience(StoryAudience::Verified).expect("audience");
    let widened = client
        .publish_story(
            "story-widened",
            "author-self",
            b"widened body",
            Some(StoryAudience::Everyone),
            &directory,
            &pairwise,
            T0,
        )
        .expect("publish widened");
    let narrow_default = client
        .publish_story("story-narrow", "author-self", b"narrow body", None, &directory, &pairwise, T0)
        .expect("publish narrow");

    drop(client);
    let (mut client, _) = open_at(&dir, T0 + 1_000, false);

    let stranger = "person-stranger-77aa11";
    let friend_only = VIEWERS[0];
    let verified = VIEWERS[2];
    let secret_of = |person: &str| *pairwise.get(person).expect("secret");
    let now = T0 + 1_000;
    let readable = |client: &StoryPrivacyClient, story: &str, person: &str| -> (bool, usize) {
        match client.open_story(story, person, &secret_of(person), now) {
            Ok(bytes) => (true, bytes.len()),
            Err(_) => (false, 0),
        }
    };

    // A relationship change after publish must not reach a published story.
    directory.insert(verified, Relationship { friend: true, verified: false });
    let after_change = client
        .publish_story(
            "story-after-change",
            "author-self",
            b"after change body",
            Some(StoryAudience::Verified),
            &directory,
            &pairwise,
            now,
        )
        .expect("publish after change");

    json!({
        "inherited": {
            "audience": inherited.audience,
            "source": inherited.audience_source,
            "audience_size": inherited.audience_size,
            "stranger_readable": readable(&client, "story-inherited", stranger).0,
        },
        "overridden": {
            "audience": overridden.audience,
            "source": overridden.audience_source,
            "audience_size": overridden.audience_size,
            "verified_readable": readable(&client, "story-overridden", verified).0,
            "verified_bytes": readable(&client, "story-overridden", verified).1,
            "friend_only_readable": readable(&client, "story-overridden", friend_only).0,
            "friend_only_bytes": readable(&client, "story-overridden", friend_only).1,
            "stranger_readable": readable(&client, "story-overridden", stranger).0,
            "stranger_bytes": readable(&client, "story-overridden", stranger).1,
            "friend_only_addressed": client.viewer_is_addressed("story-overridden", friend_only),
            "stranger_addressed": client.viewer_is_addressed("story-overridden", stranger),
        },
        "widened": {
            "audience": widened.audience,
            "source": widened.audience_source,
            "audience_size": widened.audience_size,
            "stranger_readable": readable(&client, "story-widened", stranger).0,
        },
        "narrow_default": {
            "audience": narrow_default.audience,
            "source": narrow_default.audience_source,
            "audience_size": narrow_default.audience_size,
            "stranger_readable": readable(&client, "story-narrow", stranger).0,
        },
        "relationship_snapshot": {
            "published_before_change_still_readable": readable(&client, "story-overridden", verified).0,
            "published_after_change_size": after_change.audience_size,
            "published_after_change_readable": readable(&client, "story-after-change", verified).0,
        }
    })
}

// ---------------------------------------------------------------------------
// 4. View receipts: off records nothing, on records only the aggregate
// ---------------------------------------------------------------------------

fn receipts_scenarios(root: &Path) -> Value {
    let (directory, pairwise) = people();

    // --- receipts ON -------------------------------------------------------
    let on_dir = fresh(root, "receipts-on");
    let (mut client, _) = open_at(&on_dir, T0, false);
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_view_receipts(true).expect("receipts on");
    client
        .publish_story("story-receipts", "author-self", b"receipt body", None, &directory, &pairwise, T0)
        .expect("publish");
    let mut outcomes = Vec::new();
    for (index, viewer) in VIEWERS.iter().enumerate() {
        let event = format!("open-event-{index}-4f2c8a1b");
        outcomes.push(client.record_view("story-receipts", viewer, &event, T0 + 60_000));
    }
    let count_after_four = client
        .view_signal("story-receipts")
        .map(|signal| signal.view_count)
        .unwrap_or(0);
    // Replaying the identical event ids must not move the number.
    let mut replay_outcomes = Vec::new();
    for (index, viewer) in VIEWERS.iter().enumerate() {
        let event = format!("open-event-{index}-4f2c8a1b");
        replay_outcomes.push(client.record_view("story-receipts", viewer, &event, T0 + 120_000));
    }
    let count_after_replay = client
        .view_signal("story-receipts")
        .map(|signal| signal.view_count)
        .unwrap_or(0);
    // A genuinely new open may move it, as the copy discloses.
    let _ = client.record_view("story-receipts", VIEWERS[0], "open-event-repeat-91be3d", T0 + 180_000);
    let count_after_repeat = client
        .view_signal("story-receipts")
        .map(|signal| signal.view_count)
        .unwrap_or(0);
    // A person outside the audience cannot register anything.
    let outsider = client.record_view("story-receipts", "person-not-addressed-000", "open-event-x", T0 + 200_000);
    let on_signal = client.view_signal("story-receipts").expect("signal");
    let on_signal_json = serde_json::to_value(&on_signal).expect("signal json");
    let on_signal_fields: Vec<String> = on_signal_json
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();
    let on_pre_open_copy = client.viewer_pre_open_copy().to_string();
    let on_observation = observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe on");
    drop(client);
    let (client, _) = open_at(&on_dir, T0 + 240_000, false);
    let on_after_restart = client
        .view_signal("story-receipts")
        .map(|signal| signal.view_count)
        .unwrap_or(0);
    let on_observation_after_restart =
        observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe on restart");
    drop(client);

    // --- receipts OFF ------------------------------------------------------
    let off_dir = fresh(root, "receipts-off");
    let (mut client, _) = open_at(&off_dir, T0, false);
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_view_receipts(false).expect("receipts off");
    client
        .publish_story("story-receipts", "author-self", b"receipt body", None, &directory, &pairwise, T0)
        .expect("publish");
    let mut off_outcomes = Vec::new();
    for (index, viewer) in VIEWERS.iter().enumerate() {
        let event = format!("open-event-{index}-4f2c8a1b");
        off_outcomes.push(client.record_view("story-receipts", viewer, &event, T0 + 60_000));
    }
    let off_signal_present = client.view_signal("story-receipts").is_some();
    let off_pre_open_copy = client.viewer_pre_open_copy().to_string();
    let off_observation = observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe off");
    drop(client);
    let (client, _) = open_at(&off_dir, T0 + 120_000, false);
    let off_observation_after_restart =
        observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe off restart");
    let off_signal_after_restart = client.view_signal("story-receipts").is_some();
    drop(client);

    // --- receipts ON then OFF: retroactive erasure -------------------------
    let retro_dir = fresh(root, "receipts-retroactive");
    let (mut client, _) = open_at(&retro_dir, T0, false);
    client.set_default_audience(StoryAudience::Everyone).expect("audience");
    client.set_view_receipts(true).expect("receipts on");
    client
        .publish_story("story-receipts", "author-self", b"receipt body", None, &directory, &pairwise, T0)
        .expect("publish");
    for (index, viewer) in VIEWERS.iter().enumerate() {
        let event = format!("open-event-{index}-4f2c8a1b");
        let _ = client.record_view("story-receipts", viewer, &event, T0 + 60_000);
    }
    let retro_before = observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe retro before");
    let retro_count_before = client
        .view_signal("story-receipts")
        .map(|signal| signal.view_count)
        .unwrap_or(0);
    let erasure = client.set_view_receipts(false).expect("receipts off");
    let retro_after = observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe retro after");
    let retro_signal_after = client.view_signal("story-receipts").is_some();
    drop(client);
    let (client, _) = open_at(&retro_dir, T0 + 120_000, false);
    let retro_after_restart =
        observe_viewer_records(&client, &PROFILE_KEY, &VIEWERS).expect("observe retro restart");
    drop(client);

    json!({
        "on": {
            "outcomes": outcomes.iter().map(describe_outcome).collect::<Vec<_>>(),
            "replay_outcomes": replay_outcomes.iter().map(describe_outcome).collect::<Vec<_>>(),
            "count_after_four_opens": count_after_four,
            "count_after_identical_replays": count_after_replay,
            "count_after_new_repeat_open": count_after_repeat,
            "outsider_refused": outsider.is_err(),
            "signal_fields": on_signal_fields,
            "signal": on_signal_json,
            "pre_open_copy": on_pre_open_copy,
            "count_after_restart": on_after_restart,
            "observation": serde_json::to_value(&on_observation).expect("observation"),
            "observation_after_restart": serde_json::to_value(&on_observation_after_restart).expect("observation"),
        },
        "off": {
            "outcomes": off_outcomes.iter().map(describe_outcome).collect::<Vec<_>>(),
            "signal_present": off_signal_present,
            "signal_present_after_restart": off_signal_after_restart,
            "pre_open_copy": off_pre_open_copy,
            "observation": serde_json::to_value(&off_observation).expect("observation"),
            "observation_after_restart": serde_json::to_value(&off_observation_after_restart).expect("observation"),
        },
        "retroactive": {
            "count_before_switch_off": retro_count_before,
            "records_before_switch_off": retro_before.total_viewer_records(),
            "rows_destroyed": erasure.rows_destroyed,
            "log_lines_destroyed": erasure.log_lines_destroyed,
            "records_destroyed": erasure.records_destroyed,
            "signal_after_switch_off": retro_signal_after,
            "observation_after": serde_json::to_value(&retro_after).expect("observation"),
            "observation_after_restart": serde_json::to_value(&retro_after_restart).expect("observation"),
        }
    })
}

fn describe_outcome(outcome: &Result<ViewOutcome, String>) -> Value {
    match outcome {
        Ok(ViewOutcome::NoSignalRecorded) => json!("no-signal-recorded"),
        Ok(ViewOutcome::Counted(signal)) => json!({"counted": signal.view_count}),
        Ok(ViewOutcome::ReplayIgnored(signal)) => json!({"replay_ignored": signal.view_count}),
        Err(error) => json!({"refused": error}),
    }
}

// ---------------------------------------------------------------------------
// 5. Screenshot shield: honest on both sides of availability
// ---------------------------------------------------------------------------

fn shield_scenarios(root: &Path) -> Value {
    let (directory, pairwise) = people();

    let unsupported_dir = fresh(root, "shield-unsupported");
    let (mut client, _) = open_at(&unsupported_dir, T0, false);
    let refusal = client.set_screenshot_shield(true).err();
    let unsupported_defaults = client.defaults();
    let unsupported_story = client
        .publish_story("story-shield", "author-self", b"shield body", None, &directory, &pairwise, T0)
        .expect("publish");
    drop(client);
    let (client, _) = open_at(&unsupported_dir, T0 + 1_000, false);
    let unsupported_after_restart = client.defaults();
    drop(client);

    let supported_dir = fresh(root, "shield-supported");
    let (mut client, _) = open_at(&supported_dir, T0, true);
    let accepted = client.set_screenshot_shield(true).expect("shield on");
    let supported_story = client
        .publish_story("story-shield", "author-self", b"shield body", None, &directory, &pairwise, T0)
        .expect("publish");
    drop(client);
    let (client, _) = open_at(&supported_dir, T0 + 1_000, true);
    let supported_after_restart = client.defaults();
    drop(client);

    // A profile that carried "shield on" from a machine that supports it must
    // not light up a claim on one that does not.
    let (client, _) = open_at(&supported_dir, T0 + 2_000, false);
    let ported_to_unsupported = client.defaults();
    drop(client);

    let native = shield_state(true);
    let native_application = story_privacy::apply_shield_to_window(0, true);

    json!({
        "primitive": SHIELD_PRIMITIVE,
        "unsupported": {
            "refusal": refusal,
            "state": serde_json::to_value(shield_state_for(false, true)).expect("state"),
            "defaults_state": serde_json::to_value(&unsupported_defaults.shield).expect("state"),
            "state_after_restart": serde_json::to_value(&unsupported_after_restart.shield).expect("state"),
            "story_shield_on_at_publish": unsupported_story.shield_on_at_publish,
        },
        "supported": {
            "state": serde_json::to_value(&accepted).expect("state"),
            "state_after_restart": serde_json::to_value(&supported_after_restart.shield).expect("state"),
            "story_shield_on_at_publish": supported_story.shield_on_at_publish,
            "off_state": serde_json::to_value(shield_state_for(true, false)).expect("state"),
        },
        "ported_profile_on_unsupported": serde_json::to_value(&ported_to_unsupported.shield).expect("state"),
        "native": {
            "target_os": std::env::consts::OS,
            "state": serde_json::to_value(&native).expect("state"),
            "application": serde_json::to_value(&native_application).expect("application"),
        }
    })
}

// ---------------------------------------------------------------------------

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("usage: task-6860-story-privacy <report.json> [workdir]");
        std::process::exit(2);
    }));
    let root = PathBuf::from(
        args.next()
            .unwrap_or_else(|| std::env::temp_dir().join("task-6860-story-privacy").to_string_lossy().into_owned()),
    );
    std::fs::create_dir_all(&root).expect("work root");

    let report = json!({
        "task": "6860",
        "copy": {
            "shield_disclosure": SHIELD_DISCLOSURE,
            "shield_unavailable": SHIELD_UNAVAILABLE_COPY,
            "receipts_on_viewer": VIEW_RECEIPT_VIEWER_COPY,
            "receipts_off_viewer": VIEW_RECEIPT_OFF_VIEWER_COPY,
        },
        "stable_ids": {
            "audiences": StoryAudience::ALL.iter().map(|a| a.stable_id()).collect::<Vec<_>>(),
            "lifetimes": StoryLifetime::ALL.iter().map(|l| l.stable_id()).collect::<Vec<_>>(),
            "lifetime_labels": StoryLifetime::ALL.iter().map(|l| l.label()).collect::<Vec<_>>(),
            "lifetime_seconds": StoryLifetime::ALL.iter().map(|l| l.seconds()).collect::<Vec<_>>(),
        },
        "viewer_probe_identities": VIEWERS,
        "defaults_inheritance": defaults_inheritance(&root),
        "burn_boundaries": burn_boundaries(&root),
        "send_to_override": send_to_override(&root),
        "receipts": receipts_scenarios(&root),
        "shield": shield_scenarios(&root),
    });
    std::fs::write(&out, serde_json::to_vec_pretty(&report).expect("report json"))
        .expect("write report");
    println!("TASK6860 report written to {}", out.display());
}
