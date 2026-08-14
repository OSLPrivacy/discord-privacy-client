//! The oracle's own gate.
//!
//! Every refusal below is driven by **starving the input that would have earned
//! the verdict**, because a refusal nobody has watched happen is decoration.
//! The live counterpart of this file is
//! `native_a11y`'s Discord probe and `landing_oracle_live.rs`; these run
//! everywhere and are what make the live run interpretable.

use super::*;
use crate::native_a11y::Uia2Editable;

// ---------------------------------------------------------------------------
// A judge whose every channel can be starved independently
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
struct FakeJudge {
    identity: Option<WindowIdentity>,
    uia: Option<RenderedDocument>,
    text_pattern: Option<RenderedDocument>,
    msaa: Option<RenderedDocument>,
    ink: Option<Ink>,
    value_property: Option<String>,
    submit_shaped: usize,
    timeout_on: Option<&'static str>,
    strays: Vec<(isize, WindowIdentity, RenderedDocument)>,
}

impl FakeJudge {
    fn timeout(&self, channel: &'static str) -> Result<(), JudgeTimeout> {
        if self.timeout_on == Some(channel) {
            return Err(JudgeTimeout { millis: 5_000 });
        }
        Ok(())
    }
}

impl LandingJudgeSyscalls for FakeJudge {
    fn window_identity(
        &self,
        hwnd: isize,
        _deadline: JudgeDeadline,
    ) -> Result<WindowIdentity, JudgeTimeout> {
        self.timeout("identity")?;
        if let Some((_, identity, _)) = self.strays.iter().find(|(stray, _, _)| *stray == hwnd) {
            return Ok(identity.clone());
        }
        Ok(self.identity.clone().unwrap_or(WindowIdentity {
            hwnd,
            process_id: 9020,
            process_name: "Discord".to_owned(),
        }))
    }

    fn rendered_document_uia(
        &self,
        bound: &BoundComposer,
        _caps: WalkCaps,
        _join: &str,
        _deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        self.timeout("uia")?;
        if let Some((_, _, document)) = self
            .strays
            .iter()
            .find(|(stray, _, _)| *stray == bound.hwnd)
        {
            return Ok(Some(document.clone()));
        }
        Ok(self.uia.clone())
    }

    fn rendered_document_text_pattern(
        &self,
        _bound: &BoundComposer,
        _caps: WalkCaps,
        _deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        self.timeout("text_pattern")?;
        Ok(self.text_pattern.clone())
    }

    fn rendered_document_msaa(
        &self,
        _bound: &BoundComposer,
        _caps: WalkCaps,
        _join: &str,
        _deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        self.timeout("msaa")?;
        Ok(self.msaa.clone())
    }

    fn composer_ink(
        &self,
        _bound: &BoundComposer,
        _deadline: JudgeDeadline,
    ) -> Result<Option<Ink>, JudgeTimeout> {
        self.timeout("ink")?;
        Ok(self.ink)
    }

    fn disowned_value_property(
        &self,
        _bound: &BoundComposer,
        _deadline: JudgeDeadline,
    ) -> Result<Option<String>, JudgeTimeout> {
        self.timeout("value")?;
        Ok(self.value_property.clone())
    }

    fn submit_shaped_calls(&self) -> usize {
        self.submit_shaped
    }
}

fn document(text: &str) -> RenderedDocument {
    RenderedDocument {
        leaves: vec![text.to_owned()],
        text: text.to_owned(),
        nodes_visited: 3,
        depth_reached: 2,
    }
}

fn leaves(parts: &[&str], join: &str) -> RenderedDocument {
    RenderedDocument {
        leaves: parts.iter().map(|part| (*part).to_owned()).collect(),
        text: parts.join(join),
        nodes_visited: parts.len() + 2,
        depth_reached: 2,
    }
}

fn bound() -> BoundComposer {
    BoundComposer {
        hwnd: 0x1234,
        route: crate::native_a11y::Uia2TreeRoute::MsaaBridge,
        process_id: 9020,
        composer: Uia2Editable {
            runtime_id: vec![42, 7],
            name: "Message @Deckard".to_owned(),
            value_pattern: true,
            enabled: true,
            keyboard_focusable: true,
            has_keyboard_focus: true,
            read_only: false,
        },
    }
}

const CARRIER: &str = "the usual by friday, same place as last time";

/// The composer holding this much ink when empty. Any smaller number than
/// `min_ink_delta` above it is "the pixels did not change".
fn empty_ink() -> Ink {
    Ink {
        rect: Rect {
            left: 100,
            top: 900,
            right: 1100,
            bottom: 940,
        },
        sampled: 40_000,
        inked: 120,
    }
}

