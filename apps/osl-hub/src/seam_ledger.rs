//! **LEDGER 9 -- THE SEAM LEDGER.** Adapters DECLARED versus adapters with a
//! LIVE CARRY RECEIPT, ratcheted in both directions.
//!
//! # Why a ninth ledger
//!
//! All eight Binding Ledgers in `scripts/ledger/` are global set-differences
//! over the frontend and the IPC surface, and **none of them touches the
//! provider seam**. `grep -rn "Discord\|Telegram\|WhatsApp\|Signal"
//! scripts/ledger/*.mjs` returns comments. Every provider defect found on
//! 2026-08-04 lives on that seam:
//!
//! * **D-204** the handshake asked Chromium's honeypot (`lParam=1` returns
//!   `0x0` by design), so the probe answered "no accessibility" forever.
//! * **D-211** `DiscordPTB` is not `Discord`, and `same_process_name` stripped
//!   only `.exe`.
//! * **D-212**, **D-220** (Quill), **D-228** (a reclaim that reports success it
//!   did not achieve).
//!
//! Not one of those is visible to a ledger that diffs `data-*` attributes
//! against event bindings. This ledger states, per provider: **declared ·
//! published · receipt present · receipt sound · seam-drifted.**
//!
//! # It consumes the receipt machinery; it does not duplicate it
//!
//! [`crate::carry_seam_contract`] extracts the seam contract and
//! `native_apps::tests::carry_receipt` verifies receipts against it. This module
//! calls `native_apps::tests::fleet_report()` -- the same rows the fleet re-run
//! job prints -- and classifies them. A second extractor would be a second
//! answer to the one question the receipt exists to answer, and the two would
//! drift. [`the_seam_ledger_and_the_receipt_verifier_cannot_disagree`] makes the
//! non-duplication load-bearing: every row is re-derived through
//! `verify_receipt`, which is an independent route, and a disagreement fails.
//!
//! # The ratchet -- both directions
//!
//! Identical policy to `scripts/ledger/all.mjs:20-29`, against
//! `apps/osl-hub/carry-receipts/seam-ledger-baseline.json`:
//!
//! ```text
//!   live count > baseline      -> FAIL. A provider was published on nothing, or
//!                                 a receipt went off-seam. Raising the baseline
//!                                 is not the fix.
//!   live count < baseline      -> FAIL, "the baseline is stale". A seam was
//!                                 proven and the baseline was not lowered in the
//!                                 same change. A ratchet checked in only one
//!                                 direction rots into a floor nobody lowers.
//!   same count, different ids  -> FAIL. all.mjs:28 -- "One was fixed and one
//!                                 introduced, and the number alone hid the swap."
//! ```
//!
//! The baseline is **not** an exception file. Every id in it is an open,
//! owned finding that ships today, printed on every run.
//!
//! # What the baseline may never absorb
//!
//! A **new** provider published without a sound receipt is an increase, so it
//! fails. So does the ledger failing to cover a declared provider, and so does
//! an unusable baseline. See [`structural_problems`].
//!
//! # Running it
//!
//! ```text
//! cargo test --manifest-path apps/osl-hub/Cargo.toml --features core --lib \
//!   -- --nocapture seam_ledger
//! ```
//!
//! Already a CI step (`.github/workflows/rust-test.yml:128`). It is not in
//! `scripts/ledger/all.mjs` because that job "invokes no cargo at all" by design
//! (`rust-test.yml:214-216`) and its inputs are Rust.

use crate::native_apps::tests::{
    carry_receipt::{provider_slug, seam_slug, verify_receipt, ReceiptVerdict},
    declared_native_apps, fleet_report, CarrySeam, FleetAction, FleetRow,
};
use std::path::{Path, PathBuf};

/// Where the ratchet lives. Read at run time, so a mutant can move the baseline
/// without recompiling the crate.
pub(crate) fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("carry-receipts/seam-ledger-baseline.json")
}

const BASELINE_REL: &str = "apps/osl-hub/carry-receipts/seam-ledger-baseline.json";

