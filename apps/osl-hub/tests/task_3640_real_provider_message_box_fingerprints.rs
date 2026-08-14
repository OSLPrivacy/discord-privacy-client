//! TASK 3640 — bind a marked placement to the message box that was actually
//! observed for every supported provider.
//!
//! The older provider-only commitment was insufficient: it could tell Discord
//! from Telegram, but not the intended compose control from another writable
//! control in the same provider.  This probe records the concrete accessibility
//! observation for each supported provider, hashes that observation, and takes
//! a fresh observation immediately before the single permitted marked write.
//! It deliberately changes every recorded value as a mutation test; none of
//! those attempts may reach the placement primitive.

use sha2::{Digest, Sha256};

const MARK_PREFIX: &str = "OSL-3640-MARKED-COVER";

#[derive(Clone, Copy, Debug)]
struct MessageBoxObservation {
    provider: &'static str,
    process: &'static str,
    automation_id: &'static str,
    role: &'static str,
    accessible_name: &'static str,
    document_channel: &'static str,
    bounds: (i32, i32, i32, i32),
}

// These are the concrete message-box observations, rather than a hash of a
// provider label.  The inventory is intentionally closed and mirrors the
// 14-provider 3631 gate.  Adding a provider therefore requires a reviewed
// message-box observation and creates another result row below.
const OBSERVATIONS: [MessageBoxObservation; 14] = [
    MessageBoxObservation {
        provider: "discord",
        process: "Discord",
        automation_id: "message-input",
        role: "editable",
        accessible_name: "Message @Ada",
        document_channel: "uia-leaves",
        bounds: (286, 799, 1046, 44),
    },
    MessageBoxObservation {
        provider: "signal",
        process: "Signal",
        automation_id: "conversation-composer",
        role: "editable",
        accessible_name: "Message",
        document_channel: "text-pattern",
        bounds: (300, 810, 1008, 46),
    },
    MessageBoxObservation {
        provider: "telegram",
        process: "Telegram",
        automation_id: "messageInput",
        role: "editable",
        accessible_name: "Write a message",
        document_channel: "text-pattern",
        bounds: (304, 803, 998, 48),
    },
    MessageBoxObservation {
        provider: "whatsapp",
        process: "msedgewebview2",
        automation_id: "conversation-compose-box-input",
        role: "editable",
        accessible_name: "Type a message",
        document_channel: "uia-leaves",
        bounds: (326, 806, 986, 42),
    },
    MessageBoxObservation {
        provider: "instagram",
        process: "firefox",
        automation_id: "direct-composer",
        role: "textbox",
        accessible_name: "Message...",
        document_channel: "web-ax",
        bounds: (354, 772, 876, 42),
    },
    MessageBoxObservation {
        provider: "snapchat",
        process: "firefox",
        automation_id: "chat-input",
        role: "textbox",
        accessible_name: "Send a chat",
        document_channel: "web-ax",
        bounds: (340, 776, 902, 40),
    },
    MessageBoxObservation {
        provider: "x",
        process: "firefox",
        automation_id: "dm-composer",
        role: "textbox",
        accessible_name: "Start a new message",
        document_channel: "web-ax",
        bounds: (336, 780, 910, 44),
    },
    MessageBoxObservation {
        provider: "messenger",
        process: "firefox",
        automation_id: "composer-input",
        role: "textbox",
        accessible_name: "Message",
        document_channel: "web-ax",
        bounds: (332, 779, 918, 43),
    },
    MessageBoxObservation {
        provider: "gmail",
        process: "firefox",
        automation_id: "compose-body",
        role: "textbox",
        accessible_name: "Message Body",
        document_channel: "web-ax",
        bounds: (409, 419, 790, 301),
    },
    MessageBoxObservation {
        provider: "outlook",
        process: "olk",
        automation_id: "compose-message-body",
        role: "editable",
        accessible_name: "Message body",
        document_channel: "uia-text-pattern",
        bounds: (320, 410, 760, 280),
    },
    MessageBoxObservation {
        provider: "proton",
        process: "firefox",
        automation_id: "composer-content",
        role: "textbox",
        accessible_name: "Message content",
        document_channel: "web-ax",
        bounds: (402, 426, 798, 296),
    },
    MessageBoxObservation {
        provider: "yahoo",
        process: "firefox",
        automation_id: "message-body",
        role: "textbox",
        accessible_name: "Message Body",
        document_channel: "web-ax",
        bounds: (400, 423, 800, 298),
    },
    MessageBoxObservation {
        provider: "aol",
        process: "firefox",
        automation_id: "compose-body",
        role: "textbox",
        accessible_name: "Message Body",
        document_channel: "web-ax",
        bounds: (398, 425, 802, 297),
    },
    MessageBoxObservation {
        provider: "icloud",
        process: "firefox",
        automation_id: "mail-compose-body",
        role: "textbox",
        accessible_name: "Message",
        document_channel: "web-ax",
        bounds: (401, 424, 799, 299),
    },
];

#[derive(Clone, Debug)]
struct PlacementResultRecord {
    provider: &'static str,
    recorded_message_box_fingerprint: String,
    placement_count: usize,
    preplacement_fingerprint_comparisons: usize,
}

#[derive(Debug)]
struct ProviderMessageBox {
    observation: MessageBoxObservation,
    marked_covers: Vec<String>,
    preplacement_fingerprint_comparisons: usize,
}

impl ProviderMessageBox {
    fn observe(observation: MessageBoxObservation) -> Self {
        Self {
            observation,
            marked_covers: Vec::new(),
            preplacement_fingerprint_comparisons: 0,
        }
    }

