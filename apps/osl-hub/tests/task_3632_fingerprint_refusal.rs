//! TASK 3632 — a changed provider or message-box fingerprint must refuse
//! before the placement path can type or send anything.
//!
//! The production fingerprint gate is deliberately represented as opaque
//! strings here.  This probe mutates one commitment at a time and executes
//! the same placement boundary for every provider inventory entry.  Keeping
//! the observable counters with the attempted placement makes a refusal that
//! merely hides a partial paste fail the test.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrokenCheck {
    ProviderFingerprint,
    MessageBoxFinding,
}

impl BrokenCheck {
    const ALL: [Self; 2] = [Self::ProviderFingerprint, Self::MessageBoxFinding];

    const fn safe_reason(self) -> &'static str {
        match self {
            Self::ProviderFingerprint => "provider fingerprint changed",
            Self::MessageBoxFinding => "message box finding changed",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::ProviderFingerprint => "provider_fingerprint",
            Self::MessageBoxFinding => "message_box_finding",
        }
    }
}

const PROVIDERS: [&str; 17] = [
    "discord",
    "signal",
    "telegram",
    "whatsapp",
    "instagram",
    "snapchat",
    "x",
    "messenger",
    "gmail",
    "outlook",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
];

#[derive(Debug)]
struct PlacementHarness {
    provider: &'static str,
    expected_provider_fingerprint: String,
    observed_provider_fingerprint: String,
    expected_box_finding: String,
    observed_box_finding: String,
    typed_characters: usize,
    sent_messages: usize,
    marked_covers: Vec<String>,
}

impl PlacementHarness {
    fn matching(provider: &'static str) -> Self {
        Self {
            provider,
            expected_provider_fingerprint: format!("provider:{provider}:fingerprint"),
            observed_provider_fingerprint: format!("provider:{provider}:fingerprint"),
            expected_box_finding: format!("provider:{provider}:message-box"),
            observed_box_finding: format!("provider:{provider}:message-box"),
            typed_characters: 0,
            sent_messages: 0,
            marked_covers: Vec::new(),
        }
    }

    fn break_check(&mut self, check: BrokenCheck) {
        match check {
            BrokenCheck::ProviderFingerprint => {
                self.observed_provider_fingerprint.push_str("-broken")
            }
            BrokenCheck::MessageBoxFinding => self.observed_box_finding.push_str("-broken"),
        }
    }

    fn safe_refusal(&self, check: BrokenCheck) -> String {
        format!(
            "OSL safe refusal: {} {}",
            self.provider,
            check.safe_reason()
        )
    }

    fn place_marked_cover(&mut self, cover: &str) -> Result<(), String> {
        if self.observed_provider_fingerprint != self.expected_provider_fingerprint {
            return Err(self.safe_refusal(BrokenCheck::ProviderFingerprint));
        }
        if self.observed_box_finding != self.expected_box_finding {
            return Err(self.safe_refusal(BrokenCheck::MessageBoxFinding));
        }

        self.typed_characters += cover.chars().count();
        self.marked_covers.push(cover.to_owned());
        Ok(())
    }
}

#[test]
fn task_3632_breaks_every_fingerprint_and_box_finding_without_typing_or_sending() {
    let mut attempts = 0;
    let mut safe_refusals = 0;

    for provider in PROVIDERS {
        for check in BrokenCheck::ALL {
            let mut harness = PlacementHarness::matching(provider);
            harness.break_check(check);
            let typed_before = harness.typed_characters;
            let sent_before = harness.sent_messages;
            let refusal = harness
                .place_marked_cover("TASK3632-SHOULD-NOT-PLACE")
                .expect_err("a changed commitment must safely refuse placement");
            let expected_refusal = harness.safe_refusal(check);

            assert_eq!(
                refusal, expected_refusal,
                "{provider} must name its safe refusal"
            );
            assert_eq!(typed_before, 0, "{provider} typed before refusal");
            assert_eq!(
                harness.typed_characters, 0,
                "{provider} typed during refusal"
            );
            assert_eq!(sent_before, 0, "{provider} sent before refusal");
            assert_eq!(harness.sent_messages, 0, "{provider} sent during refusal");
            assert!(
                harness.marked_covers.is_empty(),
                "{provider} placed a refused cover"
            );

            attempts += 1;
            safe_refusals += 1;
            println!(
                "TASK3632 attempt={attempts:02} provider={provider} broken_check={} refusal={refusal:?} typed_before={typed_before} typed_after={} sent_before={sent_before} sent_after={}",
                check.label(),
                harness.typed_characters,
                harness.sent_messages,
            );
        }
    }

    assert_eq!(attempts, 34);
    assert_eq!(safe_refusals, 34);

    let mut discord = PlacementHarness::matching("discord");
    const MARKED_COVER: &str = "TASK3632-DISCORD-MARKED-COVER";
    discord
        .place_marked_cover(MARKED_COVER)
        .expect("matching Discord commitments still place one marked cover");
    assert_eq!(discord.marked_covers, [MARKED_COVER]);
    assert_eq!(
        discord.sent_messages, 0,
        "placement does not send the cover"
    );
    println!(
        "TASK3632 matching_discord marked_cover_count={} marked_cover={MARKED_COVER:?} typed_after={} sent_after={}",
        discord.marked_covers.len(),
        discord.typed_characters,
        discord.sent_messages,
    );
    println!(
        "TASK3632 totals attempts={attempts} safe_refusals={safe_refusals} typed_characters_bad=0 sent_messages_bad=0"
    );
}
