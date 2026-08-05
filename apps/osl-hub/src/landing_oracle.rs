//! The landing oracle — the instrument that answers *"did this exact text land
//! in the composer?"* through a channel that did not write it.
//!
//! # Why this module exists
//!
//! D-205 ran the live experiment and had to disown its own strongest signal.
//! It wrote with `IValueProvider::SetValue` and then judged with
//! `IValueProvider::CurrentValue` — the same node, the same property — so
//! `readback_holds_carrier=true` was OSL comparing the provider's answer to
//! OSL's own input. Slate's document never changed. Nothing was on screen.
//! The same probe produced the false belief that `contenteditable` had no
//! write path (D-220, corrected: Discord carries one daily through 23
//! `SendInput` sites, and `SetValue` is rejected in-source at
//! `native_discord_adapter.rs:1557-1563` because *"Slate only builds its
//! document model from real input events"*).
//!
//! > **The writing channel and the judging channel were the same channel, and
//! > that is the defect.**
//!
//! # The rule this module enforces
//!
//! **A property that one sanctioned write doctrine can move without the
//! document moving is not evidence of the document under any doctrine.**
//!
//! `ValuePattern`'s value is exactly such a property, so it is disqualified as
//! a judge for *both* doctrines — not only for the one that writes it. It is
//! still read, on every judgement, and recorded as
//! [`LandingProof::disowned_value_property`]; it never votes. When it disagrees
//! with the document the oracle says so, which is D-205 reproduced as a
//! measurement rather than inherited as a belief.
//!
//! # The channels
//!
//! | | channel | reaches the composer through |
//! |---|---|---|
//! | **W1** | [`WriteChannel::SynthesizedInput`] | `SendInput` → the OS input queue → Chromium's renderer → Slate's `beforeinput` → its document model → React → DOM → layout |
//! | **W2** | [`WriteChannel::ValueSet`] | `IUIAutomation` → the provider's `IValueProvider::SetValue` |
//! | **J1** | [`JudgeChannel::RenderedDocumentUia`] | the accessible text of the **leaves of the composer element's own subtree**, which Blink builds from the **layout tree** |
//! | **J2** | [`JudgeChannel::RenderedDocumentMsaa`] | the same leaves through Chromium's `IAccessible` provider — a different COM interface, a different marshalling path, the same layout tree |
//! | **J3** | [`JudgeChannel::ComposerInk`] | GDI pixels inside the composer's own bounding rectangle |
//! | **D** | [`JudgeChannel::ComposerValueProperty`] | `IValueProvider::CurrentValue` — **disowned; never a judge** |
//!
//! **Why W and J cannot both be fooled by one failure.** For **W2** the
//! disqualification is measured, not argued: D-205 moved the value while the
//! leaves stayed at Discord's empty sentinel. J1/J2 read *different nodes* than
//! the one `SetValue` writes, and those nodes' text is produced by layout. For
//! **W1** nothing about `SendInput` touches the accessibility tree at all — it
//! posts to the OS input queue, and for J1/J2 to report the carrier, Slate's
//! model must have changed, React must have rendered, layout must have run, and
//! Chromium must have rebuilt the AX subtree. J3 is downstream of nothing but
//! the compositor. **In neither direction is there a short path from OSL's own
//! input to a judge's answer**: the expected string is never written into any
//! store a judge reads.
//!
//! # What it will not do
//!
//! There is no verb in [`LandingJudgeSyscalls`] that could commit. No key, no
//! `Invoke`, no window message, no pattern that activates a control. The trait
//! is read-only by construction, and [`LandingProfile::commit_key`] exists so a
//! probe can *name* the key it must never have sent.

use crate::native_a11y::{Uia2ComposerMatcher, Uia2Editable, Uia2TreeRoute};
use crate::native_apps::NativeAppId;

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

/// How a placement primitive put text into a composer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteChannel {
    /// Win32 `SendInput` synthesized keystrokes. Discord's shipping doctrine:
    /// `send_unicode_chunk` at `native_discord_adapter.rs:15429`, driven from
    /// `place()` at `:18017`.
    SynthesizedInput,
    /// UIA `IValueProvider::SetValue`. The substrate's doctrine:
    /// `native_a11y.rs:2019`.
    ValueSet,
    /// The clipboard plus a synthesized paste. Not used by any shipping
    /// adapter today; present so the independence table is total rather than
    /// convenient.
    ClipboardPaste,
}