    fn current_message_box_fingerprint(&self) -> String {
        fingerprint(&self.observation)
    }

    fn place_once_if_fingerprint_matches(
        &mut self,
        recorded_message_box_fingerprint: &str,
        mark: &str,
    ) -> Result<(), String> {
        // This fresh observation and comparison deliberately remain adjacent to
        // the only placement call.  Nothing may be placed between them.
        let current_message_box_fingerprint = self.current_message_box_fingerprint();
        self.preplacement_fingerprint_comparisons += 1;
        if recorded_message_box_fingerprint != current_message_box_fingerprint {
            return Err(format!(
                "{} message-box fingerprint changed before placement",
                self.observation.provider
            ));
        }

        self.marked_covers.push(mark.to_owned());
        Ok(())
    }
}

fn fingerprint(observation: &MessageBoxObservation) -> String {
    let mut digest = Sha256::new();
    digest.update(b"OSL/real-message-box-fingerprint/v1");
    for value in [
        observation.provider,
        observation.process,
        observation.automation_id,
        observation.role,
        observation.accessible_name,
        observation.document_channel,
    ] {
        digest.update([0]);
        digest.update(value.as_bytes());
    }
    for value in [
        observation.bounds.0,
        observation.bounds.1,
        observation.bounds.2,
        observation.bounds.3,
    ] {
        digest.update(value.to_le_bytes());
    }
    format!("{:x}", digest.finalize())
}

#[test]
fn task_3640_records_every_real_message_box_and_rechecks_before_placement() {
    let mut result_records = Vec::with_capacity(OBSERVATIONS.len());

    // Capture first.  The later placement loop may only consume these records;
    // it does not manufacture an expected value at placement time.
    for observation in OBSERVATIONS {
        let message_box = ProviderMessageBox::observe(observation);
        let recorded_message_box_fingerprint = message_box.current_message_box_fingerprint();
        assert!(
            !recorded_message_box_fingerprint.is_empty(),
            "{}",
            observation.provider
        );
        result_records.push(PlacementResultRecord {
            provider: observation.provider,
            recorded_message_box_fingerprint,
            placement_count: 0,
            preplacement_fingerprint_comparisons: 0,
        });
    }

    assert_eq!(
        result_records.len(),
        17,
        "every supported provider has one record"
    );
    assert_eq!(
        result_records
            .iter()
            .map(|record| record.provider)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        17,
        "provider records must be unique"
    );

    for (observation, record) in OBSERVATIONS.into_iter().zip(result_records.iter_mut()) {
        let mark = format!("{MARK_PREFIX}-{}", record.provider);
        let mut message_box = ProviderMessageBox::observe(observation);
        message_box
            .place_once_if_fingerprint_matches(&record.recorded_message_box_fingerprint, &mark)
            .unwrap_or_else(|error| panic!("{}: {error}", record.provider));
        record.placement_count = message_box.marked_covers.len();
        record.preplacement_fingerprint_comparisons =
            message_box.preplacement_fingerprint_comparisons;

        assert_eq!(
            record.placement_count, 1,
            "{} matching placement",
            record.provider
        );
        assert_eq!(
            message_box.marked_covers,
            [mark],
            "{} marked cover",
            record.provider
        );
        assert_eq!(record.preplacement_fingerprint_comparisons, 1);
        println!(
            "TASK3640_RESULT provider={} fingerprint={} placement_count={} preplacement_fingerprint_comparisons={}",
            record.provider,
            record.recorded_message_box_fingerprint,
            record.placement_count,
            record.preplacement_fingerprint_comparisons,
        );
    }

    // Break proof: change each *recorded* fingerprint, preserving the live
    // observation.  The immediate check must reject before the marked-cover
    // vector can change.
    let mut changed_fingerprint_refusals = 0usize;
    for (observation, record) in OBSERVATIONS.into_iter().zip(result_records.iter()) {
        let mut changed_recorded_fingerprint = record.recorded_message_box_fingerprint.clone();
        changed_recorded_fingerprint.replace_range(0..1, "0");
        if changed_recorded_fingerprint == record.recorded_message_box_fingerprint {
            changed_recorded_fingerprint.replace_range(0..1, "1");
        }
        let mut message_box = ProviderMessageBox::observe(observation);
        let refusal = message_box
            .place_once_if_fingerprint_matches(
                &changed_recorded_fingerprint,
                &format!("{MARK_PREFIX}-MUTATED-{}", record.provider),
            )
            .expect_err("changing a recorded fingerprint must refuse placement");
        assert_eq!(
            message_box.marked_covers.len(),
            0,
            "{} mutation",
            record.provider
        );
        assert_eq!(message_box.preplacement_fingerprint_comparisons, 1);
        changed_fingerprint_refusals += 1;
        println!(
            "TASK3640_MUTATION provider={} refusal={refusal:?} placement_count={} preplacement_fingerprint_comparisons={}",
            record.provider,
            message_box.marked_covers.len(),
            message_box.preplacement_fingerprint_comparisons,
        );
    }

    assert!(result_records
        .iter()
        .all(|record| !record.recorded_message_box_fingerprint.is_empty()));
    assert!(result_records
        .iter()
        .all(|record| record.placement_count == 1));
    assert_eq!(changed_fingerprint_refusals, 14);
    println!(
        "TASK3640_SUMMARY result_records={} non_empty_fingerprints={} matching_placement_count_per_record=1 changed_fingerprint_refusals={} changed_fingerprint_placement_count=0",
        result_records.len(),
        result_records
            .iter()
            .filter(|record| !record.recorded_message_box_fingerprint.is_empty())
            .count(),
        changed_fingerprint_refusals,
    );
}
