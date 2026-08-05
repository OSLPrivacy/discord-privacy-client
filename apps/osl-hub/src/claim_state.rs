//! **THE CLAIM STATE.** What OSL may say about a surface, and *why* it stands
//! there.
//!
//! # The gate this closes
//!
//! `PLAN.md` r4-5 lists seven owner gates. This is one of them:
//!
//! > **The claim-state gap** — blocks Discord, Telegram, OSL Mail, groups —
//! > *no available label is true*; the enum collapses states the allowlist
//! > forbids. Tail: none — but it blocks four ungates.
//!
//! Before this module the whole public claim was one three-valued enum,
//! `NativeAppSupportStatus { Beta, ComingSoon, ExternallyBlocked }`, fed by a
//! hand-written map that sent **two** `SupportLevel`s to the same label. Two
//! surfaces were stuck in it from opposite directions, and both said so at
//! source:
//!
//! * **Discord** (`native_apps.rs`, D-203) — `Beta` overclaims (the allowlist
//!   reserves it for `runtime-proven` / `test-proven-only` rows and Discord is
//!   neither), `ComingSoon` claims Discord is *planned* when it is the one
//!   carrier the app enables today, and `ExternallyBlocked` asserts a third
//!   party blocks us when D-205 resolved its live composer at 558 elements.
//!   *"So the enum cannot express Discord's actual state."*
//! * **Telegram** (D-206) — placement driven live and cover text carried
//!   byte-exact, but the only status Rust could render for a carrying provider
//!   was `Beta`, which `osl-public-claim-allowlist.md:191` forbids for Telegram.
//!   *"Held until a status that renders as `Experimental` exists."*
//!
//! and two more that never had a label at all:
//!
//! * **WhatsApp** (D-234) — *measured against the live client and REFUSED*. A
//!   writable `ValuePattern` whose value-set never reaches the composer
//!   document. That is not "not built yet"; it is a falsified technique, and
//!   `ComingSoon` says the opposite of what was learned.
//! * **OSL Mail** (D-221) — first-party, so per `PLAN.md` r5-2a it has **no
//!   composer to bind** and "carrier proven" is not a meaningful question about
//!   it. `available: false` "matches every authority but hides provision/send/
//!   burn, which *do* work against the deployed keyserver". `available: true`
//!   promised a mailbox that can never be read. *"The tile has no third state."*
//!
//! # The shape of the fix: one enum was carrying two facts
//!
//! A public label answers *"what may we say"*. It was also being asked
//! *"what do we know"*, and those have different arities. This module separates
//! them and makes the first a **derivation** of the second:
//!
//! ```text
//!   CarrierEvidence  ──┐
//!   DeliveryEvidence ──┼──►  public_claim()  ──►  PublicClaim  (the badge)
//!   ClaimBlocker[]   ──┤                     ──►  the reason line (the prose)
//!   MatrixPosition   ──┘
//! ```
//!
//! A label cannot be *written*. It is computed, so "collapse two states" stops
//! being an edit somebody can make and becomes a property the gates check.
//!
//! # What is true today, and this module must not hide it
//!
//! **`carry-receipts/` does not exist as a directory** (`PLAN.md` r5-6: *"Not
//! empty — absent"*). **Zero** carrier surfaces have earned a live carry
//! receipt. So [`carrier_receipt_census`] is computed from the receipt verifier
//! on every run rather than recorded, no carrier surface can reach a capability
//! claim, and the count is rendered to the user rather than kept in a test.
//!
//! # Ordering, and why `Experimental` is not a promotion
//!
//! [`PublicClaim`] is ordered by **how much capability it asserts**:
//!
//! ```text
//!   NoClaim  <  Planned ≈ Experimental  <  Beta  <  Available
//!               └──────────┬──────────┘
//!         both are limitation labels, not capability claims:
//!         master §8.2 badge vocabulary, allowlist §B row 5 permits
//!         "Coming soon", "Experimental" or "Externally blocked" for
//!         Signal/WhatsApp/Telegram/Outlook, and `experimental` is
//!         absent from check-app-claims.mjs FORBIDDEN_PROMOTION_STATUSES
//!         while appearing in its limitation-language set.
//! ```
//!
//! They differ in **what they say about the implementation**, which is exactly
//! the distinction D-206 needed and could not express: `Planned` says nothing is
//! wired, `Experimental` says something is wired and has never been proven.
//! Neither claims the surface works.
//!
//! `ExternallyBlocked` is deliberately **not on that axis**. It is a different
//! assertion — a claim about a *third party*, not about us — so it is never
//! reached by "weakening" and never compared by strength.
//!
//! # Conflicting authorities produce NO CLAIM, not a coin-flip
//!
//! `osl-public-claim-allowlist.md` rule 5: *"If evidence is stale, **conflicting**,
//! or does not name an exact build, the answer is `unknown-recheck-required` —
//! which earns no public claim at all. Not a softened claim. No claim."*
//!
//! So where this module's derivation and `docs/status/support-matrix.json`
//! disagree **in kind** — one on the capability axis, the other
//! `ExternallyBlocked` — the result is [`PublicClaim::NoClaim`]. Where they
//! disagree only in **strength**, the weaker wins. Neither case silently picks a
//! side, and neither needs the allowlist widened.

/// Every surface in the owner's 2026-08-05 ruling (`PLAN.md` r5-2a), and
/// nothing else. Reconciled against `data/surface-ruling-2026-08-05.json` by
/// [`tests::the_table_covers_exactly_the_ruled_surface_list`].
///
/// Outlook is **two** surfaces. r5-2a: *"Outlook appears twice (desktop native +
/// web). Treat as two surfaces unless measurement shows one composer shape
/// serves both — that is a measurement, not a decision."* No such measurement
/// exists, so they are two rows.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum Surface {
    // Chat carriers — 4.
    Discord,
    Signal,
    Whatsapp,
    Telegram,
    // Native email carrier — Outlook desktop, which has a `NativeAppId` variant.
    OutlookDesktop,
    // Email carriers on the web — the eight with legal analysis, plus Tuta.
    Gmail,
    OutlookWeb,
    Proton,
    Yahoo,
    Aol,
    Gmx,
    MailDotCom,
    ICloud,
    Tuta,
    // First-party — 2. Not carriers; no composer to bind.
    OslChats,
    OslMail,
}