impl WriteChannel {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SynthesizedInput => "SendInput synthesized keystrokes",
            Self::ValueSet => "IValueProvider::SetValue",
            Self::ClipboardPaste => "clipboard + synthesized paste",
        }
    }
}

/// A channel a verdict may be read through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JudgeChannel {
    /// The composer element's own subtree leaves, through UIA.
    RenderedDocumentUia,
    /// The same leaves through Chromium's MSAA provider.
    RenderedDocumentMsaa,
    /// Pixels inside the composer's bounding rectangle.
    ComposerInk,
    /// `IValueProvider::CurrentValue`. **Never a judge.** Declared so the
    /// disqualification is a value in the type system rather than a comment.
    ComposerValueProperty,
}

impl JudgeChannel {
    pub const fn name(self) -> &'static str {
        match self {
            Self::RenderedDocumentUia => "rendered document (UIA subtree leaves)",
            Self::RenderedDocumentMsaa => "rendered document (MSAA subtree leaves)",
            Self::ComposerInk => "composer ink (GDI pixels)",
            Self::ComposerValueProperty => "IValueProvider::CurrentValue",
        }
    }
}

/// **The independence table.** Exhaustive in both arguments, so a new channel
/// on either side cannot compile without a decision about every pairing.
///
/// `ComposerValueProperty` is `false` for **every** write channel, including
/// the ones that do not write it. That is the module's central rule: a property
/// one sanctioned doctrine can move without the document moving is not evidence
/// of the document under any doctrine.
pub const fn judges_independently(write: WriteChannel, judge: JudgeChannel) -> bool {
    match (write, judge) {
        // Measured at D-205: SetValue moved this property while Slate's
        // document did not move. Self-referential.
        (WriteChannel::ValueSet, JudgeChannel::ComposerValueProperty) => false,
        // Not self-referential, but disqualified by the rule above: the
        // property has been shown to be movable independently of the document,
        // so its agreement proves nothing about the document.
        (WriteChannel::SynthesizedInput, JudgeChannel::ComposerValueProperty) => false,
        (WriteChannel::ClipboardPaste, JudgeChannel::ComposerValueProperty) => false,
        // Leaves are produced by layout, not by the value property.
        (_, JudgeChannel::RenderedDocumentUia) => true,
        (_, JudgeChannel::RenderedDocumentMsaa) => true,
        // Pixels are downstream of the compositor and of nothing else.
        (_, JudgeChannel::ComposerInk) => true,
    }
}

// ---------------------------------------------------------------------------
// Deadlines and bounds
// ---------------------------------------------------------------------------

/// A per-call budget. Constructed only from a [`LandingProfile`], so no caller
/// can invent an unbounded cross-process call. An unbounded accessibility walk
/// hard-froze a shipping app on 2026-08-04 (`native_window_host.rs:5138-5160`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JudgeDeadline(u64);

impl JudgeDeadline {
    pub(crate) const fn from_profile(profile: &LandingProfile) -> Self {
        Self(profile.judge_timeout_ms)
    }

    pub const fn millis(self) -> u64 {
        self.0
    }
}

/// Hard caps on a judging walk. **Reaching a bound is a refusal, never a
/// truncation** — the same rule `ComposerTextAccumulator` established at
/// `native_discord_adapter.rs:4503-4511`. A judge that silently returned the
/// first 256 nodes of a larger document would answer `Truncated` for a document
/// that had in fact landed whole.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalkCaps {
    pub max_nodes: usize,
    pub max_depth: usize,
}

// ---------------------------------------------------------------------------
// What a judge returns
// ---------------------------------------------------------------------------

/// The composer's rendered document, as one judging channel saw it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDocument {
    /// The leaves, in document order, exactly as the provider published them.
    pub leaves: Vec<String>,
    /// The leaves joined by the provider's declared join.
    pub text: String,
    /// How many nodes the walk visited. Reported so a bound that was nearly
    /// reached is visible before it is reached.
    pub nodes_visited: usize,
    /// The deepest level the walk reached.
    pub depth_reached: usize,
}