/// The one thing this ledger is for: a declared adapter that cannot show a live,
/// sound, on-seam carry receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeamViolation {
    /// **The fatal class.** Shown to a user as supported, with no receipt, or
    /// with one that is stale, off-seam or unusable. A NEW one of these is an
    /// increase and fails the ratchet by name.
    PublishedWithoutASoundReceipt,
    /// A seam is declared and no live run has ever been made against it. Not
    /// shipping today; blocks publication.
    DeclaredWithoutALiveReceipt,
    /// A receipt exists, and the seam contract it was measured against has
    /// moved. The proof is about a seam that no longer exists.
    ReceiptOffSeam,
    /// A receipt exists and was never sound -- wrong schema, unparseable, or a
    /// contract that cannot be computed at all. Fatal wherever it is found.
    ReceiptUnsound,
    /// No carrier path is wired, so there is no adapter to prove. Recorded
    /// rather than skipped: an unwired provider is a fact about the product.
    NoCarryPathDeclared,
}

impl SeamViolation {
    pub(crate) const fn slug(self) -> &'static str {
        match self {
            Self::PublishedWithoutASoundReceipt => "published-without-a-sound-receipt",
            Self::DeclaredWithoutALiveReceipt => "declared-without-a-live-receipt",
            Self::ReceiptOffSeam => "receipt-off-seam",
            Self::ReceiptUnsound => "receipt-unsound",
            Self::NoCarryPathDeclared => "no-carry-path-declared",
        }
    }
}

/// One provider's row: the five facts the ledger states, plus its verdict.
#[derive(Debug, Clone)]
pub(crate) struct SeamRow {
    pub provider: &'static str,
    /// The seam the adapter is declared to consume.
    pub declared: &'static str,
    pub published: bool,
    pub support: String,
    pub receipt_present: bool,
    pub receipt_sound: bool,
    pub seam_drifted: bool,
    pub violation: Option<SeamViolation>,
    pub detail: String,
}

impl SeamRow {
    /// `<provider>:<violation>` -- the ratchet id. Provider-first so the
    /// baseline reads as a list of who owes what.
    pub(crate) fn id(&self) -> Option<String> {
        self.violation
            .map(|violation| format!("{}:{}", self.provider, violation.slug()))
    }
}

/// **The classification, pure.** Everything the ledger decides is decided here,
/// from three values, so every branch can be driven from a literal without a
/// signed-in Windows session or a receipt file on disk. That is what makes
/// [`every_published_shape_without_a_sound_receipt_is_a_violation`] a real
/// mutant suite instead of a restatement of the tree.
pub(crate) fn classify(
    action: FleetAction,
    published: bool,
    seam: CarrySeam,
) -> (bool, bool, bool, Option<SeamViolation>) {
    // (receipt_present, receipt_sound, seam_drifted)
    let (present, sound, drifted) = match action {
        FleetAction::Ok => (true, true, false),
        // The substrate FILE changed but not one declaration the adapter
        // consumes. That is exactly what rebinding to the seam contract bought,
        // and it is not a violation.
        FleetAction::SubstrateDrift => (true, true, false),
        FleetAction::MustRerun | FleetAction::RerunBeforePublishing => (true, false, true),
        FleetAction::ContractUnavailable | FleetAction::Unusable => (true, false, false),
        FleetAction::MustEarn | FleetAction::DebtRecorded | FleetAction::NoReceiptNotPublished => {
            (false, false, false)
        }
    };

    let violation = if sound {
        None
    } else if published {
        // A published provider with anything short of a sound, on-seam receipt
        // is the fatal class, whatever the reason. The reason is in `detail`;
        // it does not change what the user was told.
        Some(SeamViolation::PublishedWithoutASoundReceipt)
    } else if matches!(seam, CarrySeam::NoCarryPath) {
        Some(SeamViolation::NoCarryPathDeclared)
    } else if drifted {
        Some(SeamViolation::ReceiptOffSeam)
    } else if present {
        Some(SeamViolation::ReceiptUnsound)
    } else {
        Some(SeamViolation::DeclaredWithoutALiveReceipt)
    };

    (present, sound, drifted, violation)
}