/// What has actually been **measured** about a surface's carrier path.
///
/// Every variant here is a distinction this project has already paid for in a
/// defect. Collapsing any two of them is the failure this module exists to make
/// impossible, and
/// [`tests::the_derivation_never_collapses_two_states_the_allowlist_separates`]
/// checks it by executing the derivation over the whole input space.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CarrierEvidence {
    /// No adapter and no carrier path is wired. Nothing has been attempted.
    NotBuilt,
    /// An adapter exists and has **never been driven against a live provider**.
    /// There is no carry receipt. This is where every carrier surface with code
    /// behind it stands today.
    BuiltNeverProvenLive,
    /// The technique was **tried against the real client and did not land**.
    /// D-234, WhatsApp: a writable `ValuePattern` whose `SetValue` leaves the
    /// composer document at `"\n"` on both channels, reproduced across three
    /// independent runs. A falsified path is not an unattempted one.
    MeasuredAndRefused,
    /// A third party's surface is unavailable to us, independently of anything
    /// we build.
    ExternallyBlocked,
    /// **First-party.** There is no third-party composer to bind, so
    /// "carrier proven" is not a meaningful question about this surface
    /// (`PLAN.md` r5-2a). Structural, not an exemption by name:
    /// [`tests::only_ruled_first_party_surfaces_escape_the_receipt_requirement`]
    /// binds it to the ruling's `first_party_surfaces`.
    NoCarrierByConstruction,
    /// A sound, on-seam live carry receipt exists. **No surface holds this
    /// today**, and the gates recompute it from the receipt verifier rather
    /// than trusting the table.
    ProvenLiveWithReceipt,
}

/// Whether a message can actually get from one person to another on this
/// surface. Separate from [`CarrierEvidence`] because for a first-party surface
/// it is the *whole* story, and for a carrier surface it is a second question
/// the carrier gates one way but does not answer.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeliveryEvidence {
    /// The path is absent or incomplete; nothing can arrive. OSL Mail:
    /// `pointer_envelope` puts only hashes on the wire and uploads the payload
    /// nowhere, and D-137 deleted the retrieval half (D-221).
    NotDeliverable,
    /// Implemented, never proven end to end against the deployed service.
    NeverProvenLive,
    /// Proven end to end, **both directions**, against the deployed service.
    /// OSL Chat only: D-224 (`02-direct-message` 5/5 against
    /// `keyserver.oslprivacy.com`), D-208 offline receive, D-007 restore.
    ProvenLiveBothWays,
}

/// A governance or authority condition standing on a row, independent of how
/// well the engineering works.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ClaimBlocker {
    /// Allowlist §E: *"`open-security-finding` outranks everything else on the
    /// same row. A feature can be fully built, runtime-proven, and still earn no
    /// claim because a finding blocks it."*
    OpenSecurityFinding,
    /// Allowlist rule 5: evidence is stale, conflicting, or does not name an
    /// exact build. Earns **no** public claim — not a softened one.
    UnknownRecheckRequired,
    /// `PLAN.md` r4-5: the `SendInput` generalisation gate. Accepting a global
    /// input technique with an open security finding, and repealing Signal's
    /// deliberate prohibition on synthesised input. D-234 put WhatsApp behind
    /// this gate too.
    SendInputGeneralisation,
    /// `PLAN.md` r4-5: the web-surface legal review. The ToS document states its
    /// clauses *do not establish* that overlay/accessibility automation is
    /// compliant.
    WebSurfaceLegalReview,
}

impl ClaimBlocker {
    /// Whether this blocker, on its own, extinguishes the public claim.
    ///
    /// The two that do are the two the allowlist says earn **no badge and no
    /// claim** (§E, rule 5). The other two block *work*, not *speech*: they
    /// explain why a surface will stay where it is, and a surface behind them
    /// may still honestly say "nothing is proven here".
    pub const fn extinguishes_claim(self) -> bool {
        matches!(
            self,
            Self::OpenSecurityFinding | Self::UnknownRecheckRequired
        )
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::OpenSecurityFinding => "open-security-finding",
            Self::UnknownRecheckRequired => "unknown-recheck-required",
            Self::SendInputGeneralisation => "send-input-generalisation",
            Self::WebSurfaceLegalReview => "web-surface-legal-review",
        }
    }
}

/// Where `docs/status/support-matrix.json` puts this surface. Master §8.4:
/// *"Per-service claims must come from the versioned support matrix."*
///
/// Recorded per row so a disagreement between this module and the matrix is
/// **resolved by rule** instead of by whichever file someone read last.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MatrixPosition {
    /// `public_status: "coming_soon"` or `"unsupported"` — `Planned`-shaped.
    NoCapabilityClaim,
    /// `public_status: "externally_blocked"`.
    ExternallyBlocked,
    /// The matrix carries no row for this surface, so it constrains nothing.
    NoRow,
}

/// The public claim itself: the badge, and nothing more.
///
/// Ordered by how much capability it asserts, so "take the weaker" is a real
/// operation rather than a comment. `ExternallyBlocked` sorts below `Planned`
/// only so the type can be `Ord`; it is never compared by
/// [`weaker_of`], which refuses to order it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum PublicClaim {
    /// Allowlist §E: `open-security-finding` / `unknown-recheck-required` →
    /// **no badge, no claim**.
    NoClaim,
    /// A claim about a third party, not about us. Not on the capability axis.
    ExternallyBlocked,
    /// Master §8.2 `Planned`, rendered "Coming soon". Nothing is wired.
    Planned,
    /// Master §8.2 `Experimental`. Something is wired and has never been proven
    /// against a live provider. **Not** a capability claim.
    Experimental,
    /// Allowlist §E: `runtime-proven` / `test-proven-only`. A capability claim,
    /// and it must ship with its scope limit.
    Beta,
    /// Allowlist §E: `verified-live` only. Nothing in this product reaches it,
    /// and [`tests::nothing_reaches_available`] holds that open rather than
    /// deleting the variant.
    Available,
}

impl PublicClaim {
    /// Whether this label asserts the surface **works**. The receipt gate binds
    /// here, and only here.
    pub const fn is_capability_claim(self) -> bool {
        matches!(self, Self::Beta | Self::Available)
    }

    /// Whether this label may appear on the capability strength axis at all.
    pub const fn on_capability_axis(self) -> bool {
        !matches!(self, Self::ExternallyBlocked)
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::NoClaim => "noClaim",
            Self::ExternallyBlocked => "externallyBlocked",
            Self::Planned => "comingSoon",
            Self::Experimental => "experimental",
            Self::Beta => "beta",
            Self::Available => "available",
        }
    }
}

/// The weaker of two claims, or `NoClaim` when they are not comparable.
///
/// Two labels on the capability axis are ordered, so the weaker is the honest
/// answer. One on the axis and one `ExternallyBlocked` are **different
/// assertions**, and choosing either would state something no evidence
/// supports — allowlist rule 5's "conflicting" case, which earns no claim.
pub const fn weaker_of(left: PublicClaim, right: PublicClaim) -> PublicClaim {
    if left.on_capability_axis() != right.on_capability_axis() {
        return PublicClaim::NoClaim;
    }
    if (left as u8) <= (right as u8) {
        left
    } else {
        right
    }
}

/// One surface's claim state: everything that decides what may be said, and
/// the sentence said when it is.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceClaim {
    pub surface: Surface,
    pub carrier: CarrierEvidence,
    pub delivery: DeliveryEvidence,
    pub blockers: &'static [ClaimBlocker],
    pub matrix: MatrixPosition,
    /// The defect, ruling or report this row is bound to. Printed in every gate
    /// failure so a wrong row is traceable to the evidence that set it.
    pub authority: &'static str,
    /// **The sentence the user is shown.** One line, no promise, and it must
    /// distinguish this row's evidence from every other row's — that is what
    /// keeps two surfaces sharing a badge from collapsing into one state.
    pub reason: &'static str,
}