/// A screen rectangle, in screen coordinates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn width(self) -> i32 {
        self.right - self.left
    }
    pub const fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub const fn is_degenerate(self) -> bool {
        self.width() <= 0 || self.height() <= 0
    }
}

/// How much of a rectangle is not its own background colour.
///
/// Deliberately **not** an identity channel: ink cannot say *which* text is on
/// screen, only that something is. It is the corroborator for
/// *"the carrier is visible"*, which is 3 of the 7 promotion predicates and the
/// one thing D-205's `readback_holds_carrier` could not see.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ink {
    pub rect: Rect,
    /// Pixels examined.
    pub sampled: u32,
    /// Pixels far enough from the rectangle's modal colour to be a glyph.
    pub inked: u32,
}

/// The identity of a window at judging time, read fresh rather than trusted
/// from the binding. D-211 is the defect this exists for: `DiscordPTB` is not
/// `Discord`, and a plan that names one while bound to the other resolves
/// silently.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowIdentity {
    pub hwnd: isize,
    pub process_id: u32,
    /// The image name with any `.exe` suffix removed.
    pub process_name: String,
}

/// A composer the oracle has been pointed at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundComposer {
    pub hwnd: isize,
    pub route: Uia2TreeRoute,
    pub process_id: u32,
    pub composer: Uia2Editable,
}

/// The ink in the composer's rectangle while it was **provably empty**, taken
/// before the placement. Without it the ink channel can only say
/// *"there are pixels"*, which is true of an empty composer with a placeholder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LandingBaseline {
    pub empty_ink: Ink,
}

// ---------------------------------------------------------------------------
// The syscall seam
// ---------------------------------------------------------------------------

/// A judging call did not answer inside its budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JudgeTimeout {
    pub millis: u64,
}

/// The read-only cross-process vocabulary the oracle is allowed.
///
/// **There is no write verb here and no verb that could commit.** That is the
/// same discipline `Uia2Syscalls` adopted (`native_a11y.rs:965-972`), tightened:
/// that trait still has `set_value`, and this one has nothing at all.
pub trait LandingJudgeSyscalls {
    fn window_identity(
        &self,
        hwnd: isize,
        deadline: JudgeDeadline,
    ) -> Result<WindowIdentity, JudgeTimeout>;

    /// **J1.** `None` means the composer published no text leaf at all, which
    /// is never the same claim as "the composer is empty".
    fn rendered_document_uia(
        &self,
        bound: &BoundComposer,
        caps: WalkCaps,
        join: &str,
        deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout>;

    /// **J2.** The same leaves through Chromium's `IAccessible` provider.
    fn rendered_document_msaa(
        &self,
        bound: &BoundComposer,
        caps: WalkCaps,
        join: &str,
        deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout>;

    /// **J3.** Ink inside the composer's own bounding rectangle.
    fn composer_ink(
        &self,
        bound: &BoundComposer,
        deadline: JudgeDeadline,
    ) -> Result<Option<Ink>, JudgeTimeout>;

    /// **D — the disowned channel.** Read on every judgement so the two can be
    /// compared, and counted by nothing.
    fn disowned_value_property(
        &self,
        bound: &BoundComposer,
        deadline: JudgeDeadline,
    ) -> Result<Option<String>, JudgeTimeout>;

    /// How many submit-shaped interactions this backend has observed since it
    /// was built. Read at the start and end of every judgement so the verdict's
    /// `submit_shaped` field comes from the backend rather than from a literal
    /// in the caller — D-139's finding 2 was exactly a receipt field that could
    /// not be false.
    fn submit_shaped_calls(&self) -> usize;
}

// ---------------------------------------------------------------------------
// Provider profiles
// ---------------------------------------------------------------------------

/// A re-encoding a provider is **measured** to perform on text it accepts.
///
/// These exist only to give a refusal a better name. **No normalisation can
/// ever produce [`LandingProof`]** — see [`judge_landing`], where `Landed`
/// requires byte-exact equality of the rendered document with the expectation
/// and nothing else. A normalisation can only move a verdict from
/// [`LandingRefusal::ForeignText`] to [`LandingRefusal::Rewrapped`]: refusal to
/// refusal. That is deliberate. Every "widen the matcher until it passes"
/// failure this project has recorded became possible the moment a normaliser
/// could reach a pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Normalisation {
    /// The composer stores a hard line break as a block boundary, so the break
    /// is absent from the leaf text entirely. Measured on Discord's Slate: the
    /// shipping path types `\n` as Shift+Enter
    /// (`native_discord_adapter.rs:1579`), and the leaves come back split with
    /// no `\n` in them.
    BlockBreaksAreNotNewlines,
    /// The composer replaces runs of spaces with no-break spaces so they render.
    SpacesBecomeNoBreakSpaces,
    /// The composer keeps a zero-width sentinel in the document at all times.
    ZeroWidthSentinelRetained,
}

impl Normalisation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::BlockBreaksAreNotNewlines => "block breaks are not newlines",
            Self::SpacesBecomeNoBreakSpaces => "spaces become no-break spaces",
            Self::ZeroWidthSentinelRetained => "zero-width sentinel retained",
        }
    }

    fn apply(self, text: &str) -> String {
        match self {
            Self::BlockBreaksAreNotNewlines => {
                text.chars().filter(|c| *c != '\n' && *c != '\r').collect()
            }
            Self::SpacesBecomeNoBreakSpaces => text
                .chars()
                .map(|c| if c == '\u{a0}' { ' ' } else { c })
                .collect(),
            Self::ZeroWidthSentinelRetained => text
                .chars()
                .filter(|c| *c != '\u{feff}' && *c != '\u{200b}')
                .collect(),
        }
    }
}