fn row_from(fleet: &FleetRow) -> SeamRow {
    let (receipt_present, receipt_sound, seam_drifted, violation) =
        classify(fleet.action, fleet.published, fleet.seam);
    SeamRow {
        provider: provider_slug(fleet.id),
        declared: seam_slug(fleet.seam),
        published: fleet.published,
        // The PUBLIC CLAIM, not the adapter profile's internal support level and
        // not the word "ComingSoon" stamped on everything unpublished. That
        // stamp was this ledger's own copy of the collapse `PLAN.md` r4-5 names:
        // it printed the same label for a provider never built, one built and
        // never proven, and one measured and refused.
        support: crate::claim_state::public_claim(crate::native_apps::claim_surface(fleet.id))
            .slug()
            .to_owned(),
        receipt_present,
        receipt_sound,
        seam_drifted,
        violation,
        detail: fleet.detail.clone(),
    }
}

/// The ledger over the real tree.
pub(crate) fn seam_ledger() -> Vec<SeamRow> {
    fleet_report().iter().map(row_from).collect()
}

pub(crate) fn render(rows: &[SeamRow]) -> String {
    let mut out = String::from(
        "\nLEDGER 9 -- THE SEAM LEDGER: adapters declared vs adapters with a live carry receipt\n\
         ------------------------------------------------------------------------------------\n  \
         provider   claim          pub   declared seam          recpt sound drift  verdict\n",
    );
    for row in rows {
        out.push_str(&format!(
            "  {:<10} {:<14} {:<5} {:<22} {:<5} {:<5} {:<6} {}\n",
            row.provider,
            // The PUBLIC CLAIM and whether the adapter is PUBLISHED are two
            // different facts, and the row states both. They used to be one
            // column, which is why an unpublished provider printed the word
            // "ComingSoon" whatever its evidence said.
            row.support,
            if row.published { "yes" } else { "no" },
            row.declared,
            if row.receipt_present { "yes" } else { "NO" },
            if row.receipt_sound { "yes" } else { "NO" },
            if row.seam_drifted { "YES" } else { "no" },
            row.violation
                .map(|v| v.slug().to_owned())
                .unwrap_or_else(|| "-- clean".to_owned()),
        ));
        out.push_str(&format!("             {}\n", row.detail));
    }
    let published_unsound: Vec<&str> = rows
        .iter()
        .filter(|r| r.violation == Some(SeamViolation::PublishedWithoutASoundReceipt))
        .map(|r| r.provider)
        .collect();
    out.push_str(&format!(
        "\nPUBLISHED WITHOUT A SOUND RECEIPT: {}\n",
        if published_unsound.is_empty() {
            "none".to_owned()
        } else {
            published_unsound.join(", ")
        },
    ));
    out
}

// ---------------------------------------------------------------------------
// The baseline and the two-way ratchet.
//
// Deliberately the same shape as scripts/ledger/state-baseline.json and the
// same three-way comparison as scripts/ledger/all.mjs:128-173. PLAN.md r4-3
// item 4: one baseline schema for every ratchet. `openViolations` must equal
// `ids.len()`, because D-192 is what a pinned count without its set does.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct Baseline {
    pub ids: Vec<String>,
    pub open_violations: Option<usize>,
    pub owners: Vec<String>,
    pub problems: Vec<String>,
}