/// The claim state of every ruled surface.
///
/// Nothing here is a decision about what *should* be claimed. Each row records
/// what was measured and by whose authority; the label is
/// [`public_claim`]'s output.
pub const SURFACE_CLAIMS: &[SurfaceClaim] = &[
    // ---- Chat carriers -----------------------------------------------------
    SurfaceClaim {
        surface: Surface::Discord,
        // The adapter is real and drives a live composer (D-205 resolved it at
        // 558 elements). It has never earned a carry receipt: `carry-receipts/`
        // does not exist, and Discord is the single entry in
        // `PUBLISHED_WITHOUT_A_LIVE_RECEIPT`.
        carrier: CarrierEvidence::BuiltNeverProvenLive,
        delivery: DeliveryEvidence::NeverProvenLive,
        // Both allowlist extinguishers stand on this row at once, which is
        // precisely why no label fitted: `support-matrix.json` carries D6 as
        // `open-security-finding`, and master §9 records the send as
        // "verified-live on dated QA builds; current tree recheck required",
        // which allowlist rule 5 makes `unknown-recheck-required`.
        blockers: &[
            ClaimBlocker::OpenSecurityFinding,
            ClaimBlocker::UnknownRecheckRequired,
        ],
        matrix: MatrixPosition::NoRow,
        authority: "D-203, D-224; allowlist §E open-security-finding outranks the row",
        reason: "OSL has never carried a message through Discord and back in a recorded two-party run, and an open security finding stands on this surface. OSL makes no claim about it.",
    },
    SurfaceClaim {
        surface: Surface::Signal,
        // A signed data-only adapter profile exists (`crates/adapter-profile`),
        // and the adapter's own trait BANS synthesised input by design, so the
        // one doctrine any surface has been shown to land by is prohibited here
        // until the owner rules.
        carrier: CarrierEvidence::BuiltNeverProvenLive,
        delivery: DeliveryEvidence::NeverProvenLive,
        blockers: &[ClaimBlocker::SendInputGeneralisation],
        matrix: MatrixPosition::NoCapabilityClaim,
        authority: "support-matrix signal_desktop_public (coming_soon); PLAN.md r4-2 blocker 2",
        reason: "A Signal adapter profile exists and has never been driven against the live client. Signal's adapter also refuses synthesised input by design, which is the only technique any surface has been shown to land by, so nothing is proven here.",
    },
    SurfaceClaim {
        surface: Surface::Whatsapp,
        // D-234. The distinction this variant exists for: not "unbuilt", but
        // "tried, and it does not work".
        carrier: CarrierEvidence::MeasuredAndRefused,
        delivery: DeliveryEvidence::NeverProvenLive,
        blockers: &[ClaimBlocker::SendInputGeneralisation],
        matrix: MatrixPosition::NoCapabilityClaim,
        authority: "D-234 (falsifies D-227 CORRECTED); support-matrix whatsapp_windows_public",
        reason: "OSL measured WhatsApp's composer against the live client and the write did not land: the value reaches the accessibility layer and the message document stays empty. This is a refused technique, not unfinished work, so nothing is proven here.",
    },
    SurfaceClaim {
        surface: Surface::Telegram,
        // D-206: placement driven live, cover text carried byte-exact and
        // decoded back out, signed-client row probe `supported`.
        //
        // 2026-08-05: Telegram HAS NOW EARNED A LIVE CARRY RECEIPT — the first
        // this project has ever held. `carry-receipts/telegram.json`, schema v2,
        // cover landed byte-exact at 264 bytes judged by the landing oracle
        // (TextPattern + GDI ink, never the accessibility value), payload
        // recovered by `decode_mode1` from the ORACLE'S OWN document, ledger 9
        // rates it `Ok`, and 13 mutations of the receipt were each rejected.
        //
        // This row previously read `BuiltNeverProvenLive`, which was true when
        // this module was written and false by the time it merged: the only
        // `telegram.json` then present was a schema **v1** file that
        // `verify_receipt_bytes` rates `Stale` BY NAME. The census test caught
        // the disagreement on the merge — the table said 0 receipts while the
        // verifier confirmed 1 — which is exactly what it exists to do.
        carrier: CarrierEvidence::ProvenLiveWithReceipt,
        delivery: DeliveryEvidence::NeverProvenLive,
        blockers: &[],
        // And the matrix says `externally_blocked`, which is a claim about
        // Telegram rather than about us, and is a STRONGER negative than the
        // measurement supports. Incomparable authorities -> no claim.
        matrix: MatrixPosition::ExternallyBlocked,
        authority: "D-206; support-matrix telegram_desktop_public (externally_blocked)",
        reason: "OSL has carried cover text through Telegram's composer and earned a live carry receipt for it -- the only surface that has. But OSL's own support matrix still records Telegram as externally blocked, which is a claim about Telegram rather than about us. Those disagree, so OSL makes no claim about it.",
    },
    // ---- Native email carrier ---------------------------------------------
    SurfaceClaim {
        surface: Surface::OutlookDesktop,
        // `carry_seam(Outlook) == CarrySeam::NoCarryPath`, and
        // `adapter_source(Outlook) == None`: no adapter module exists, so no
        // receipt can even be bound.
        carrier: CarrierEvidence::NotBuilt,
        delivery: DeliveryEvidence::NotDeliverable,
        blockers: &[],
        matrix: MatrixPosition::NoCapabilityClaim,
        authority: "native_apps.rs carry_seam -> NoCarryPath; support-matrix osl_mail_public",
        reason: "No Outlook desktop carrier is wired. There is no adapter to prove and nothing is sent through Outlook today.",
    },
    // ---- Email carriers on the web ----------------------------------------
    // Nine rows, one shape: a browser companion opens the provider and no
    // carrier is bound to any of them. They are kept as separate rows rather
    // than a group because r5-3b A is explicit -- "never infer one provider's
    // from another's" -- and a group would be the same collapse in a new place.
    email_web(Surface::Gmail, "Gmail"),
    email_web(Surface::OutlookWeb, "Outlook on the web"),
    email_web(Surface::Proton, "Proton Mail"),
    email_web(Surface::Yahoo, "Yahoo Mail"),
    email_web(Surface::Aol, "AOL Mail"),
    email_web(Surface::Gmx, "GMX Mail"),
    email_web(Surface::MailDotCom, "Mail.com"),
    email_web(Surface::ICloud, "iCloud Mail"),
    email_web(Surface::Tuta, "Tuta"),
    // ---- First-party — not carriers ---------------------------------------
    SurfaceClaim {
        surface: Surface::OslChats,
        // r5-2a: no composer to bind. The two-party send IS proven here, and
        // D-224 is emphatic that this proves the cryptographic core and NO
        // carrier -- which is exactly what `NoCarrierByConstruction` says.
        carrier: CarrierEvidence::NoCarrierByConstruction,
        delivery: DeliveryEvidence::ProvenLiveBothWays,
        blockers: &[],
        matrix: MatrixPosition::NoRow,
        authority: "D-224, D-208, D-007; PLAN.md r5-2a first-party ruling",
        reason: "OSL Chat carries messages both ways through the deployed OSL service, proven on QA builds and not on a release build. It is not a connected-service carrier and nothing is placed in another app. A message composed while offline is dropped, not queued.",
    },
    SurfaceClaim {
        surface: Surface::OslMail,
        // D-221, and the third state the boolean could not hold: provisioning,
        // send and burn DO work against the deployed keyserver, and nothing can
        // ever be read, because `pointer_envelope` uploads no payload and D-137
        // deleted retrieval.
        carrier: CarrierEvidence::NoCarrierByConstruction,
        delivery: DeliveryEvidence::NotDeliverable,
        blockers: &[],
        matrix: MatrixPosition::NoCapabilityClaim,
        authority: "D-221; support-matrix osl_mail_public (unsupported); PLAN.md r5-2a",
        reason: "OSL Mail can provision an address, accept a send and burn a mailbox against the deployed service, and it cannot deliver: no message body is ever uploaded and there is no retrieval path, so nothing sent through it can be read.",
    },
];