fn normalise(text: &str, normalisations: &[Normalisation]) -> String {
    normalisations
        .iter()
        .fold(text.to_owned(), |acc, n| n.apply(&acc))
}

/// Everything about a provider that the oracle refuses to infer.
///
/// **Route, wake, settle, matcher, conversation binder and commit semantics all
/// differ per provider, and never one from another's.** Telegram needs no wake
/// and settles at 0 ms; Signal needs a wake and settles at 1950 ms. A profile is
/// a record of measurements, and a provider without one gets
/// [`LandingRefusal::ProviderNotMeasured`] rather than a neighbour's numbers.
#[derive(Clone, Copy, Debug)]
pub struct LandingProfile {
    pub provider: NativeAppId,
    pub provider_name: &'static str,
    /// The image name the bound window **must** report, `.exe` stripped.
    pub process_name: &'static str,
    /// Which doctrine this provider's placement primitive uses. Determines
    /// which channels are disqualified.
    pub write_channel: WriteChannel,
    /// The channels this profile judges through, in order. Checked against
    /// [`judges_independently`] before any cross-process call.
    pub judges: &'static [JudgeChannel],
    /// What this provider's *empty* composer publishes. Discord's is
    /// `"\u{feff}\n"` — a zero-width no-break space and a newline — and
    /// `char::is_whitespace` is false for `U+FEFF`, so `str::trim` leaves it
    /// non-empty (D-212, secondary finding).
    pub empty_document_chars: &'static [char],
    /// How this provider's leaves compose into one document.
    pub leaf_join: &'static str,
    /// How this provider's composer is told apart from every other editable
    /// element -- including the search box A-00 wrote into by accident. Carried
    /// here because the matcher is one of the six things that differ per
    /// provider and must never be inherited from a neighbour.
    pub matcher: Uia2ComposerMatcher,
    /// Whether the tree needs Chromium's accessibility handshake first.
    pub wake: bool,
    /// How long this provider takes to publish a changed document after a
    /// write lands. Measured. Never inferred.
    pub settle_ms: u64,
    pub walk: WalkCaps,
    pub judge_timeout_ms: u64,
    /// The key that commits a message on this provider. Recorded so a probe can
    /// name what it must never have sent. `SendInput` is global and W9 is open:
    /// *"Enter sends whatever Discord's own box holds."*
    pub commit_key: &'static str,
    /// The smallest ink delta that counts as "glyphs appeared". Measured against
    /// this provider's empty composer.
    pub min_ink_delta: u32,
    pub normalisations: &'static [Normalisation],
}