pub(crate) fn parse_baseline(text: &str) -> Baseline {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return Baseline {
                ids: Vec::new(),
                open_violations: None,
                owners: Vec::new(),
                problems: vec![format!("{BASELINE_REL}: unparseable ({error})")],
            }
        }
    };
    let mut problems = Vec::new();

    if value["schema"].as_str() != Some("osl-seam-ledger-baseline-v1") {
        problems.push(format!(
            "{BASELINE_REL}: schema is {:?}, expected \"osl-seam-ledger-baseline-v1\"",
            value["schema"].as_str().unwrap_or("<missing>")
        ));
    }

    let ids: Option<Vec<String>> = value["ids"].as_array().map(|array| {
        array
            .iter()
            .map(|entry| entry.as_str().unwrap_or("<not-a-string>").to_owned())
            .collect()
    });
    if ids.is_none() {
        problems.push(format!("{BASELINE_REL}: missing \"ids\" array"));
    }
    let ids = ids.unwrap_or_default();

    let open_violations = value["openViolations"].as_u64().map(|n| n as usize);
    match open_violations {
        None => problems.push(format!(
            "{BASELINE_REL}: missing numeric \"openViolations\""
        )),
        Some(n) if n != ids.len() => problems.push(format!(
            "{BASELINE_REL}: RATCHET -- \"openViolations\" is {n} but \"ids\" lists {}. The number \
             and the list are the same fact stated twice on purpose; a number nobody can check \
             against a list is a number that drifts (D-192).",
            ids.len()
        )),
        Some(_) => {}
    }

    for id in &ids {
        if ids.iter().filter(|other| *other == id).count() > 1 {
            let duplicate = format!("{BASELINE_REL}: duplicate id {id:?}");
            if !problems.contains(&duplicate) {
                problems.push(duplicate);
            }
        }
        if value["defects"][id].as_str().is_none() {
            problems.push(format!(
                "{BASELINE_REL}: id {id:?} has no entry in \"defects\". Every recorded violation \
                 names the defect that owns it, or it is not recorded -- it is parked."
            ));
        }
    }

    let owners = value["owners"]
        .as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(|entry| entry.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    Baseline {
        ids,
        open_violations,
        owners,
        problems,
    }
}

pub(crate) struct Verdict {
    pub ok: bool,
    pub lines: Vec<String>,
}

/// The ratchet itself. Pure over two id lists, so both failure directions and
/// the equal-count swap can be driven without a tree.
pub(crate) fn ratchet(live_ids: &[String], baseline: &Baseline) -> Verdict {
    let mut live: Vec<String> = live_ids.to_vec();
    live.sort();
    live.dedup();
    let mut base = baseline.ids.clone();
    base.sort();
    let mut lines = Vec::new();

    if !baseline.problems.is_empty() {
        lines.push(format!("RATCHET: {BASELINE_REL} IS NOT USABLE."));
        for problem in &baseline.problems {
            lines.push(format!("  - {problem}"));
        }
        return Verdict { ok: false, lines };
    }

    let introduced: Vec<&String> = live.iter().filter(|id| !base.contains(id)).collect();
    let fixed: Vec<&String> = base.iter().filter(|id| !live.contains(id)).collect();

    if introduced.is_empty() && fixed.is_empty() {
        lines.push(format!(
            "RATCHET: the seam ledger is at its recorded baseline of {} ({BASELINE_REL}).",
            base.len()
        ));
        lines.push(format!(
            "  These are open findings with owners ({}), not accepted exceptions.",
            if baseline.owners.is_empty() {
                "none recorded".to_owned()
            } else {
                baseline.owners.join(", ")
            }
        ));
        lines.push(
            "  The number may only go DOWN, and it may never be spent again.".to_owned(),
        );
        return Verdict { ok: true, lines };
    }

    if !introduced.is_empty() && !fixed.is_empty() {
        lines.push(format!(
            "RATCHET: the seam ledger DRIFTED -- {} proven and {} introduced, so the count alone \
             hid the swap (scripts/ledger/all.mjs:28).",
            fixed.len(),
            introduced.len()
        ));
    } else if !introduced.is_empty() {
        lines.push(format!(
            "RATCHET: the seam ledger REGRESSED -- baseline {}, now {}.",
            base.len(),
            live.len()
        ));
    } else {
        lines.push(format!(
            "RATCHET: the seam ledger's BASELINE IS STALE -- baseline {}, now {}.",
            base.len(),
            live.len()
        ));
        lines.push(
            "  A seam was proven and the baseline was not lowered in the same change. A ratchet \
             checked in only one direction rots into a floor nobody ever lowers. Take the win:"
                .to_owned(),
        );
    }

    if !introduced.is_empty() {
        lines.push(
            "  NEW violations, absent from the baseline -- fix them; raising the baseline is not \
             an option:"
                .to_owned(),
        );
        for id in &introduced {
            lines.push(format!("    + {id}"));
        }
    }
    if !fixed.is_empty() {
        lines.push("  Violations still listed in the baseline that no longer exist -- delete these ids:".to_owned());
        for id in &fixed {
            lines.push(format!("    - {id}"));
        }
    }
    lines.push(format!(
        "  {BASELINE_REL} must then read \"openViolations\": {}",
        live.len()
    ));
    Verdict { ok: false, lines }
}