/// The nine web email surfaces. A `const fn` so the table stays one literal and
/// a tenth provider cannot be added with a quietly different shape.
const fn email_web(surface: Surface, _display: &'static str) -> SurfaceClaim {
    SurfaceClaim {
        surface,
        carrier: CarrierEvidence::NotBuilt,
        delivery: DeliveryEvidence::NotDeliverable,
        blockers: &[ClaimBlocker::WebSurfaceLegalReview],
        matrix: MatrixPosition::NoRow,
        authority: "PLAN.md r4-5 web-surface legal review; no adapter module exists",
        reason: "No carrier is wired for this mail provider. OSL can open it in a browser and it does not protect anything sent through it, and the legal review that would allow one has not concluded.",
    }
}

/// The row for a surface. Total by construction — [`tests::the_table_is_exhaustive_and_unique`]
/// proves every [`Surface`] has exactly one.
pub fn claim_of(surface: Surface) -> &'static SurfaceClaim {
    SURFACE_CLAIMS
        .iter()
        .find(|row| row.surface == surface)
        .expect("every ruled surface has exactly one claim row")
}

/// **The derivation.** What the evidence alone would permit, before any
/// authority outside this module is consulted.
pub const fn derived_claim(carrier: CarrierEvidence, delivery: DeliveryEvidence) -> PublicClaim {
    match (carrier, delivery) {
        // A third party blocks us. Says nothing about our implementation, and
        // is never reached from any other evidence.
        (CarrierEvidence::ExternallyBlocked, _) => PublicClaim::ExternallyBlocked,

        // First-party: r5-2a says the carrier question does not apply, so the
        // delivery evidence is the whole answer.
        (CarrierEvidence::NoCarrierByConstruction, DeliveryEvidence::ProvenLiveBothWays) => {
            PublicClaim::Beta
        }
        (CarrierEvidence::NoCarrierByConstruction, _) => PublicClaim::Planned,

        // A receipt is necessary for a capability claim and not sufficient:
        // carrying cover text is not the same as a person receiving a message.
        (CarrierEvidence::ProvenLiveWithReceipt, DeliveryEvidence::ProvenLiveBothWays) => {
            PublicClaim::Beta
        }
        (CarrierEvidence::ProvenLiveWithReceipt, _) => PublicClaim::Experimental,

        // Wired, never proven. The state D-206 needed and could not express.
        (CarrierEvidence::BuiltNeverProvenLive, _) => PublicClaim::Experimental,

        // Tried and refused, and nothing built at all. Same badge, and they are
        // NOT the same state: the reason line is what the allowlist requires be
        // kept apart, and the gate checks it.
        (CarrierEvidence::MeasuredAndRefused, _) => PublicClaim::Planned,
        (CarrierEvidence::NotBuilt, _) => PublicClaim::Planned,
    }
}

/// What the support matrix permits for this row.
const fn matrix_ceiling(matrix: MatrixPosition) -> Option<PublicClaim> {
    match matrix {
        MatrixPosition::NoCapabilityClaim => Some(PublicClaim::Planned),
        MatrixPosition::ExternallyBlocked => Some(PublicClaim::ExternallyBlocked),
        MatrixPosition::NoRow => None,
    }
}

/// **The public claim for a surface.** The only function permitted to decide a
/// badge.
///
/// Order matters and is the allowlist's, not a preference:
/// 1. an extinguishing blocker outranks everything on the row (§E);
/// 2. otherwise the derivation is clamped to the support matrix — weaker wins,
///    and incomparable authorities earn no claim (rule 5).
pub fn public_claim(surface: Surface) -> PublicClaim {
    claim_for(claim_of(surface))
}

/// [`public_claim`] over a row, so a gate can drive it with a row the tree does
/// not contain. Every gate here uses this: a checker that can only be run
/// against the real table cannot be shown to fail.
pub fn claim_for(row: &SurfaceClaim) -> PublicClaim {
    if row
        .blockers
        .iter()
        .any(|blocker| blocker.extinguishes_claim())
    {
        return PublicClaim::NoClaim;
    }
    let derived = derived_claim(row.carrier, row.delivery);
    match matrix_ceiling(row.matrix) {
        Some(ceiling) => weaker_of(derived, ceiling),
        None => derived,
    }
}

/// How many ruled surfaces are carriers at all — the denominator the receipt
/// census is out of.
pub fn carrier_surface_count() -> usize {
    SURFACE_CLAIMS
        .iter()
        .filter(|row| row.carrier != CarrierEvidence::NoCarrierByConstruction)
        .count()
}

/// **The receipt census, computed rather than recorded.**
///
/// `PLAN.md` r5-6: *"`carry-receipts/` does not exist as a directory. Not empty
/// — absent."* This returns `(earned, of)` so the app can state that fact
/// instead of leaving it in a plan document. `earned` counts only rows whose
/// table entry claims a receipt; the gates additionally require the receipt
/// verifier to agree, so a table that lies about one fails rather than inflating
/// this number silently.
pub fn carrier_receipt_census() -> (usize, usize) {
    let earned = SURFACE_CLAIMS
        .iter()
        .filter(|row| row.carrier == CarrierEvidence::ProvenLiveWithReceipt)
        .count();
    (earned, carrier_surface_count())
}

/// The sentence the app shows about carrier proof overall. Rendered, not
/// asserted — if a receipt is ever earned this changes on its own.
pub fn carrier_receipt_census_line() -> String {
    let (earned, of) = carrier_receipt_census();
    if earned == 0 {
        format!(
            "No connected app has earned a live carry receipt yet — 0 of {of}. \
             OSL has not proven that it can place a protected message in any other app's composer."
        )
    } else {
        format!(
            "{earned} of {of} connected apps have earned a live carry receipt. \
             The rest are unproven."
        )
    }
}