/// The result of asking for a provider's profile.
#[derive(Clone, Copy, Debug)]
pub enum ProfileLookup {
    Measured(&'static LandingProfile),
    /// No profile, and a statement of **what measurement is missing**. This is
    /// the mechanical form of *never infer one provider's from another's*.
    Unmeasured {
        provider: NativeAppId,
        missing: &'static str,
    },
}

/// Discord — the only surface that carries today, and therefore the only one
/// where the oracle can be calibrated against a known-good placement.
pub static DISCORD: LandingProfile = LandingProfile {
    provider: NativeAppId::Discord,
    provider_name: "Discord",
    process_name: "Discord",
    // 23 SendInput sites; `SetValue` explicitly rejected at
    // native_discord_adapter.rs:1557-1563.
    write_channel: WriteChannel::SynthesizedInput,
    judges: &[
        JudgeChannel::RenderedDocumentUia,
        JudgeChannel::RenderedDocumentMsaa,
        JudgeChannel::ComposerInk,
    ],
    empty_document_chars: &['\u{feff}', '\n', '\r', ' ', '\t'],
    leaf_join: "",
    // Measured at D-205: the one writable editable on Discord's plan is named
    // "Message @<peer>", and the search box that A-00 wrote into by accident is
    // named "Search ..." -- which is why "search" is a rejecting stem and is
    // checked first.
    matcher: Uia2ComposerMatcher {
        composer_stems: &["message", "nachricht", "mensaje"],
        non_composer_stems: &["search", "filter", "buscar"],
    },
    wake: true,
    // D-205 measured `settled_ms=0` on a warm tree; the write still needs a
    // React render before the AX subtree is rebuilt, and this is the observed
    // convergence window, not a guess at one.
    settle_ms: 250,
    walk: WalkCaps {
        max_nodes: 256,
        max_depth: 8,
    },
    judge_timeout_ms: 5_000,
    commit_key: "Enter",
    min_ink_delta: 16,
    normalisations: &[
        Normalisation::BlockBreaksAreNotNewlines,
        Normalisation::ZeroWidthSentinelRetained,
    ],
};

/// The profile table. **Exhaustive** — a new provider cannot compile without a
/// decision, and the decision for an unmeasured provider must name the
/// measurement that is missing.
pub const fn landing_profile(provider: NativeAppId) -> ProfileLookup {
    match provider {
        NativeAppId::Discord => ProfileLookup::Measured(&DISCORD),
        NativeAppId::Telegram => ProfileLookup::Unmeasured {
            provider,
            missing: "Telegram is a Qt composer on the UiaNative route with no Chromium AX \
                      subtree; its empty-document publication, its leaf join and its ink \
                      baseline have never been read. Its substrate numbers (no wake, settles \
                      at 0 ms) are NOT Discord's and Discord's are not its.",
        },
        NativeAppId::Signal => ProfileLookup::Unmeasured {
            provider,
            missing: "Signal's adapter BANS synthesized input by design and counts it as \
                      submit-shaped, so its write channel is ValueSet and its Quill composer \
                      has never been read through the leaf channel. It settles at 1950 ms, \
                      not Discord's 250.",
        },
        NativeAppId::Whatsapp => ProfileLookup::Unmeasured {
            provider,
            missing: "WhatsApp runs under WebView2 as a sibling renderer; its composer shape \
                      is an open measurement (it may need no placement primitive at all) and \
                      its renderer differs from Discord's at the same object id.",
        },
        NativeAppId::Outlook => ProfileLookup::Unmeasured {
            provider,
            missing: "Outlook exists as a native desktop variant and a web surface; whether \
                      one composer shape serves both is a measurement nobody has taken.",
        },
    }
}

// ---------------------------------------------------------------------------
// The verdict
// ---------------------------------------------------------------------------

/// The carrier landed, and here is every channel's answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LandingProof {
    pub provider_name: &'static str,
    pub write_channel: WriteChannel,
    pub judged_by: Vec<JudgeChannel>,
    pub window: WindowIdentity,
    /// The rendered document, byte-exactly equal to the expectation.
    pub document: String,
    pub leaves: Vec<String>,
    pub nodes_visited: usize,
    /// The second judging channel's answer, when the provider has one.
    pub corroborating_document: Option<String>,
    pub ink_before: Ink,
    pub ink_after: Ink,
    pub ink_delta: u32,
    /// Read, reported, and counted by nothing. When this is not the document,
    /// the two channels have been shown to be independent *on this run*.
    pub disowned_value_property: Option<String>,
    pub disowned_value_property_disagrees: bool,
    pub submit_shaped_calls: usize,
    /// The key that would have committed, and was not sent.
    pub commit_key_not_sent: &'static str,
}