/// Reasons this ledger can be red that the baseline deliberately does **not**
/// absorb. The baseline records known findings; it must never absorb "the
/// ledger could not see a provider" or "the report and the verifier disagree".
pub(crate) fn structural_problems(rows: &[SeamRow]) -> Vec<String> {
    let mut problems = Vec::new();

    let declared: Vec<&'static str> = declared_native_apps()
        .into_iter()
        .map(provider_slug)
        .collect();
    for slug in &declared {
        let seen = rows.iter().filter(|row| row.provider == *slug).count();
        if seen != 1 {
            problems.push(format!(
                "{slug} appears {seen} time(s) in the seam ledger; every declared native app must \
                 appear exactly once. A provider the ledger cannot see is a provider it cannot gate."
            ));
        }
    }
    for row in rows {
        if !declared.contains(&row.provider) {
            problems.push(format!(
                "{} is in the seam ledger but is not a declared native app",
                row.provider
            ));
        }
    }

    // The report and the gate's own verifier are two independent routes to the
    // same fact. If they can disagree, one of them has stopped looking.
    for fleet in fleet_report() {
        let row = row_from(&fleet);
        let verdict = verify_receipt(fleet.id, fleet.seam);
        let verifier_says_sound = matches!(
            verdict,
            ReceiptVerdict::Earned | ReceiptVerdict::EarnedWithSubstrateDrift(_)
        );
        if verifier_says_sound != row.receipt_sound {
            problems.push(format!(
                "{}: the seam ledger says receipt_sound={}, and verify_receipt says {verdict:?}. \
                 The ledger and the gate must not be able to disagree about who has a proof.",
                row.provider, row.receipt_sound
            ));
        }
        let verifier_says_present = !matches!(verdict, ReceiptVerdict::Absent);
        if verifier_says_present != row.receipt_present {
            problems.push(format!(
                "{}: the seam ledger says receipt_present={}, and verify_receipt says {verdict:?}.",
                row.provider, row.receipt_present
            ));
        }
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> Baseline {
        let text = std::fs::read_to_string(baseline_path())
            .unwrap_or_else(|error| panic!("{BASELINE_REL} is readable: {error}"));
        parse_baseline(&text)
    }

    /// **LEDGER 9.** Prints the whole table either way, so a reader sees the
    /// real state of the seam rather than one pass/fail bit.
    #[test]
    fn the_seam_ledger_is_at_its_recorded_baseline() {
        let rows = seam_ledger();
        eprintln!("{}", render(&rows));

        let mut failures = Vec::new();

        let structural = structural_problems(&rows);
        if !structural.is_empty() {
            failures.push(
                "STRUCTURAL: the seam ledger is red for a reason the baseline does not cover:"
                    .to_owned(),
            );
            failures.extend(structural.into_iter().map(|p| format!("  - {p}")));
        }

        let ids: Vec<String> = rows.iter().filter_map(SeamRow::id).collect();
        let verdict = ratchet(&ids, &baseline());
        eprintln!("{}", verdict.lines.join("\n"));
        if !verdict.ok {
            failures.extend(verdict.lines);
        }

        // `--strict` for this ledger, the same escape hatch scripts/ledger/all.mjs
        // offers: every row fails, baseline or not. The ratchet exists so that a
        // permanently-red CI does not train everyone to ignore it -- it is not a
        // claim that the recorded rows are fine. Anyone asking "what does the seam
        // actually look like with no allowances" runs this.
        if std::env::var("OSL_SEAM_LEDGER_STRICT").is_ok() && !ids.is_empty() {
            failures.push(format!(
                "OSL_SEAM_LEDGER_STRICT: {} declared adapter(s) cannot show a live, sound, on-seam \
                 receipt, and strict mode fails on any of them regardless of the baseline:",
                ids.len()
            ));
            for id in &ids {
                failures.push(format!("  - {id}"));
            }
        }

        assert!(
            failures.is_empty(),
            "\n{}\n{}\n",
            render(&rows),
            failures.join("\n")
        );
    }

    /// The non-duplication, made load-bearing. This ledger classifies rows the
    /// fleet report computed **without** going through `verify_receipt`; here
    /// the two routes are compared. A ledger that agreed with itself would
    /// prove nothing.
    #[test]
    fn the_seam_ledger_and_the_receipt_verifier_cannot_disagree() {
        let rows = seam_ledger();
        let problems = structural_problems(&rows);
        assert!(
            problems.is_empty(),
            "the seam ledger and the receipt verifier disagree:\n{}",
            problems.join("\n")
        );
    }

    /// **The gate's own rejection paths.** Every shape a published provider can
    /// be in without a sound, on-seam receipt must classify as the fatal class,
    /// by name. Driven from literals: no receipt file, no Windows session, and
    /// no way to satisfy it by editing the manifest.
    #[test]
    fn every_published_shape_without_a_sound_receipt_is_a_violation() {
        let unsound = [
            FleetAction::MustEarn,
            FleetAction::DebtRecorded,
            FleetAction::MustRerun,
            FleetAction::RerunBeforePublishing,
            FleetAction::Unusable,
            FleetAction::ContractUnavailable,
        ];
        for action in unsound {
            for seam in [
                CarrySeam::Uia2Substrate,
                CarrySeam::NativeWindowHost,
                CarrySeam::ProviderOwnedBackend,
                CarrySeam::NoCarryPath,
            ] {
                let (_, sound, _, violation) = classify(action, true, seam);
                assert!(!sound, "{action:?} must never read as a sound receipt");
                assert_eq!(
                    violation,
                    Some(SeamViolation::PublishedWithoutASoundReceipt),
                    "a provider published on {action:?} over {} must be the fatal class, not \
                     something softer",
                    seam_slug(seam)
                );
            }
        }

        // And the two shapes that ARE sound must not be reported as violations,
        // or the ledger is red for everyone and gets ignored.
        for action in [FleetAction::Ok, FleetAction::SubstrateDrift] {
            let (present, sound, drifted, violation) =
                classify(action, true, CarrySeam::Uia2Substrate);
            assert!(present && sound && !drifted, "{action:?} is a live receipt");
            assert_eq!(violation, None, "{action:?} must not be a violation");
        }
    }

    /// Unpublished providers are still on the ledger, with the right name. The
    /// point of the set-difference is that a declared adapter with no proof is
    /// VISIBLE, not that it is fatal today.
    #[test]
    fn a_declared_adapter_with_no_receipt_is_named_even_when_it_is_not_published() {
        let (present, sound, _, violation) = classify(
            FleetAction::NoReceiptNotPublished,
            false,
            CarrySeam::Uia2Substrate,
        );
        assert!(!present && !sound);
        assert_eq!(violation, Some(SeamViolation::DeclaredWithoutALiveReceipt));

        assert_eq!(
            classify(FleetAction::NoReceiptNotPublished, false, CarrySeam::NoCarryPath).3,
            Some(SeamViolation::NoCarryPathDeclared),
            "a provider with no carrier path wired is recorded as that, not as an unearned receipt"
        );
        assert_eq!(
            classify(FleetAction::RerunBeforePublishing, false, CarrySeam::Uia2Substrate).3,
            Some(SeamViolation::ReceiptOffSeam),
            "a receipt measured against a seam that has moved is not a proof of today's seam"
        );
        assert_eq!(
            classify(FleetAction::Unusable, false, CarrySeam::Uia2Substrate).3,
            Some(SeamViolation::ReceiptUnsound),
            "an unsound receipt is fatal wherever it is found, including at ComingSoon"
        );
    }

    /// **The ratchet, in all three directions.** Driven from id lists, so this
    /// cannot be satisfied by anything in the tree.
    #[test]
    fn the_ratchet_fails_on_an_increase_a_decrease_and_a_swap() {
        let base = Baseline {
            ids: vec!["discord:published-without-a-sound-receipt".to_owned()],
            open_violations: Some(1),
            owners: vec!["D-203".to_owned()],
            problems: Vec::new(),
        };

        assert!(
            ratchet(&["discord:published-without-a-sound-receipt".to_owned()], &base).ok,
            "the recorded baseline itself must be green, or every case below is vacuous"
        );

        let increase = ratchet(
            &[
                "discord:published-without-a-sound-receipt".to_owned(),
                "signal:published-without-a-sound-receipt".to_owned(),
            ],
            &base,
        );
        assert!(!increase.ok, "a new violation must fail");
        assert!(
            increase
                .lines
                .iter()
                .any(|line| line.contains("signal:published-without-a-sound-receipt")),
            "the failure must NAME the provider it is about:\n{}",
            increase.lines.join("\n")
        );

        let decrease = ratchet(&[], &base);
        assert!(!decrease.ok, "an unrecorded decrease must fail");
        assert!(
            decrease.lines.iter().any(|line| line.contains("BASELINE IS STALE")),
            "a decrease must say the baseline is stale, not congratulate anyone:\n{}",
            decrease.lines.join("\n")
        );

        let swap = ratchet(&["signal:declared-without-a-live-receipt".to_owned()], &base);
        assert!(!swap.ok, "same count, different ids must fail");
        assert!(
            swap.lines.iter().any(|line| line.contains("DRIFTED")),
            "a swap at an unchanged count is the case the number alone hides:\n{}",
            swap.lines.join("\n")
        );
    }

    /// A baseline that cannot be trusted is not a baseline. None of these may
    /// read as green.
    #[test]
    fn an_unusable_baseline_is_never_absorbed() {
        for (label, text) in [
            ("not json", "{".to_owned()),
            (
                "count does not match the ids",
                r#"{"schema":"osl-seam-ledger-baseline-v1","openViolations":9,"ids":["a:b"],"defects":{"a:b":"D-1"}}"#.to_owned(),
            ),
            (
                "wrong schema",
                r#"{"schema":"something-else","openViolations":0,"ids":[]}"#.to_owned(),
            ),
            (
                "an id with no defect",
                r#"{"schema":"osl-seam-ledger-baseline-v1","openViolations":1,"ids":["a:b"],"defects":{}}"#.to_owned(),
            ),
            (
                "no ids array at all",
                r#"{"schema":"osl-seam-ledger-baseline-v1","openViolations":0}"#.to_owned(),
            ),
        ] {
            let parsed = parse_baseline(&text);
            assert!(
                !parsed.problems.is_empty(),
                "a baseline that is {label} must be rejected"
            );
            let verdict = ratchet(&[], &parsed);
            assert!(
                !verdict.ok,
                "a baseline that is {label} must fail the ratchet, not pass it"
            );
        }
    }

    /// The real baseline on this tree parses, and its number matches its list.
    #[test]
    fn the_recorded_baseline_is_well_formed() {
        let parsed = baseline();
        assert!(
            parsed.problems.is_empty(),
            "{BASELINE_REL} is not usable:\n{}",
            parsed.problems.join("\n")
        );
        assert_eq!(
            parsed.open_violations,
            Some(parsed.ids.len()),
            "openViolations must equal ids.len()"
        );
        assert!(
            !parsed.owners.is_empty(),
            "the baseline must name the defects that own its rows"
        );
    }
}