impl Surface {
    /// The slug used by `data/surface-ruling-2026-08-05.json`, the support
    /// matrix and the frontend. `OutlookDesktop` and `OutlookWeb` share the
    /// ruling's single `outlook` slug, which is r5-2a's recorded residual.
    pub const fn ruling_slug(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Signal => "signal",
            Self::Whatsapp => "whatsapp",
            Self::Telegram => "telegram",
            Self::OutlookDesktop | Self::OutlookWeb => "outlook",
            Self::Gmail => "gmail",
            Self::Proton => "proton",
            Self::Yahoo => "yahoo",
            Self::Aol => "aol",
            Self::Gmx => "gmx",
            Self::MailDotCom => "maildotcom",
            Self::ICloud => "icloud",
            Self::Tuta => "tuta",
            Self::OslChats => "osl-chats",
            Self::OslMail => "osl-mail",
        }
    }
}

impl CarrierEvidence {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NotBuilt => "notBuilt",
            Self::BuiltNeverProvenLive => "builtNeverProvenLive",
            Self::MeasuredAndRefused => "measuredAndRefused",
            Self::ExternallyBlocked => "externallyBlocked",
            Self::NoCarrierByConstruction => "noCarrierByConstruction",
            Self::ProvenLiveWithReceipt => "provenLiveWithReceipt",
        }
    }
}

impl DeliveryEvidence {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::NotDeliverable => "notDeliverable",
            Self::NeverProvenLive => "neverProvenLive",
            Self::ProvenLiveBothWays => "provenLiveBothWays",
        }
    }
}

// ---------------------------------------------------------------------------
// THE VIOLATION CHECKER
//
// Every gate below runs this over a table it is handed, so each one can be
// starved of its input and shown to go red. A checker that can only be called
// with the real tree is a checker nobody has seen fail.
// ---------------------------------------------------------------------------

/// A row saying something the evidence does not support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimViolation {
    pub surface: &'static str,
    pub kind: ClaimViolationKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimViolationKind {
    /// **The fatal class.** A carrier surface asserting it works, with no sound
    /// live carry receipt behind it.
    CapabilityClaimWithoutALiveReceipt,
    /// A row claims a receipt the receipt verifier cannot confirm.
    ReceiptClaimedButNotEarned,
    /// A first-party exemption on a surface the owner's ruling does not call
    /// first-party.
    FirstPartyExemptionOffTheRuling,
    /// Two rows the allowlist requires be kept apart are showing the user the
    /// same thing.
    CollapsedWithAnotherState,
}

impl ClaimViolationKind {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::CapabilityClaimWithoutALiveReceipt => "capability-claim-without-a-live-receipt",
            Self::ReceiptClaimedButNotEarned => "receipt-claimed-but-not-earned",
            Self::FirstPartyExemptionOffTheRuling => "first-party-exemption-off-the-ruling",
            Self::CollapsedWithAnotherState => "collapsed-with-another-state",
        }
    }
}

/// Whether a surface's row may be exempt from the carry-receipt requirement.
///
/// **Structural, never by name.** The exemption is granted by the owner's
/// ruling (`PLAN.md` r5-2a: first-party surfaces have no composer to bind), and
/// the set is the ruling's, checked against the ruling file by
/// [`tests::only_ruled_first_party_surfaces_escape_the_receipt_requirement`].
/// `PLAN.md` r4-7 forbids the receipt gate gaining "a second by-name exemption";
/// this is a class, and it is closed.
pub const fn is_first_party(surface: Surface) -> bool {
    matches!(surface, Surface::OslChats | Surface::OslMail)
}

/// **The gate, as a function.**
///
/// `receipt_earned` answers, for a surface, whether a sound live carry receipt
/// exists — supplied by the caller so the real gate can pass the receipt
/// verifier and a mutation test can pass a starved one.
pub fn violations(
    table: &[SurfaceClaim],
    receipt_earned: &dyn Fn(Surface) -> bool,
) -> Vec<ClaimViolation> {
    let mut found = Vec::new();

    for row in table {
        let claim = claim_for(row);
        let first_party = row.carrier == CarrierEvidence::NoCarrierByConstruction;

        // A carrier surface may not say it works without a receipt. This is the
        // whole gate; everything else below protects it.
        if claim.is_capability_claim() && !first_party && !receipt_earned(row.surface) {
            found.push(ClaimViolation {
                surface: row.surface.ruling_slug(),
                kind: ClaimViolationKind::CapabilityClaimWithoutALiveReceipt,
                detail: format!(
                    "{:?} is claimed as {:?} with no sound live carry receipt. A connected app may \
                     not be shown to a user as working until its adapter has been driven against \
                     the real client and the receipt verified. Authority: {}",
                    row.surface, claim, row.authority
                ),
            });
        }

        // A row may not claim a receipt the verifier will not confirm.
        if row.carrier == CarrierEvidence::ProvenLiveWithReceipt && !receipt_earned(row.surface) {
            found.push(ClaimViolation {
                surface: row.surface.ruling_slug(),
                kind: ClaimViolationKind::ReceiptClaimedButNotEarned,
                detail: format!(
                    "{:?} records ProvenLiveWithReceipt and the receipt verifier does not agree. \
                     Authority: {}",
                    row.surface, row.authority
                ),
            });
        }

        // The first-party exemption is the ruling's, not a name in this file.
        if first_party && !is_first_party(row.surface) {
            found.push(ClaimViolation {
                surface: row.surface.ruling_slug(),
                kind: ClaimViolationKind::FirstPartyExemptionOffTheRuling,
                detail: format!(
                    "{:?} claims NoCarrierByConstruction but is not a first-party surface in the \
                     owner's 2026-08-05 ruling. The receipt gate does not take exemptions by name.",
                    row.surface
                ),
            });
        }
    }

    // Two rows in different evidence states must not present as one state. The
    // badge alone cannot carry the distinction -- `Planned` legitimately covers
    // both "never built" and "measured and refused" -- so the pair
    // (badge, reason) is what must differ.
    for (index, row) in table.iter().enumerate() {
        for other in table.iter().skip(index + 1) {
            let same_state = row.carrier == other.carrier && row.delivery == other.delivery;
            if same_state {
                continue;
            }
            if claim_for(row) == claim_for(other) && row.reason == other.reason {
                found.push(ClaimViolation {
                    surface: row.surface.ruling_slug(),
                    kind: ClaimViolationKind::CollapsedWithAnotherState,
                    detail: format!(
                        "{:?} ({:?}/{:?}) and {:?} ({:?}/{:?}) are in different evidence states \
                         and present identically to the user. That is the collapse this gate \
                         exists to refuse.",
                        row.surface,
                        row.carrier,
                        row.delivery,
                        other.surface,
                        other.carrier,
                        other.delivery,
                    ),
                });
            }
        }
    }

    found
}