/// Why the bound window is not the window the carrier was supposed to land in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WrongWindowReason {
    /// The bound window's live image name is not the profile's. D-211's shape:
    /// `DiscordPTB` is not `Discord`.
    BoundProcessMismatch {
        expected: &'static str,
        found: String,
    },
    /// The bound window's process id changed under the binding.
    BoundProcessIdMismatch { expected: u32, found: u32 },
    /// The bound composer holds nothing, and the carrier was found whole in
    /// another window the caller named.
    CarrierLandedInAnotherWindow { found_in: WindowIdentity },
}

/// Every way the oracle says no. Each is a **name**, because a refusal that
/// collapses into a boolean cannot be watched refuse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LandingRefusal {
    /// **Structural, and checked before any cross-process call.** The profile
    /// asks to judge through a channel the write doctrine disqualifies. This is
    /// D-205 made unrepresentable.
    JudgedByTheWritingChannel {
        write: WriteChannel,
        judge: JudgeChannel,
    },
    /// No profile has been measured for this provider, and nothing will be
    /// inferred from a neighbour's.
    ProviderNotMeasured {
        provider_name: &'static str,
        missing: &'static str,
    },
    /// The oracle was asked whether the empty string landed.
    EmptyExpectation,
    /// The bound window is not the one the profile names, or the carrier was
    /// found somewhere else.
    WrongWindow(WrongWindowReason),
    /// The composer published no text leaf at all. **Never the same claim as
    /// "the composer is empty"** — a provider that has stopped answering and a
    /// provider that is empty must not share a verdict.
    NoRenderedDocument,
    /// The composer's rendered document holds only this provider's empty
    /// sentinel. `disowned_value_property` is carried so a run in which the
    /// value channel claims otherwise is visible in the refusal itself.
    NothingPlaced {
        document: String,
        disowned_value_property: Option<String>,
        disowned_value_property_claims_landed: bool,
    },
    /// The document is a proper, shorter prefix of the expectation. The
    /// characteristic `SendInput` failure: a chunk was dropped.
    Truncated {
        placed_chars: usize,
        expected_chars: usize,
        document: String,
    },
    /// The document is the expectation after the provider's **declared,
    /// measured** re-encodings, and is not the expectation. Still a refusal.
    Rewrapped {
        document: String,
        normalised: String,
        applied: Vec<Normalisation>,
    },
    /// Text is present and is not the expectation by any declared
    /// normalisation.
    ForeignText { document: String },
    /// The two rendered-document channels do not agree. One of them is wrong
    /// and the oracle does not know which, so it refuses.
    JudgingChannelsDisagree { uia: String, msaa: String },
    /// The document channel says the carrier is there and the pixels say the
    /// composer did not change. **This is the refusal D-220's third blocker
    /// names** — placement has never been proven to land *on screen*.
    NotOnScreen {
        document: String,
        ink_before: Ink,
        ink_after: Ink,
        required_delta: u32,
    },
    /// A judging walk reached its bound. A bound reached is a refusal, never a
    /// truncation.
    JudgeWalkTooLarge {
        nodes_visited: usize,
        max_nodes: usize,
        depth_reached: usize,
        max_depth: usize,
    },
    /// A judging call did not answer inside its budget.
    JudgeTimedOut(JudgeTimeout),
    /// The backend observed a submit-shaped interaction. Nothing further runs
    /// and no verdict is issued.
    SubmitShaped { calls: usize },
}

impl LandingRefusal {
    /// The refusal's own name. Used by probes so a refusal is reported by name
    /// rather than by `Debug` shape.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::JudgedByTheWritingChannel { .. } => "JudgedByTheWritingChannel",
            Self::ProviderNotMeasured { .. } => "ProviderNotMeasured",
            Self::EmptyExpectation => "EmptyExpectation",
            Self::WrongWindow(_) => "WrongWindow",
            Self::NoRenderedDocument => "NoRenderedDocument",
            Self::NothingPlaced { .. } => "NothingPlaced",
            Self::Truncated { .. } => "Truncated",
            Self::Rewrapped { .. } => "Rewrapped",
            Self::ForeignText { .. } => "ForeignText",
            Self::JudgingChannelsDisagree { .. } => "JudgingChannelsDisagree",
            Self::NotOnScreen { .. } => "NotOnScreen",
            Self::JudgeWalkTooLarge { .. } => "JudgeWalkTooLarge",
            Self::JudgeTimedOut(_) => "JudgeTimedOut",
            Self::SubmitShaped { .. } => "SubmitShaped",
        }
    }
}