fn inked(count: u32) -> Ink {
    Ink {
        inked: count,
        ..empty_ink()
    }
}

fn baseline() -> LandingBaseline {
    LandingBaseline {
        empty_ink: empty_ink(),
    }
}

fn landed_judge() -> FakeJudge {
    FakeJudge {
        uia: Some(document(CARRIER)),
        text_pattern: Some(document(CARRIER)),
        ink: Some(inked(1_400)),
        value_property: Some(CARRIER.to_owned()),
        ..FakeJudge::default()
    }
}

fn judge(fake: &FakeJudge) -> Result<LandingProof, LandingRefusal> {
    judge_landing(fake, &DISCORD, &bound(), CARRIER, baseline(), &[])
}

fn judge_empty(fake: &FakeJudge) -> Result<ComposerEmptyProof, ComposerEmptyRefusal> {
    judge_empty_composer(fake, &DISCORD, &bound(), baseline())
}

// ---------------------------------------------------------------------------
// The sound case, asserted FIRST. A gate that has only ever been shown
// refusing is as useless as one that has only ever been shown passing.
// ---------------------------------------------------------------------------

#[test]
fn the_sound_landing_is_accepted() {
    let proof = judge(&landed_judge()).expect("a byte-exact landing on screen must be accepted");
    assert_eq!(proof.document, CARRIER);
    assert_eq!(proof.write_channel, WriteChannel::SynthesizedInput);
    assert_eq!(
        proof.judged_by,
        vec![
            JudgeChannel::RenderedDocumentUia,
            JudgeChannel::RenderedDocumentTextPattern,
            JudgeChannel::ComposerInk
        ]
    );
    assert_eq!(proof.ink_delta, 1_280);
    assert_eq!(proof.commit_key_not_sent, "Enter");
    assert_eq!(proof.submit_shaped_calls, 0);
}

// ---------------------------------------------------------------------------
// 1 · Nothing placed
// ---------------------------------------------------------------------------

#[test]
fn nothing_placed_is_refused_by_name() {
    let fake = FakeJudge {
        // Discord's empty Slate composer. `U+FEFF` is not whitespace to
        // `str::trim`, so a naive emptiness predicate calls this a draft.
        uia: Some(leaves(&["\u{feff}", "\n"], "")),
        text_pattern: Some(leaves(&["\u{feff}", "\n"], "")),
        ink: Some(empty_ink()),
        value_property: Some(String::new()),
        ..FakeJudge::default()
    };
    let refusal = judge(&fake).expect_err("an empty composer must not earn a landing");
    assert_eq!(refusal.name(), "NothingPlaced");
    let LandingRefusal::NothingPlaced {
        disowned_value_property_claims_landed,
        ..
    } = refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert!(!disowned_value_property_claims_landed);
}

#[test]
fn an_empty_composer_is_proven_by_document_leaves_and_ink() {
    let fake = FakeJudge {
        uia: Some(leaves(&["\u{feff}", "\n"], "")),
        text_pattern: Some(leaves(&["\u{feff}", "\n"], "")),
        ink: Some(inked(empty_ink().inked + DISCORD.min_ink_delta - 1)),
        value_property: Some(String::new()),
        ..FakeJudge::default()
    };
    let proof = judge_empty(&fake).expect("the empty document and empty-baseline ink must pass");
    assert_eq!(proof.document, "\u{feff}\n");
    assert_eq!(proof.ink_delta, DISCORD.min_ink_delta - 1);
    assert_eq!(proof.judged_by, DISCORD.judges);
}

#[test]
fn accepted_input_without_an_empty_document_is_not_a_clear_proof() {
    let fake = FakeJudge {
        uia: Some(document(CARRIER)),
        text_pattern: Some(document(CARRIER)),
        ink: Some(inked(1_400)),
        value_property: Some(CARRIER.to_owned()),
        ..FakeJudge::default()
    };
    let refusal = judge_empty(&fake).expect_err("a non-empty document must not be called clear");
    assert_eq!(refusal.name(), "NotEmpty");
}