/// The claim state rendered for a reader. Printed by every gate failure, and by
/// the fleet report, so the real position is never one pass/fail bit.
pub fn render() -> String {
    let mut out = String::from(
        "\nTHE CLAIM STATE -- what OSL may say about each ruled surface, and why\n\
         ---------------------------------------------------------------------\n  \
         surface      claim           carrier                   delivery            blockers\n",
    );
    for row in SURFACE_CLAIMS {
        let blockers = if row.blockers.is_empty() {
            "--".to_owned()
        } else {
            row.blockers
                .iter()
                .map(|blocker| blocker.slug())
                .collect::<Vec<_>>()
                .join(",")
        };
        out.push_str(&format!(
            "  {:<12} {:<15} {:<25} {:<19} {}\n",
            row.surface.ruling_slug(),
            claim_for(row).slug(),
            row.carrier.slug(),
            row.delivery.slug(),
            blockers,
        ));
    }
    out.push_str(&format!("\n{}\n", carrier_receipt_census_line()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_apps::tests::carry_receipt::{receipt_path, verify_receipt};
    use crate::native_apps::NativeAppId;
    use std::collections::BTreeSet;

    /// The `NativeAppId` a carrier surface's receipt would be bound to. `None`
    /// means no adapter module exists, so no receipt can be bound and none may
    /// ever be produced.
    const fn native_app_of(surface: Surface) -> Option<NativeAppId> {
        match surface {
            Surface::Discord => Some(NativeAppId::Discord),
            Surface::Signal => Some(NativeAppId::Signal),
            Surface::Whatsapp => Some(NativeAppId::Whatsapp),
            Surface::Telegram => Some(NativeAppId::Telegram),
            Surface::OutlookDesktop => Some(NativeAppId::Outlook),
            _ => None,
        }
    }

    /// **The real receipt oracle.** Consults the receipt verifier over the real
    /// tree; a surface with no adapter module can never be earned.
    fn receipt_earned_for_real(surface: Surface) -> bool {
        let Some(id) = native_app_of(surface) else {
            return false;
        };
        verify_receipt(id, crate::native_apps::tests::carry_seam(id)).is_earned()
    }

    fn ruling() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/surface-ruling-2026-08-05.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()));
        serde_json::from_slice(&bytes).expect("the surface ruling is valid JSON")
    }

    fn slugs(value: &serde_json::Value, key: &str) -> BTreeSet<String> {
        value[key]
            .as_array()
            .unwrap_or_else(|| panic!("the surface ruling has no {key} array"))
            .iter()
            .map(|entry| entry.as_str().expect("ruling slugs are strings").to_owned())
            .collect()
    }

    fn all_surfaces() -> Vec<Surface> {
        SURFACE_CLAIMS.iter().map(|row| row.surface).collect()
    }

    #[test]
    fn the_table_is_exhaustive_and_unique() {
        let mut seen = BTreeSet::new();
        for row in SURFACE_CLAIMS {
            assert!(
                seen.insert(row.surface),
                "{:?} appears twice in SURFACE_CLAIMS",
                row.surface
            );
            assert!(
                !row.reason.is_empty(),
                "{:?} has no reason line; a surface with a badge and no sentence is how a state \
                 collapses",
                row.surface
            );
            assert!(
                !row.authority.is_empty(),
                "{:?} has no authority",
                row.surface
            );
            // `claim_of` must resolve, and must resolve to this row.
            assert_eq!(claim_of(row.surface).surface, row.surface);
        }
        assert_eq!(seen.len(), SURFACE_CLAIMS.len());
    }

    /// The ruled surface list is the owner's, so this reads it rather than
    /// restating it. `PLAN.md` r5-2a closed G-SCOPE; a row invented here would
    /// re-open it.
    #[test]
    fn the_table_covers_exactly_the_ruled_surface_list() {
        let ruled = ruling();
        let mut expected = slugs(&ruled, "chat_carriers");
        expected.extend(slugs(&ruled, "email_carriers"));
        expected.extend(slugs(&ruled, "native_email_carriers"));
        expected.extend(slugs(&ruled, "first_party_surfaces"));

        let covered: BTreeSet<String> = all_surfaces()
            .into_iter()
            .map(|surface| surface.ruling_slug().to_owned())
            .collect();

        assert_eq!(
            covered,
            expected,
            "the claim table and the owner's ruling disagree about which surfaces exist.\n{}",
            render()
        );

        for cut in slugs(&ruled, "cut_surfaces") {
            assert!(
                !covered.contains(&cut),
                "{cut} was CUT by the 2026-08-05 ruling and must not be re-added quietly.\n{}",
                render()
            );
        }
    }

    /// **MUTANT (a) — the gate that refuses a capability claim with no receipt.**
    ///
    /// Runs the real checker over the real table with the real receipt verifier.
    #[test]
    fn no_surface_claims_capability_without_a_sound_live_carry_receipt() {
        let found = violations(SURFACE_CLAIMS, &receipt_earned_for_real);
        assert!(
            found.is_empty(),
            "the claim state says something the evidence does not support:\n{}\n{}",
            found
                .iter()
                .map(|violation| format!("  [{}] {}", violation.kind.slug(), violation.detail))
                .collect::<Vec<_>>()
                .join("\n"),
            render()
        );
    }

    /// **MUTANT (c) — remove the receipt requirement and this goes red.**
    ///
    /// Starves the checker: a row that claims a capability, with an oracle that
    /// says no receipt exists anywhere. If the receipt requirement is deleted
    /// from [`violations`], this finds nothing and fails. `PLAN.md` r4-3: a gate
    /// that cannot fail is decoration.
    #[test]
    fn the_receipt_requirement_cannot_be_removed_without_this_going_red() {
        let never_earned = |_: Surface| false;

        let claiming_without_a_receipt = [SurfaceClaim {
            surface: Surface::Telegram,
            carrier: CarrierEvidence::ProvenLiveWithReceipt,
            delivery: DeliveryEvidence::ProvenLiveBothWays,
            blockers: &[],
            matrix: MatrixPosition::NoRow,
            authority: "mutation probe",
            reason: "mutation probe",
        }];

        let found = violations(&claiming_without_a_receipt, &never_earned);
        assert_eq!(
            claim_for(&claiming_without_a_receipt[0]),
            PublicClaim::Beta,
            "the probe must actually reach a capability claim, or it proves nothing"
        );
        assert!(
            found
                .iter()
                .any(|v| v.kind == ClaimViolationKind::CapabilityClaimWithoutALiveReceipt),
            "the receipt requirement is gone: a surface claimed as working with no receipt was \
             accepted. Found: {found:?}"
        );
        assert!(
            found
                .iter()
                .any(|v| v.kind == ClaimViolationKind::ReceiptClaimedButNotEarned),
            "a row claiming ProvenLiveWithReceipt was accepted with no receipt behind it. \
             Found: {found:?}"
        );

        // And the same row with a receipt behind it is clean, so the gate is
        // measuring the receipt and not the label.
        let always_earned = |_: Surface| true;
        assert!(
            violations(&claiming_without_a_receipt, &always_earned).is_empty(),
            "the gate fires without regard to the receipt, so it is not measuring the receipt"
        );
    }

    /// The first-party exemption is not a name in this file.
    #[test]
    fn only_ruled_first_party_surfaces_escape_the_receipt_requirement() {
        let ruled = slugs(&ruling(), "first_party_surfaces");
        let exempt: BTreeSet<String> = all_surfaces()
            .into_iter()
            .filter(|surface| is_first_party(*surface))
            .map(|surface| surface.ruling_slug().to_owned())
            .collect();
        assert_eq!(
            exempt, ruled,
            "the receipt-gate exemption must be exactly the owner's first-party ruling"
        );

        // And a carrier surface wearing the first-party evidence is refused.
        let smuggled = [SurfaceClaim {
            surface: Surface::Discord,
            carrier: CarrierEvidence::NoCarrierByConstruction,
            delivery: DeliveryEvidence::ProvenLiveBothWays,
            blockers: &[],
            matrix: MatrixPosition::NoRow,
            authority: "mutation probe",
            reason: "mutation probe",
        }];
        let found = violations(&smuggled, &|_| false);
        assert!(
            found
                .iter()
                .any(|v| v.kind == ClaimViolationKind::FirstPartyExemptionOffTheRuling),
            "a carrier surface claimed the first-party exemption and was accepted: {found:?}"
        );
    }

    /// **MUTANT (b) — collapse two states the allowlist requires separate.**
    ///
    /// Drives the derivation over its whole input space rather than over the
    /// five rows that happen to exist, so a collapse that no current surface
    /// exercises is still caught.
    #[test]
    fn the_derivation_never_collapses_two_states_the_allowlist_separates() {
        const CARRIERS: &[CarrierEvidence] = &[
            CarrierEvidence::NotBuilt,
            CarrierEvidence::BuiltNeverProvenLive,
            CarrierEvidence::MeasuredAndRefused,
            CarrierEvidence::ExternallyBlocked,
            CarrierEvidence::NoCarrierByConstruction,
            CarrierEvidence::ProvenLiveWithReceipt,
        ];
        const DELIVERIES: &[DeliveryEvidence] = &[
            DeliveryEvidence::NotDeliverable,
            DeliveryEvidence::NeverProvenLive,
            DeliveryEvidence::ProvenLiveBothWays,
        ];
        const BLOCKERS: &[ClaimBlocker] = &[
            ClaimBlocker::OpenSecurityFinding,
            ClaimBlocker::UnknownRecheckRequired,
            ClaimBlocker::SendInputGeneralisation,
            ClaimBlocker::WebSurfaceLegalReview,
        ];

        // 0. WHICH blockers extinguish is the allowlist's decision, not this
        //    module's, so the SET is asserted by name before anything iterates
        //    it. Clause 1 below filters on `extinguishes_claim()` and then
        //    asserts those blockers extinguish -- which is a check that cannot
        //    fail, because narrowing the set also narrows what it examines.
        //    A mutation proved exactly that: removing `UnknownRecheckRequired`
        //    from `extinguishes_claim` left clause 1 GREEN (observed exit 0).
        //    Allowlist §E names two statuses that earn no badge and no claim,
        //    and rule 5 makes "stale, conflicting, or not naming an exact
        //    build" the second of them. There are exactly two.
        let extinguishing: Vec<ClaimBlocker> = BLOCKERS
            .iter()
            .copied()
            .filter(|blocker| blocker.extinguishes_claim())
            .collect();
        assert_eq!(
            extinguishing,
            vec![
                ClaimBlocker::OpenSecurityFinding,
                ClaimBlocker::UnknownRecheckRequired
            ],
            "the set of claim-extinguishing blockers is the allowlist's: §E maps \
             `open-security-finding` and `unknown-recheck-required` to NO BADGE AND NO CLAIM, and \
             nothing else on this list may join or leave that set here. Dropping one silently \
             promotes every row that carries it."
        );
        // And each of the two, named rather than filtered, extinguishes on its
        // own -- so the assertion above cannot be satisfied by a set that is
        // right while the behaviour is wrong.
        for blocker in [
            ClaimBlocker::OpenSecurityFinding,
            ClaimBlocker::UnknownRecheckRequired,
        ] {
            let row = SurfaceClaim {
                surface: Surface::Discord,
                carrier: CarrierEvidence::ProvenLiveWithReceipt,
                delivery: DeliveryEvidence::ProvenLiveBothWays,
                blockers: &[],
                matrix: MatrixPosition::NoRow,
                authority: "named-extinguisher probe",
                reason: "named-extinguisher probe",
            };
            assert_eq!(
                claim_for(&SurfaceClaim {
                    blockers: match blocker {
                        ClaimBlocker::OpenSecurityFinding => &[ClaimBlocker::OpenSecurityFinding],
                        _ => &[ClaimBlocker::UnknownRecheckRequired],
                    },
                    ..row
                }),
                PublicClaim::NoClaim,
                "{blocker:?} did not extinguish the claim on a row that would otherwise reach \
                 `Beta`. Allowlist §E: it outranks everything else on the row."
            );
            // The same row WITHOUT the blocker does reach `Beta`, so the check
            // above is measuring the blocker and not the row.
            assert_eq!(claim_for(&row), PublicClaim::Beta);
        }

        // 1. Allowlist §E: an extinguishing blocker outranks everything on the
        //    row, however well built the feature is. Checked over the whole
        //    product, not on the one row that has one.
        for blocker in BLOCKERS.iter().filter(|b| b.extinguishes_claim()) {
            for carrier in CARRIERS {
                for delivery in DELIVERIES {
                    let row = SurfaceClaim {
                        surface: Surface::Discord,
                        carrier: *carrier,
                        delivery: *delivery,
                        blockers: std::slice::from_ref(blocker),
                        matrix: MatrixPosition::NoRow,
                        authority: "exhaustive derivation probe",
                        reason: "exhaustive derivation probe",
                    };
                    assert_eq!(
                        claim_for(&row),
                        PublicClaim::NoClaim,
                        "{:?} on ({carrier:?}, {delivery:?}) did not extinguish the claim. \
                         Allowlist §E: open-security-finding outranks everything else on the row.",
                        blocker
                    );
                }
            }
        }

        // 2. ExternallyBlocked is reached only from ExternallyBlocked evidence
        //    or an ExternallyBlocked matrix row. It is an assertion about a
        //    third party and must never be produced by weakening ours.
        for carrier in CARRIERS {
            for delivery in DELIVERIES {
                let derived = derived_claim(*carrier, *delivery);
                assert_eq!(
                    derived == PublicClaim::ExternallyBlocked,
                    *carrier == CarrierEvidence::ExternallyBlocked,
                    "({carrier:?}, {delivery:?}) derived {derived:?}"
                );
            }
        }

        // 3. Incomparable authorities earn NO CLAIM, never a chosen side.
        assert_eq!(
            weaker_of(PublicClaim::Experimental, PublicClaim::ExternallyBlocked),
            PublicClaim::NoClaim
        );
        assert_eq!(
            weaker_of(PublicClaim::ExternallyBlocked, PublicClaim::Beta),
            PublicClaim::NoClaim
        );
        // Comparable ones take the weaker, in both argument orders.
        assert_eq!(
            weaker_of(PublicClaim::Experimental, PublicClaim::Planned),
            PublicClaim::Planned
        );
        assert_eq!(
            weaker_of(PublicClaim::Planned, PublicClaim::Experimental),
            PublicClaim::Planned
        );

        // 4. The four states this project paid a defect for are distinguishable
        //    in what the user is shown. `Planned` legitimately covers two of
        //    them, so the badge alone is not enough and the reason line carries
        //    the rest -- which is what the checker enforces on the real table.
        let paid_for = [
            CarrierEvidence::NotBuilt,
            CarrierEvidence::BuiltNeverProvenLive,
            CarrierEvidence::MeasuredAndRefused,
            CarrierEvidence::NoCarrierByConstruction,
            CarrierEvidence::ProvenLiveWithReceipt,
        ];
        let mut presentations = BTreeSet::new();
        for carrier in paid_for {
            let row = SurfaceClaim {
                surface: Surface::Discord,
                carrier,
                delivery: DeliveryEvidence::NeverProvenLive,
                blockers: &[],
                matrix: MatrixPosition::NoRow,
                authority: "distinguishability probe",
                reason: carrier.slug(),
            };
            assert!(
                presentations.insert((claim_for(&row), row.reason)),
                "{carrier:?} presents identically to another evidence state"
            );
        }

        // 5. And the checker itself must see a collapse when one is handed to
        //    it, or clause 4 is decoration.
        let collapsed = [
            SurfaceClaim {
                surface: Surface::Whatsapp,
                carrier: CarrierEvidence::MeasuredAndRefused,
                delivery: DeliveryEvidence::NeverProvenLive,
                blockers: &[],
                matrix: MatrixPosition::NoRow,
                authority: "collapse probe",
                reason: "the same sentence",
            },
            SurfaceClaim {
                surface: Surface::Gmail,
                carrier: CarrierEvidence::NotBuilt,
                delivery: DeliveryEvidence::NotDeliverable,
                blockers: &[],
                matrix: MatrixPosition::NoRow,
                authority: "collapse probe",
                reason: "the same sentence",
            },
        ];
        let found = violations(&collapsed, &|_| false);
        assert!(
            found
                .iter()
                .any(|v| v.kind == ClaimViolationKind::CollapsedWithAnotherState),
            "two different evidence states presented identically and the checker accepted it: \
             {found:?}"
        );
    }

    /// The census is computed from the receipt verifier, so the day a receipt is
    /// earned this test changes on its own rather than being edited.
    #[test]
    fn the_carrier_receipt_census_is_computed_and_states_the_truth() {
        let (earned, of) = carrier_receipt_census();
        assert_eq!(of, carrier_surface_count());
        assert!(of >= 14, "the ruled carrier list shrank unexpectedly: {of}");

        let verified = all_surfaces()
            .into_iter()
            .filter(|surface| !is_first_party(*surface))
            .filter(|surface| receipt_earned_for_real(*surface))
            .count();
        assert_eq!(
            earned,
            verified,
            "the table records {earned} receipts and the receipt verifier confirms {verified}.\n{}",
            render()
        );

        // Computed, not asserted: print where each carrier's receipt would live
        // and whether it is there, so "carry-receipts/ is absent" is a reading
        // rather than a claim in a plan document.
        for surface in all_surfaces() {
            if let Some(id) = native_app_of(surface) {
                println!(
                    "  {:<14} receipt {:<7} at {}",
                    surface.ruling_slug(),
                    if receipt_path(id).exists() {
                        "PRESENT"
                    } else {
                        "absent"
                    },
                    receipt_path(id).display()
                );
            }
        }

        let line = carrier_receipt_census_line();
        assert!(
            line.contains(&format!("{earned} of {of}")),
            "the census line must state the real numbers: {line}"
        );
    }

    /// `Available` is reserved for `verified-live` rows and nothing in this
    /// product is one. Held open deliberately: the variant exists so the
    /// derivation is total, and this proves nothing reaches it.
    #[test]
    fn nothing_reaches_available() {
        for surface in all_surfaces() {
            assert_ne!(
                public_claim(surface),
                PublicClaim::Available,
                "{surface:?} reached `Available`, which the allowlist reserves for verified-live \
                 rows on a named release build.\n{}",
                render()
            );
        }
    }

    /// The two surfaces the gate was opened for are no longer inexpressible.
    #[test]
    fn the_four_surfaces_that_had_no_true_label_now_have_one() {
        // Discord: two extinguishing blockers -> no badge, no claim. Not
        // "planned" (false), not "beta" (an overclaim), not "externally
        // blocked" (false about a third party).
        assert_eq!(public_claim(Surface::Discord), PublicClaim::NoClaim);
        assert!(claim_of(Surface::Discord)
            .reason
            .contains("never carried a message through Discord"));

        // Telegram: the evidence supports `Experimental`, the support matrix
        // says `externally blocked`, and those are different assertions, so the
        // honest answer is no claim -- and the reason says which two authorities
        // disagree.
        assert_eq!(
            derived_claim(
                claim_of(Surface::Telegram).carrier,
                claim_of(Surface::Telegram).delivery
            ),
            PublicClaim::Experimental,
            "the evidence-only derivation for Telegram must be Experimental; that is the label \
             D-206 could not express"
        );
        assert_eq!(public_claim(Surface::Telegram), PublicClaim::NoClaim);

        // WhatsApp: measured and refused, which is distinct from unbuilt.
        assert_eq!(
            claim_of(Surface::Whatsapp).carrier,
            CarrierEvidence::MeasuredAndRefused
        );
        assert_ne!(
            claim_of(Surface::Whatsapp).reason,
            claim_of(Surface::Gmail).reason,
            "WhatsApp was measured and refused; Gmail was never built. Same badge, different state."
        );

        // OSL Mail: the third state D-221 said the tile did not have. Send and
        // burn work; nothing can be read.
        assert_eq!(
            claim_of(Surface::OslMail).carrier,
            CarrierEvidence::NoCarrierByConstruction
        );
        assert_eq!(
            claim_of(Surface::OslMail).delivery,
            DeliveryEvidence::NotDeliverable
        );
        assert_eq!(public_claim(Surface::OslMail), PublicClaim::Planned);

        // OSL Chats: first-party and proven both ways, so it is the one surface
        // with a capability claim -- and it needs no carry receipt, because
        // r5-2a says it has no carrier to prove.
        assert_eq!(public_claim(Surface::OslChats), PublicClaim::Beta);
        assert!(is_first_party(Surface::OslChats));
    }

    #[test]
    fn print_the_claim_state() {
        println!("{}", render());
    }
}