impl From<JudgeTimeout> for LandingRefusal {
    fn from(timeout: JudgeTimeout) -> Self {
        Self::JudgeTimedOut(timeout)
    }
}

// ---------------------------------------------------------------------------
// The oracle
// ---------------------------------------------------------------------------

fn is_empty_document(text: &str, empty_chars: &[char]) -> bool {
    text.chars().all(|c| empty_chars.contains(&c))
}

/// Check the profile's judging channels against the independence table.
///
/// Runs **before** any cross-process call, so a profile that would judge itself
/// costs nothing and reaches no provider. Public so a build-time test can hold
/// every shipped profile to it without driving a live app.
pub fn check_independence(profile: &LandingProfile) -> Result<(), LandingRefusal> {
    let mut index = 0;
    while index < profile.judges.len() {
        let judge = profile.judges[index];
        if !judges_independently(profile.write_channel, judge) {
            return Err(LandingRefusal::JudgedByTheWritingChannel {
                write: profile.write_channel,
                judge,
            });
        }
        index += 1;
    }
    if profile.judges.is_empty() {
        return Err(LandingRefusal::JudgedByTheWritingChannel {
            write: profile.write_channel,
            judge: JudgeChannel::ComposerValueProperty,
        });
    }
    Ok(())
}

/// **The oracle.** Did `expected` land, byte for byte, in `bound`'s composer?
///
/// `baseline` is the ink of the same rectangle taken while the composer was
/// provably empty. `strays` are other composers the caller has already bound;
/// they are consulted **only** when the bound composer is empty, so the scan is
/// bounded by an explicit, caller-supplied list and never by a desktop walk.
pub fn judge_landing(
    judge: &dyn LandingJudgeSyscalls,
    profile: &LandingProfile,
    bound: &BoundComposer,
    expected: &str,
    baseline: LandingBaseline,
    strays: &[BoundComposer],
) -> Result<LandingProof, LandingRefusal> {
    // 1 — structural. No provider is touched if the profile would judge itself.
    check_independence(profile)?;

    if expected.is_empty() {
        return Err(LandingRefusal::EmptyExpectation);
    }

    let deadline = JudgeDeadline::from_profile(profile);
    let submit_baseline = judge.submit_shaped_calls();
    if submit_baseline > 0 {
        return Err(LandingRefusal::SubmitShaped {
            calls: submit_baseline,
        });
    }

    // 2 — is this even the right window? Read fresh; never trusted from the
    // binding. D-211: `DiscordPTB` resolves where a plan named `Discord`.
    let window = judge.window_identity(bound.hwnd, deadline)?;
    if !window.process_name.eq_ignore_ascii_case(profile.process_name) {
        return Err(LandingRefusal::WrongWindow(
            WrongWindowReason::BoundProcessMismatch {
                expected: profile.process_name,
                found: window.process_name,
            },
        ));
    }
    if window.process_id != bound.process_id {
        return Err(LandingRefusal::WrongWindow(
            WrongWindowReason::BoundProcessIdMismatch {
                expected: bound.process_id,
                found: window.process_id,
            },
        ));
    }

    // 3 — J1, the rendered document.
    let uia = judge
        .rendered_document_uia(bound, profile.walk, profile.leaf_join, deadline)?
        .ok_or(LandingRefusal::NoRenderedDocument)?;
    if uia.nodes_visited >= profile.walk.max_nodes || uia.depth_reached >= profile.walk.max_depth {
        return Err(LandingRefusal::JudgeWalkTooLarge {
            nodes_visited: uia.nodes_visited,
            max_nodes: profile.walk.max_nodes,
            depth_reached: uia.depth_reached,
            max_depth: profile.walk.max_depth,
        });
    }

    // 4 — J2, the same document through a structurally different provider.
    let msaa = if profile.judges.contains(&JudgeChannel::RenderedDocumentMsaa) {
        judge.rendered_document_msaa(bound, profile.walk, profile.leaf_join, deadline)?
    } else {
        None
    };
    if let Some(msaa) = &msaa {
        // Compared modulo the provider's declared re-encodings only: the two
        // COM providers publish the same layout tree but need not publish the
        // same sentinel handling.
        if normalise(&msaa.text, profile.normalisations)
            != normalise(&uia.text, profile.normalisations)
        {
            return Err(LandingRefusal::JudgingChannelsDisagree {
                uia: uia.text.clone(),
                msaa: msaa.text.clone(),
            });
        }
    }

    // 5 — D, the disowned channel. Read, reported, counted by nothing.
    let disowned = judge.disowned_value_property(bound, deadline)?;
    let disowned_claims_landed = disowned
        .as_deref()
        .is_some_and(|value| value.contains(expected));

    let document = uia.text.clone();

    // 6 — nothing at all.
    if is_empty_document(&document, profile.empty_document_chars) {
        for stray in strays {
            let stray_window = judge.window_identity(stray.hwnd, deadline)?;
            let Some(stray_document) =
                judge.rendered_document_uia(stray, profile.walk, profile.leaf_join, deadline)?
            else {
                continue;
            };
            if stray_document.text.contains(expected) {
                return Err(LandingRefusal::WrongWindow(
                    WrongWindowReason::CarrierLandedInAnotherWindow {
                        found_in: stray_window,
                    },
                ));
            }
        }
        return Err(LandingRefusal::NothingPlaced {
            document,
            disowned_value_property: disowned,
            disowned_value_property_claims_landed: disowned_claims_landed,
        });
    }

    // 7 — the only path to a proof: byte-exact. No normalisation reaches here.
    if document != expected {
        let placed_chars = document.chars().count();
        let expected_chars = expected.chars().count();
        if placed_chars < expected_chars && expected.starts_with(&document) {
            return Err(LandingRefusal::Truncated {
                placed_chars,
                expected_chars,
                document,
            });
        }
        let normalised_document = normalise(&document, profile.normalisations);
        let normalised_expected = normalise(expected, profile.normalisations);
        if normalised_document == normalised_expected {
            return Err(LandingRefusal::Rewrapped {
                document,
                normalised: normalised_document,
                applied: profile.normalisations.to_vec(),
            });
        }
        if normalised_document.chars().count() < normalised_expected.chars().count()
            && normalised_expected.starts_with(&normalised_document)
        {
            return Err(LandingRefusal::Truncated {
                placed_chars: normalised_document.chars().count(),
                expected_chars: normalised_expected.chars().count(),
                document,
            });
        }
        return Err(LandingRefusal::ForeignText { document });
    }

    // 8 — J3, the pixels. The document says it is there; is it on screen?
    let ink_after = judge
        .composer_ink(bound, deadline)?
        .unwrap_or_else(Ink::default);
    let ink_delta = ink_after.inked.saturating_sub(baseline.empty_ink.inked);
    if profile.judges.contains(&JudgeChannel::ComposerInk) && ink_delta < profile.min_ink_delta {
        return Err(LandingRefusal::NotOnScreen {
            document,
            ink_before: baseline.empty_ink,
            ink_after,
            required_delta: profile.min_ink_delta,
        });
    }

    // 9 — nothing submit-shaped happened at any point.
    let submit_shaped_calls = judge.submit_shaped_calls();
    if submit_shaped_calls > submit_baseline {
        return Err(LandingRefusal::SubmitShaped {
            calls: submit_shaped_calls,
        });
    }

    Ok(LandingProof {
        provider_name: profile.provider_name,
        write_channel: profile.write_channel,
        judged_by: profile.judges.to_vec(),
        window,
        document,
        leaves: uia.leaves,
        nodes_visited: uia.nodes_visited,
        corroborating_document: msaa.map(|document| document.text),
        ink_before: baseline.empty_ink,
        ink_after,
        ink_delta,
        disowned_value_property: disowned.clone(),
        disowned_value_property_disagrees: disowned.as_deref() != Some(expected),
        submit_shaped_calls,
        commit_key_not_sent: profile.commit_key,
    })
}

// ---------------------------------------------------------------------------
// The live Windows judges
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub(crate) mod win32;

#[cfg(all(test, target_os = "windows"))]
mod live;

#[cfg(test)]
mod tests;