/// **D-205, reproduced as a refusal instead of inherited as a belief.**
///
/// `SetValue` moved the accessibility value; Slate's document did not move.
/// The value channel says the carrier is there. The oracle refuses anyway, and
/// records that the disowned channel disagreed — which is the whole point of
/// the module.
#[test]
fn the_value_property_claiming_a_landing_does_not_produce_one() {
    let fake = FakeJudge {
        uia: Some(leaves(&["\u{feff}", "\n"], "")),
        text_pattern: Some(leaves(&["\u{feff}", "\n"], "")),
        ink: Some(empty_ink()),
        // What D-205 read back and believed.
        value_property: Some(CARRIER.to_owned()),
        ..FakeJudge::default()
    };

    // What the old instrument would have concluded, stated explicitly so the
    // difference is visible rather than asserted.
    let old_verdict = fake
        .value_property
        .as_deref()
        .is_some_and(|value| value.contains(CARRIER));
    assert!(
        old_verdict,
        "the fixture must reproduce D-205: readback_holds_carrier was TRUE"
    );

    let refusal = judge(&fake).expect_err("the value property must never earn a landing");
    assert_eq!(refusal.name(), "NothingPlaced");
    let LandingRefusal::NothingPlaced {
        disowned_value_property_claims_landed,
        ..
    } = refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert!(
        disowned_value_property_claims_landed,
        "the refusal must carry the fact that the disowned channel disagreed"
    );
}

#[test]
fn the_value_property_claiming_empty_does_not_prove_a_clear() {
    let fake = FakeJudge {
        uia: Some(document(CARRIER)),
        text_pattern: Some(document(CARRIER)),
        ink: Some(inked(1_400)),
        value_property: Some(String::new()),
        ..FakeJudge::default()
    };

    let old_verdict = fake
        .value_property
        .as_deref()
        .is_some_and(|value| value.is_empty());
    assert!(
        old_verdict,
        "the fixture must reproduce the disowned-channel clear mutant"
    );

    let refusal = judge_empty(&fake).expect_err("the value property must never prove a clear");
    assert_eq!(refusal.name(), "NotEmpty");
}

#[test]
fn an_empty_document_with_carrier_ink_is_not_a_clear_proof() {
    let fake = FakeJudge {
        uia: Some(leaves(&["\u{feff}", "\n"], "")),
        text_pattern: Some(leaves(&["\u{feff}", "\n"], "")),
        ink: Some(inked(empty_ink().inked + DISCORD.min_ink_delta)),
        value_property: Some(String::new()),
        ..FakeJudge::default()
    };

    let refusal = judge_empty(&fake).expect_err("empty leaves without empty ink are not enough");
    assert_eq!(refusal.name(), "StillInked");
}

// ---------------------------------------------------------------------------
// 2 · A truncated carrier
// ---------------------------------------------------------------------------

#[test]
fn a_truncated_carrier_is_refused_by_name() {
    let short = &CARRIER[..17];
    let fake = FakeJudge {
        uia: Some(document(short)),
        text_pattern: Some(document(short)),
        ink: Some(inked(600)),
        value_property: Some(short.to_owned()),
        ..FakeJudge::default()
    };
    let refusal = judge(&fake).expect_err("a dropped chunk must not earn a landing");
    assert_eq!(refusal.name(), "Truncated");
    let LandingRefusal::Truncated {
        placed_chars,
        expected_chars,
        ..
    } = refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert_eq!(placed_chars, 17);
    assert_eq!(expected_chars, CARRIER.chars().count());
}

// ---------------------------------------------------------------------------
// 3 · A carrier the composer re-wrapped or re-encoded
// ---------------------------------------------------------------------------

#[test]
fn a_rewrapped_carrier_is_refused_by_name() {
    // Slate stores a hard break as a block boundary. The shipping path types
    // `\n` as Shift+Enter (native_discord_adapter.rs:1579) and the leaves come
    // back split, with no `\n` anywhere in them.
    let expected = "first line\nsecond line";
    let fake = FakeJudge {
        uia: Some(leaves(&["first line", "second line"], "")),
        text_pattern: Some(leaves(&["first line", "second line"], "")),
        ink: Some(inked(1_100)),
        value_property: Some("first line\nsecond line".to_owned()),
        ..FakeJudge::default()
    };
    let refusal = judge_landing(&fake, &DISCORD, &bound(), expected, baseline(), &[])
        .expect_err("a re-encoded document is not the carrier");
    assert_eq!(refusal.name(), "Rewrapped");
    let LandingRefusal::Rewrapped {
        document,
        normalised,
        applied,
    } = refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert_eq!(document, "first linesecond line");
    assert_eq!(normalised, "first linesecond line");
    assert!(applied.contains(&Normalisation::BlockBreaksAreNotNewlines));
}

#[test]
fn a_normalisation_can_never_reach_a_landing() {
    // The same fixture as above. Whatever a normalisation does, `Landed`
    // requires byte-exact equality, so no widening of the normaliser list can
    // turn this into a pass — only into a differently-named refusal.
    let expected = "first line\nsecond line";
    let fake = FakeJudge {
        uia: Some(leaves(&["first line", "second line"], "")),
        text_pattern: Some(leaves(&["first line", "second line"], "")),
        ink: Some(inked(1_100)),
        ..FakeJudge::default()
    };
    let verdict = judge_landing(&fake, &DISCORD, &bound(), expected, baseline(), &[]);
    assert!(
        verdict.is_err(),
        "a document that is not byte-exactly the carrier must never be a proof"
    );
}

#[test]
fn text_that_is_not_the_carrier_at_all_is_refused_by_name() {
    let fake = FakeJudge {
        uia: Some(document("what time are you around tomorrow")),
        text_pattern: Some(document("what time are you around tomorrow")),
        ink: Some(inked(1_000)),
        ..FakeJudge::default()
    };
    let refusal = judge(&fake).expect_err("a stranger's draft is not the carrier");
    assert_eq!(refusal.name(), "ForeignText");
}

// ---------------------------------------------------------------------------
// 4 · Text placed in the wrong window
// ---------------------------------------------------------------------------

#[test]
fn a_bound_window_of_the_wrong_process_is_refused_by_name() {
    // D-211's shape: `same_process_name` strips only `.exe`, so `DiscordPTB`
    // is not `Discord` and a plan naming one can be bound to the other.
    let fake = FakeJudge {
        identity: Some(WindowIdentity {
            hwnd: 0x1234,
            process_id: 20652,
            process_name: "DiscordPTB".to_owned(),
        }),
        uia: Some(document(CARRIER)),
        text_pattern: Some(document(CARRIER)),
        ink: Some(inked(1_400)),
        ..FakeJudge::default()
    };
    let refusal = judge(&fake).expect_err("the wrong Discord is the wrong window");
    assert_eq!(refusal.name(), "WrongWindow");
    let LandingRefusal::WrongWindow(WrongWindowReason::BoundProcessMismatch { expected, found }) =
        refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert_eq!(expected, "Discord");
    assert_eq!(found, "DiscordPTB");
}

#[test]
fn a_carrier_that_landed_in_another_window_is_refused_by_name() {
    let stray_hwnd = 0x9999;
    let fake = FakeJudge {
        uia: Some(leaves(&["\u{feff}", "\n"], "")),
        text_pattern: Some(leaves(&["\u{feff}", "\n"], "")),
        ink: Some(empty_ink()),
        strays: vec![(
            stray_hwnd,
            WindowIdentity {
                hwnd: stray_hwnd,
                process_id: 20652,
                process_name: "DiscordPTB".to_owned(),
            },
            document(CARRIER),
        )],
        ..FakeJudge::default()
    };
    let stray = BoundComposer {
        hwnd: stray_hwnd,
        ..bound()
    };
    let refusal = judge_landing(&fake, &DISCORD, &bound(), CARRIER, baseline(), &[stray])
        .expect_err("landing somewhere else is not landing here");
    assert_eq!(refusal.name(), "WrongWindow");
    let LandingRefusal::WrongWindow(WrongWindowReason::CarrierLandedInAnotherWindow { found_in }) =
        refusal
    else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert_eq!(found_in.process_name, "DiscordPTB");
}

// ---------------------------------------------------------------------------
// The structural refusal — the one that makes D-205 unrepresentable
// ---------------------------------------------------------------------------

#[test]
fn a_profile_that_judges_by_the_writing_channel_is_refused_before_any_call() {
    static SELF_JUDGING: LandingProfile = LandingProfile {
        // The one thing changed: it judges through the channel it writes.
        judges: &[JudgeChannel::ComposerValueProperty],
        document_channel: JudgeChannel::RenderedDocumentUia,
        write_channel: WriteChannel::ValueSet,
        provider: NativeAppId::Discord,
        provider_name: "Discord",
        process_name: "Discord",
        empty_document_chars: &['\u{feff}', '\n'],
        leaf_join: "",
        matcher: DISCORD.matcher,
        wake: true,
        settle_ms: 250,
        walk: WalkCaps {
            max_nodes: 256,
            max_depth: 8,
        },
        judge_timeout_ms: 5_000,
        commit_key: "Enter",
        min_ink_delta: 16,
        normalisations: &[],
    };
    // A judge that would answer anything at all, to prove the refusal happens
    // before it is consulted.
    struct Exploding;
    impl LandingJudgeSyscalls for Exploding {
        fn window_identity(
            &self,
            _: isize,
            _: JudgeDeadline,
        ) -> Result<WindowIdentity, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn rendered_document_uia(
            &self,
            _: &BoundComposer,
            _: WalkCaps,
            _: &str,
            _: JudgeDeadline,
        ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn rendered_document_text_pattern(
            &self,
            _: &BoundComposer,
            _: WalkCaps,
            _: JudgeDeadline,
        ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn rendered_document_msaa(
            &self,
            _: &BoundComposer,
            _: WalkCaps,
            _: &str,
            _: JudgeDeadline,
        ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn composer_ink(
            &self,
            _: &BoundComposer,
            _: JudgeDeadline,
        ) -> Result<Option<Ink>, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn disowned_value_property(
            &self,
            _: &BoundComposer,
            _: JudgeDeadline,
        ) -> Result<Option<String>, JudgeTimeout> {
            panic!("the oracle must refuse before it touches the provider");
        }
        fn submit_shaped_calls(&self) -> usize {
            panic!("the oracle must refuse before it touches the provider");
        }
    }

    let refusal = judge_landing(
        &Exploding,
        &SELF_JUDGING,
        &bound(),
        CARRIER,
        baseline(),
        &[],
    )
    .expect_err("a self-judging profile must be refused");
    assert_eq!(refusal.name(), "JudgedByTheWritingChannel");
}

/// The same disqualification for the *other* doctrine. The rule is not
/// "do not judge with what you wrote"; it is "a property one doctrine can move
/// without the document moving is not evidence of the document".
#[test]
fn the_value_property_is_disqualified_for_synthesized_input_too() {
    assert!(!judges_independently(
        WriteChannel::SynthesizedInput,
        JudgeChannel::ComposerValueProperty
    ));
    assert!(!judges_independently(
        WriteChannel::ValueSet,
        JudgeChannel::ComposerValueProperty
    ));
    assert!(!judges_independently(
        WriteChannel::ClipboardPaste,
        JudgeChannel::ComposerValueProperty
    ));
}

#[test]
fn every_shipped_profile_judges_independently() {
    for provider in [
        NativeAppId::Discord,
        NativeAppId::Telegram,
        NativeAppId::Instagram,
        NativeAppId::Signal,
        NativeAppId::Whatsapp,
        NativeAppId::Outlook,
    ] {
        if let ProfileLookup::Measured(profile) = landing_profile(provider) {
            check_independence(profile)
                .unwrap_or_else(|refusal| panic!("{provider:?}: {refusal:?}"));
            assert!(
                !profile
                    .judges
                    .contains(&JudgeChannel::ComposerValueProperty),
                "{provider:?} judges through the disowned channel"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// On screen
// ---------------------------------------------------------------------------

#[test]
fn a_document_the_pixels_cannot_see_is_refused_by_name() {
    // The a11y tree says the carrier is there and the composer's rectangle is
    // exactly as empty as it was. D-220's third blocker: placement has never
    // been proven to land ON SCREEN.
    let fake = FakeJudge {
        ink: Some(inked(empty_ink().inked + DISCORD.min_ink_delta - 1)),
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("a document nobody can see has not landed");
    assert_eq!(refusal.name(), "NotOnScreen");
}

// ---------------------------------------------------------------------------
// Channel disagreement, bounds, timeouts, submit
// ---------------------------------------------------------------------------

#[test]
fn two_judging_channels_that_disagree_are_refused_by_name() {
    let fake = FakeJudge {
        text_pattern: Some(document("something else entirely")),
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("two answers and no way to choose is a refusal");
    assert_eq!(refusal.name(), "JudgingChannelsDisagree");
}

#[test]
fn a_composer_that_publishes_no_leaf_is_not_called_empty() {
    let fake = FakeJudge {
        uia: None,
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("silence is not emptiness");
    assert_eq!(refusal.name(), "NoRenderedDocument");
}

#[test]
fn a_walk_that_reaches_its_bound_is_refused_by_name() {
    let fake = FakeJudge {
        uia: Some(RenderedDocument {
            leaves: vec![CARRIER.to_owned()],
            text: CARRIER.to_owned(),
            nodes_visited: DISCORD.walk.max_nodes,
            depth_reached: 2,
        }),
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("a bound reached is a refusal, never a truncation");
    assert_eq!(refusal.name(), "JudgeWalkTooLarge");
}

#[test]
fn a_channel_that_stops_answering_is_refused_by_name() {
    // Every channel the oracle depends on, starved one at a time. Silence must
    // never become a verdict.
    for channel in ["identity", "uia", "text_pattern", "value", "ink"] {
        let fake = FakeJudge {
            timeout_on: Some(channel),
            ..landed_judge()
        };
        match judge(&fake) {
            Ok(_) => panic!("{channel} stopped answering and the oracle still issued a proof"),
            Err(refusal) => assert_eq!(refusal.name(), "JudgeTimedOut", "channel {channel}"),
        }
    }
}

#[test]
fn a_submit_shaped_interaction_stops_everything() {
    let fake = FakeJudge {
        submit_shaped: 1,
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("nothing runs after a submit-shaped interaction");
    assert_eq!(refusal.name(), "SubmitShaped");
}

#[test]
fn the_empty_expectation_is_refused() {
    let refusal = judge_landing(&landed_judge(), &DISCORD, &bound(), "", baseline(), &[])
        .expect_err("the empty string cannot land");
    assert_eq!(refusal.name(), "EmptyExpectation");
}

// ---------------------------------------------------------------------------
// Provider parameterisation
// ---------------------------------------------------------------------------

#[test]
fn an_unmeasured_provider_gets_no_neighbours_numbers() {
    // WhatsApp left this list on 2026-08-05 by being measured, not by being
    // excused: its profile is pinned field by field in
    // `whatsapps_profile_records_what_was_measured_rather_than_what_is_usual`,
    // and the run that took the figures wrote nothing into a conversation.
    for provider in [
        NativeAppId::Signal,
        NativeAppId::Instagram,
        NativeAppId::Outlook,
    ] {
        let ProfileLookup::Unmeasured { missing, .. } = landing_profile(provider) else {
            panic!("{provider:?} claims a measured profile it has not earned");
        };
        assert!(
            missing.len() > 60,
            "{provider:?} must name what measurement is missing, not merely that one is"
        );
    }
}

#[test]
fn exactly_three_surfaces_have_a_measured_profile_today() {
    // Discord was the only surface that carried. WhatsApp is the second, and
    // this test is the place that claim is recorded -- not a place to widen
    // quietly. WhatsApp's profile was measured live on 2026-08-05 through this
    // module's own judges (`live_whatsapp`), by a run that wrote nothing into
    // the conversation composer to take it.
    let measured: Vec<_> = [
        NativeAppId::Discord,
        NativeAppId::Telegram,
        NativeAppId::Instagram,
        NativeAppId::Signal,
        NativeAppId::Whatsapp,
        NativeAppId::Outlook,
    ]
    .into_iter()
    .filter(|provider| matches!(landing_profile(*provider), ProfileLookup::Measured(_)))
    .collect();
    assert_eq!(
        measured,
        vec![
            NativeAppId::Discord,
            NativeAppId::Telegram,
            NativeAppId::Whatsapp
        ],
        "exactly three surfaces have a measured landing profile; every other provider must still \
         name the measurement it is missing"
    );
}

/// WhatsApp's profile is a record of what was read on the owner's host, and
/// each of these is a figure a later lane could otherwise quietly relax.
#[test]
fn whatsapps_profile_records_what_was_measured_rather_than_what_is_usual() {
    assert_eq!(WHATSAPP.write_channel, WriteChannel::ValueSet);
    assert!(
        WHATSAPP.wake,
        "WhatsApp's WebView2 renderer does not serve its tree before the handshake"
    );
    assert_eq!(WHATSAPP.commit_key, "Enter");
    assert_eq!(
        WHATSAPP.empty_document_chars,
        &['\n'],
        "WhatsApp's empty composer publishes exactly \"\\n\" through both document channels; \
         widening this set weakens every 'provably empty' claim made after a clear"
    );
    assert!(
        WHATSAPP.normalisations.is_empty(),
        "no re-encoding has been measured on WhatsApp, so none may be declared -- a declared \
         normalisation silences a channel disagreement and must cost a measurement"
    );
    assert!(
        !WHATSAPP
            .judges
            .contains(&JudgeChannel::RenderedDocumentMsaa),
        "Chromium's LegacyIAccessible bridge answered None on every live read here, so naming \
         it would be a corroborator that cannot corroborate"
    );
    assert_eq!(
        WHATSAPP.process_name, "msedgewebview2",
        "the bound window is WhatsApp's WebView2 renderer, not its shell; the shell's parentage \
         is enforced by the acquisition plan, not by this field"
    );
    // The measured ink rate was 7.53 px/char on this host. The floor is five
    // characters' worth, by the same rule Discord's 48 was set by.
    assert_eq!(WHATSAPP.min_ink_delta, 38);
}

#[test]
fn discords_profile_records_what_was_measured_rather_than_what_is_usual() {
    assert_eq!(DISCORD.write_channel, WriteChannel::SynthesizedInput);
    assert!(DISCORD.wake, "Discord's MsaaBridge route needs the wake");
    assert_eq!(DISCORD.commit_key, "Enter");
    assert!(
        DISCORD.empty_document_chars.contains(&'\u{feff}'),
        "Discord's empty Slate composer publishes U+FEFF and str::trim does not remove it"
    );
}

/// A declared corroborator that answers nothing is a refusal, not a shrug.
///
/// This is the gate that stopped Discord's profile from claiming an MSAA
/// corroborator it does not have: the live run measured
/// `rendered_document_msaa -> None` against Chromium's `LegacyIAccessible`
/// bridge, and rather than leave a channel in the profile that could never
/// disagree with anything, the profile names `RenderedDocumentTextPattern`,
/// which answers.
#[test]
fn a_declared_channel_that_answers_nothing_is_refused_by_name() {
    let fake = FakeJudge {
        text_pattern: None,
        ..landed_judge()
    };
    let refusal = judge(&fake).expect_err("a silent corroborator cannot corroborate");
    assert_eq!(refusal.name(), "CorroboratingChannelSilent");
    let LandingRefusal::CorroboratingChannelSilent { channel } = refusal else {
        panic!("wrong refusal: {refusal:?}");
    };
    assert_eq!(channel, JudgeChannel::RenderedDocumentTextPattern);
}

// ---------------------------------------------------------------------------
// The primary document channel — added 2026-08-05, and held to MORE than a
// corroborator is
// ---------------------------------------------------------------------------

/// `document_channel` was hard-wired to `RenderedDocumentUia` until Telegram
/// showed that hard-wiring encoded a Chromium assumption. Making it declarable
/// must not make it a hole, so all three of its guards are driven here by
/// starving what would have earned the pass — and each is checked **before any
/// cross-process call**, proven by a judge whose every method panics.
struct Exploding2;
impl LandingJudgeSyscalls for Exploding2 {
    fn window_identity(&self, _: isize, _: JudgeDeadline) -> Result<WindowIdentity, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn rendered_document_uia(
        &self,
        _: &BoundComposer,
        _: WalkCaps,
        _: &str,
        _: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn rendered_document_text_pattern(
        &self,
        _: &BoundComposer,
        _: WalkCaps,
        _: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn rendered_document_msaa(
        &self,
        _: &BoundComposer,
        _: WalkCaps,
        _: &str,
        _: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn composer_ink(
        &self,
        _: &BoundComposer,
        _: JudgeDeadline,
    ) -> Result<Option<Ink>, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn disowned_value_property(
        &self,
        _: &BoundComposer,
        _: JudgeDeadline,
    ) -> Result<Option<String>, JudgeTimeout> {
        panic!("the oracle must refuse before it touches the provider");
    }
    fn submit_shaped_calls(&self) -> usize {
        panic!("the oracle must refuse before it touches the provider");
    }
}

#[test]
fn the_value_property_can_never_be_the_primary_document_channel() {
    let value_as_document = LandingProfile {
        judges: &[
            JudgeChannel::ComposerValueProperty,
            JudgeChannel::ComposerInk,
        ],
        document_channel: JudgeChannel::ComposerValueProperty,
        ..DISCORD
    };
    // Caught by the independence table first, which is the stronger refusal.
    let refusal = check_independence(&value_as_document).expect_err("D-205 made unrepresentable");
    assert_eq!(refusal.name(), "JudgedByTheWritingChannel");
    let refusal = judge_landing(
        &Exploding2,
        &value_as_document,
        &bound(),
        CARRIER,
        baseline(),
        &[],
    )
    .expect_err("the disowned channel cannot be the document");
    assert_eq!(refusal.name(), "JudgedByTheWritingChannel");
}

#[test]
fn ink_is_not_a_document_and_cannot_be_the_primary_channel() {
    let ink_as_document = LandingProfile {
        judges: &[JudgeChannel::ComposerInk],
        document_channel: JudgeChannel::ComposerInk,
        ..DISCORD
    };
    assert!(!is_document_channel(JudgeChannel::ComposerInk));
    let refusal = check_independence(&ink_as_document).expect_err("pixels are not a document");
    assert_eq!(refusal.name(), "InvalidDocumentChannel");
    let refusal = judge_landing(
        &Exploding2,
        &ink_as_document,
        &bound(),
        CARRIER,
        baseline(),
        &[],
    )
    .expect_err("pixels are not a document");
    assert_eq!(refusal.name(), "InvalidDocumentChannel");
    let refusal = judge_empty_composer(&Exploding2, &ink_as_document, &bound(), baseline())
        .expect_err("the empty judge takes the same guard");
    assert_eq!(refusal.name(), "InvalidDocumentChannel");
}

#[test]
fn a_primary_channel_that_is_not_declared_in_judges_is_refused() {
    // The whole point of the independence table is that every judging channel
    // passes it. A primary that is not in `judges` would be the one channel
    // that never did.
    let undeclared = LandingProfile {
        judges: &[JudgeChannel::RenderedDocumentUia, JudgeChannel::ComposerInk],
        document_channel: JudgeChannel::RenderedDocumentMsaa,
        ..DISCORD
    };
    let refusal = check_independence(&undeclared).expect_err("an undeclared primary is a hole");
    assert_eq!(refusal.name(), "InvalidDocumentChannel");
    let LandingRefusal::InvalidDocumentChannel { channel, .. } = refusal else {
        panic!("wrong refusal");
    };
    assert_eq!(channel, JudgeChannel::RenderedDocumentMsaa);
}

#[test]
fn every_shipped_profile_declares_its_primary_channel_in_its_judges() {
    for provider in [
        NativeAppId::Discord,
        NativeAppId::Telegram,
        NativeAppId::Instagram,
        NativeAppId::Signal,
        NativeAppId::Whatsapp,
        NativeAppId::Outlook,
    ] {
        if let ProfileLookup::Measured(profile) = landing_profile(provider) {
            assert!(
                is_document_channel(profile.document_channel),
                "{provider:?} judges a document through a channel that publishes none"
            );
            assert!(
                profile.judges.contains(&profile.document_channel),
                "{provider:?}'s primary channel skips the independence table"
            );
            assert!(
                judges_independently(profile.write_channel, profile.document_channel),
                "{provider:?} reads its document through the channel that wrote it"
            );
        }
    }
}

/// The two Chromium surfaces must not move: making the channel declarable was
/// meant to change nothing for them, and this is where that is recorded.
#[test]
fn the_chromium_surfaces_still_judge_through_the_leaf_walk() {
    assert_eq!(DISCORD.document_channel, JudgeChannel::RenderedDocumentUia);
    assert_eq!(WHATSAPP.document_channel, JudgeChannel::RenderedDocumentUia);
}

/// Telegram's profile is a record of what was read on the owner's live host,
/// and each of these is a figure a later lane could otherwise quietly relax.
#[test]
fn telegrams_profile_records_what_was_measured_rather_than_what_is_usual() {
    assert_eq!(TELEGRAM.write_channel, WriteChannel::ValueSet);
    assert!(
        !TELEGRAM.wake,
        "Qt publishes its UIA tree eagerly; woke=false was measured on every acquisition, and \
         sending Chromium's handshake here would be a call to an object that does not want it"
    );
    assert_eq!(TELEGRAM.commit_key, "Enter");
    assert_eq!(
        TELEGRAM.document_channel,
        JudgeChannel::RenderedDocumentTextPattern,
        "the leaf walk answered None on Telegram's composer with the field empty AND with 32 \
         characters in it; a channel that cannot tell those apart is not this surface's document"
    );
    assert!(
        !TELEGRAM.judges.contains(&JudgeChannel::RenderedDocumentUia),
        "declaring the leaf walk here would be a corroborator that cannot corroborate"
    );
    assert!(
        !TELEGRAM
            .judges
            .contains(&JudgeChannel::RenderedDocumentMsaa),
        "on Qt a childless composer's MSAA leaf read collapses to accValue, which is the \
         disowned value property under another name"
    );
    assert!(
        TELEGRAM.judges.contains(&JudgeChannel::ComposerInk),
        "with one document channel the pixels are the corroborator, so they are not optional here"
    );
    assert!(
        TELEGRAM.empty_document_chars.is_empty(),
        "Telegram's empty composer publishes the EMPTY STRING; an empty character set is the \
         narrowest possible claim and widening it is how a residue would pass as clear"
    );
    assert!(
        TELEGRAM.normalisations.is_empty(),
        "no re-encoding has been measured on Telegram, so none may be declared"
    );
    // The measured ink rate was 22.53 px/char on this host. The floor is five
    // characters' worth, by the same rule Discord's 48 and WhatsApp's 38 were
    // set by.
    assert_eq!(TELEGRAM.min_ink_delta, 112);
    assert_eq!(
        TELEGRAM.walk.max_nodes,
        crate::native_telegram_adapter::TELEGRAM_LIVE_CARRIER_MAX_BYTES,
        "on this profile max_nodes bounds GetText's character count; anything below the shipping \
         carrier bound would silently truncate a carrier the shipping path is willing to place"
    );
}

/// **The empty string is the only empty state Telegram admits.** Starve it: one
/// stray character and the composer is not empty, whatever the ink says.
#[test]
fn telegram_admits_no_sentinel_as_an_empty_composer() {
    assert!(is_empty_document("", TELEGRAM.empty_document_chars));
    for residue in ["\n", "\u{feff}", " ", "o"] {
        assert!(
            !is_empty_document(residue, TELEGRAM.empty_document_chars),
            "{residue:?} must not read as an empty Telegram composer"
        );
    }
}
