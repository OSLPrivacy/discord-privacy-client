//! Geometry planning for the payload-bearing Discord carrier row.
//!
//! The row OSL types into Discord is the message's own wordbank flagtext: the
//! prose-token cover produced by `stego::encode_token` through
//! `ipc::prose_token::prose_token_send`, which encodes the fixed-size
//! cipher-store pointer for this exact message. The planner never sees the
//! draft: besides that already-public flagtext it accepts only de-identified
//! visible structure (counts per hard line), never message text or ciphertext.
//!
//! The flagtext is payload-bearing, so no character of it may be substituted,
//! padded, reordered or dropped — doing so would destroy the pointer the
//! recipient decodes. The one edit the planner does make is to the *separator
//! bytes*: [`stego::shape_cover`] rewrites a `' '` between two words as a
//! `'\n'` so the row renders in a chosen number of lines. Both stego decoders
//! tokenise with `split_whitespace`, so the word sequence, the word order and
//! the character count are all unchanged and decoding cannot see the
//! difference; Discord's renderer, which is the only thing that reacts to it,
//! sizes the row from the resulting line count.
//!
//! That matters because the OSL eye paints the decrypted plaintext *over* the
//! carrier row in place. A one-line cover behind three lines of plaintext
//! overflows into the neighbouring rows and the illusion breaks, so the planner
//! shapes the cover to the row count the protected text will occupy and refuses
//! to place a row it cannot make tall enough
//! ([`FallbackReason::CarrierTooShortForRow`]).
//!
//! Shaping needs the plaintext's *shape* and nothing else: the row budget comes
//! from the de-identified per-hard-line grapheme counts already described above,
//! so no message text or ciphertext reaches this module. Callers must still
//! measure the real Discord row locally and fall back to the protected viewport
//! whenever the row is richer than plain text.

/// The carrier's *character* count still follows the fixed-size cipher-store
/// pointer ([`stego::TOKEN_PAYLOAD_BITS`] worth) rather than the protected
/// message. Its *row* count no longer does: the cover is shaped to the rendered
/// line count of the text it hides, because painted plaintext taller than its
/// carrier row would spill into the neighbouring rows.
///
/// So what a reader of the Discord row learns is the protected message's
/// rendered row count — `log2(rows)` bits, roughly 1-2 for ordinary chat and at
/// most 6 at [`MAX_TARGET_LINES`]. Per-line lengths are deliberately *not*
/// matched, which would leak a length per line instead of a single count.
pub const LENGTH_METADATA_LEAKAGE_WARNING: &str =
    "Carrier length follows the fixed-size message pointer, but the carrier's row count is shaped to match the protected message's rendered line count so painted text cannot overflow the row; that row count, and the presence of a row, are visible.";

pub const DISCORD_CHARACTER_CAP: usize = 2_000;
pub const MAX_HARD_LINES: usize = 64;
pub const MAX_VISIBLE_GRAPHEMES: u32 = 20_000;
/// Rows the planner will admit for one carrier. Mirrored by
/// [`stego::MAX_SHAPED_ROWS`], which is the shaper's own ceiling: a shaped cover
/// taller than this could never be placed. `mirrored_row_ceilings_agree` pins
/// the two together.
pub const MAX_TARGET_LINES: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    /// Average grapheme width before zoom and display-density scaling.
    pub average_grapheme_width_px: f64,
    /// Line-box height before zoom and display-density scaling.
    pub line_height_px: f64,
    /// Discord/app zoom, where 1.0 is 100%.
    pub zoom: f64,
    /// Display density, where 1.0 is 96 DPI.
    pub density: f64,
}

/// De-identified visible shape produced by a trusted local measurer.
///
/// Each entry is the grapheme-ish count of one hard line. A blank entry
/// represents an explicit blank line, including a trailing newline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleStructure {
    pub hard_line_graphemes: Vec<u32>,
    pub has_markdown: bool,
    pub has_media: bool,
    pub has_reply: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedPaddingSize {
    Compact,
    Standard,
    Tall,
}

impl FixedPaddingSize {
    pub const fn line_count(self) -> usize {
        match self {
            Self::Compact => 2,
            Self::Standard => 4,
            Self::Tall => 8,
        }
    }
}

/// Which row count the carrier is shaped to.
///
/// This is a live setting, not a recorded preference: it chooses the row budget
/// handed to [`stego::shape_cover`], and the two modes really do produce
/// differently shaped covers and different row geometry.
///
/// The payload still sets a floor — a cover cannot render in fewer rows than the
/// greedy wrap of its own words — so either mode may come out taller than asked.
/// That direction is benign, because the overlay is sized to the carrier row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivacyPaddingMode {
    /// Shape the cover to the protected text's own rendered line count. This is
    /// the tightest fit for the eye and the weaker of the two privacy options:
    /// the row discloses that line count.
    ShapeMatched,
    /// Shape the cover to a message-independent height bucket instead. The row
    /// then reveals only which bucket was chosen, so this leaks strictly less
    /// than [`Self::ShapeMatched`] and is the better privacy choice. The cost is
    /// a row usually taller than the plaintext needs, and a bucket *shorter*
    /// than the protected text refuses the row outright
    /// ([`FallbackReason::CarrierTooShortForRow`]) rather than let painted text
    /// overflow into a neighbour.
    FixedSize(FixedPaddingSize),
}

#[derive(Clone, Copy, Debug)]
pub struct CarrierGeometryInput<'a> {
    /// Measured physical content width of the Discord message column.
    pub content_width_px: f64,
    /// `None` means the local measurer could not establish trustworthy data.
    pub metrics: Option<LineMetrics>,
    pub visible: &'a VisibleStructure,
    pub padding: PrivacyPaddingMode,
    /// This message's wordbank flagtext, already encoded from its cipher-store
    /// pointer. Empty means the encrypted copy was not prepared for this send,
    /// which fails closed to the protected viewport.
    pub flagtext: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    Markdown,
    Media,
    Reply,
    UnknownMetrics,
    InvalidMetrics,
    UnknownStructure,
    StructureLimit,
    TargetLineLimit,
    /// No wordbank flagtext was prepared for this send, so there is no
    /// payload-bearing row to place.
    FlagtextUnavailable,
    /// The prepared flagtext is not placeable cover text.
    FlagtextRejected,
    /// The cover cannot be stretched to as many rendered rows as the text it has
    /// to hide, so painted plaintext would overflow into the neighbouring
    /// Discord rows. The row is refused instead of faked.
    CarrierTooShortForRow,
    DiscordCharacterCap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierDecision {
    RowOverlay,
    ProtectedViewportFallback(FallbackReason),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryTarget {
    pub line_count: usize,
    pub target_height_px: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CarrierPlan {
    pub decision: CarrierDecision,
    /// The accepted payload-bearing flagtext, present only on `RowOverlay`.
    ///
    /// Carries hard line breaks whenever the row had to be shaped: the same
    /// words in the same order as the encoder produced, with some `' '`
    /// separators rewritten as `'\n'` so the row renders at the height the
    /// painted plaintext needs. Every `'\n'` here becomes exactly one
    /// Shift+Enter in the composer (`native_discord_adapter::carrier_input_steps`),
    /// never a bare Enter, so a shaped cover is still sent as one message.
    flagtext: Option<String>,
    /// Known geometry is retained when planning reached a trustworthy target.
    /// It is absent when content or measurements made row geometry uncertain.
    pub target: Option<GeometryTarget>,
    pub padding: PrivacyPaddingMode,
    pub metadata_warning: &'static str,
}

impl CarrierPlan {
    fn fallback(
        reason: FallbackReason,
        padding: PrivacyPaddingMode,
        target: Option<GeometryTarget>,
    ) -> Self {
        Self {
            decision: CarrierDecision::ProtectedViewportFallback(reason),
            flagtext: None,
            target,
            padding,
            metadata_warning: LENGTH_METADATA_LEAKAGE_WARNING,
        }
    }

    /// The exact text to type into Discord: this message's wordbank flagtext,
    /// word-for-word as the encoder produced it, with its separators shaped to
    /// the row budget.
    pub fn cover_text(&self) -> Option<String> {
        (self.decision == CarrierDecision::RowOverlay)
            .then(|| self.flagtext.clone())
            .flatten()
    }
}

pub fn plan_carrier(input: CarrierGeometryInput<'_>) -> CarrierPlan {
    let padding = input.padding;
    let visible = input.visible;

    if visible.has_media {
        return CarrierPlan::fallback(FallbackReason::Media, padding, None);
    }
    if visible.has_reply {
        return CarrierPlan::fallback(FallbackReason::Reply, padding, None);
    }
    if visible.has_markdown {
        return CarrierPlan::fallback(FallbackReason::Markdown, padding, None);
    }
    if visible.hard_line_graphemes.is_empty() {
        return CarrierPlan::fallback(FallbackReason::UnknownStructure, padding, None);
    }
    if visible.hard_line_graphemes.len() > MAX_HARD_LINES
        || visible
            .hard_line_graphemes
            .iter()
            .try_fold(0u32, |sum, count| sum.checked_add(*count))
            .is_none_or(|sum| sum > MAX_VISIBLE_GRAPHEMES)
    {
        return CarrierPlan::fallback(FallbackReason::StructureLimit, padding, None);
    }

    let Some(metrics) = input.metrics else {
        return CarrierPlan::fallback(FallbackReason::UnknownMetrics, padding, None);
    };
    if !valid_positive(input.content_width_px)
        || !valid_positive(metrics.average_grapheme_width_px)
        || !valid_positive(metrics.line_height_px)
        || !valid_positive(metrics.zoom)
        || !valid_positive(metrics.density)
    {
        return CarrierPlan::fallback(FallbackReason::InvalidMetrics, padding, None);
    }

    let scaled_grapheme_width = metrics.average_grapheme_width_px * metrics.zoom * metrics.density;
    let scaled_line_height = metrics.line_height_px * metrics.zoom * metrics.density;
    if !valid_positive(scaled_grapheme_width) || !valid_positive(scaled_line_height) {
        return CarrierPlan::fallback(FallbackReason::InvalidMetrics, padding, None);
    }

    let capacity = (input.content_width_px / scaled_grapheme_width).floor();
    if !capacity.is_finite() || capacity < 1.0 || capacity > u32::MAX as f64 {
        return CarrierPlan::fallback(FallbackReason::InvalidMetrics, padding, None);
    }
    let Ok(capacity) = usize::try_from(capacity as u32) else {
        return CarrierPlan::fallback(FallbackReason::InvalidMetrics, padding, None);
    };

    // The flagtext's words are payload-bearing, so they are accepted or refused
    // whole. A `'\n'` is admitted because separators are the one thing shaping
    // rewrites, and because a cover that arrives already shaped must still be
    // placeable; this matches `native_discord_adapter::valid_cover`, which has
    // always allowed `'\n'`.
    if input.flagtext.is_empty() {
        return CarrierPlan::fallback(FallbackReason::FlagtextUnavailable, padding, None);
    }
    let flag_characters = input.flagtext.chars().count();
    if flag_characters > DISCORD_CHARACTER_CAP
        || input.flagtext.len() > DISCORD_CHARACTER_CAP * 4
        || input
            .flagtext
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return CarrierPlan::fallback(FallbackReason::FlagtextRejected, padding, None);
    }

    // Rows the painted plaintext will occupy in this column. This is the only
    // place the measured column width exists, so it is the only place the cover
    // can be shaped to it.
    let protected_rows = stego::rows_for_hard_lines(&visible.hard_line_graphemes, capacity);
    let target_rows = match padding {
        PrivacyPaddingMode::ShapeMatched => protected_rows,
        // A message-independent bucket: the row then discloses the bucket rather
        // than the plaintext's own height.
        PrivacyPaddingMode::FixedSize(size) => size.line_count(),
    };

    // Shaping only rewrites separator bytes — a `' '` becomes a `'\n'`. Both
    // stego decoders tokenise with `split_whitespace`, so the word sequence and
    // the character count are untouched and the pointer still decodes; only
    // Discord's renderer reacts.
    let shaped = stego::shape_cover(input.flagtext, stego::RowBudget::new(target_rows, capacity));
    // `ShapedCover::rows` is `stego::rendered_rows` of the text that will
    // actually be typed — hard breaks honoured first, then the identical greedy
    // wrap per hard line. It replaces this module's old local wrap, which
    // tokenised with `split_whitespace` and so undercounted a shaped cover. It is
    // also honest about an empty cover (0 rows) where `rendered_rows("")` is 1.
    let line_count = shaped.rows;
    // `Taller` is benign and deliberately accepted: `GeometryTarget` sizes the
    // overlay to the *carrier* row, so painted plaintext still lands inside it
    // and the bubble is merely taller than it had to be. Dropping payload to
    // shrink it is never an option.
    //
    // `Shorter`, and equally a fixed-size bucket below the protected text's own
    // height, is the direction that breaks the illusion: painted plaintext would
    // spill into the neighbouring Discord rows. Fail closed to the protected
    // viewport rather than place a row that cannot hold what covers it.
    if shaped.outcome.overflows_row() || line_count < protected_rows {
        return CarrierPlan::fallback(FallbackReason::CarrierTooShortForRow, padding, None);
    }
    let flagtext = shaped.into_text();
    // Unreachable while the shortfall check above holds — an empty cover reports
    // `Shorter` — but a row with nothing to type is refused, not placed.
    if flagtext.is_empty() {
        return CarrierPlan::fallback(FallbackReason::FlagtextRejected, padding, None);
    }
    if line_count == 0 || line_count > MAX_TARGET_LINES {
        return CarrierPlan::fallback(FallbackReason::TargetLineLimit, padding, None);
    }

    let target = GeometryTarget {
        line_count,
        target_height_px: scaled_line_height * line_count as f64,
    };
    if !target.target_height_px.is_finite() {
        return CarrierPlan::fallback(FallbackReason::InvalidMetrics, padding, None);
    }

    CarrierPlan {
        decision: CarrierDecision::RowOverlay,
        flagtext: Some(flagtext),
        target: Some(target),
        padding,
        metadata_warning: LENGTH_METADATA_LEAKAGE_WARNING,
    }
}

fn valid_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in for a real `stego::encode_token` cover: plausible chat prose from
    /// the wordbank, no marker and no base64, as the encoder hands it over —
    /// single-space separated, before the planner shapes its separators. Fifteen
    /// words and 77 characters, so in a 30-character column it cannot render in
    /// fewer than three rows nor more than fifteen.
    const FLAGTEXT: &str =
        "i that same thing little know if oh that sounds good one going today honestly";

    fn plain(counts: &[u32]) -> VisibleStructure {
        VisibleStructure {
            hard_line_graphemes: counts.to_vec(),
            has_markdown: false,
            has_media: false,
            has_reply: false,
        }
    }

    fn input<'a>(visible: &'a VisibleStructure) -> CarrierGeometryInput<'a> {
        CarrierGeometryInput {
            content_width_px: 240.0,
            metrics: Some(LineMetrics {
                average_grapheme_width_px: 8.0,
                line_height_px: 20.0,
                zoom: 1.0,
                density: 1.0,
            }),
            visible,
            padding: PrivacyPaddingMode::ShapeMatched,
            flagtext: FLAGTEXT,
        }
    }

    /// Was `the_carrier_is_the_messages_own_flagtext_byte_for_byte`, which
    /// asserted the carrier contained no `'\n'`. The row is now shaped to the
    /// height of the plaintext painted over it, so separators *are* rewritten.
    /// What still has to hold — and is what the payload actually depends on — is
    /// word-for-word identity and an unchanged character count.
    #[test]
    fn the_carrier_is_the_messages_own_flagtext_word_for_word() {
        let visible = plain(&[5]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.decision, CarrierDecision::RowOverlay);
        let carrier = plan.cover_text().unwrap();
        // No substitution, no filler, no reordering, no dropped word: the
        // pointer the recipient decodes survives untouched.
        assert_eq!(
            carrier.split_whitespace().collect::<Vec<_>>(),
            FLAGTEXT.split_whitespace().collect::<Vec<_>>()
        );
        // One separator byte per gap either way, so the 2000-character cap is
        // unaffected.
        assert_eq!(carrier.chars().count(), FLAGTEXT.chars().count());
        // And the separators really were rewritten: this cover needs three rows
        // in the measured column, so it is typed as three lines.
        assert!(carrier.contains('\n'));
        assert_eq!(carrier.split('\n').count(), plan.target.unwrap().line_count);
    }

    /// A shaped cover is typed with one Shift+Enter per `'\n'`
    /// (`native_discord_adapter::carrier_input_steps`). A blank line, a
    /// whitespace run or an edge separator would either produce a step the
    /// renderer collapses or leave the byte-exact readback proof unprovable, so
    /// none may ever appear.
    #[test]
    fn the_shaped_carrier_is_typeable_and_reads_back_byte_exact() {
        for counts in [vec![5], vec![40], vec![120], vec![10, 10, 10]] {
            let visible = plain(&counts);
            let carrier = plan_carrier(input(&visible))
                .cover_text()
                .expect("a measurable row is placeable");
            assert!(
                !carrier.contains("\n\n"),
                "a blank line types an empty step"
            );
            assert!(!carrier.contains("  "));
            assert!(!carrier.contains(" \n") && !carrier.contains("\n "));
            assert!(!carrier.starts_with(char::is_whitespace));
            assert!(!carrier.ends_with(char::is_whitespace));
            assert!(!carrier.contains('\r'));
            assert!(!carrier.contains('\u{a0}') && !carrier.contains('\u{feff}'));
            assert!(!carrier
                .chars()
                .any(|character| character.is_control() && character != '\n'));
        }
    }

    #[test]
    fn different_messages_never_share_a_carrier() {
        let visible = plain(&[5]);
        let mut first = input(&visible);
        first.flagtext = "ok i will weekend again with you get what i was thinking usual ngl";
        let mut second = input(&visible);
        second.flagtext = "on yeah i feel work think it the week and im friday same thing earlier";
        let first = plan_carrier(first).cover_text().unwrap();
        let second = plan_carrier(second).cover_text().unwrap();
        assert_ne!(first, second);
        // The retired placeholder corpus must not reappear from anywhere.
        for carrier in [&first, &second] {
            assert!(!carrier.contains("OSL protected message"));
            assert!(!carrier.contains("placeholder"));
        }
    }

    /// Inverted. This used to assert that two very different drafts produced an
    /// *identical* carrier and identical row geometry. The row count must now
    /// follow the protected text, or painted plaintext overflows its row — so the
    /// half that survives is the character count (still pointer-sized), and the
    /// half that is now deliberately false is the row count.
    #[test]
    fn carrier_characters_follow_the_pointer_but_the_row_count_follows_the_message() {
        let short = plain(&[1]);
        let long = plain(&[400]);
        let short_plan = plan_carrier(input(&short));
        let long_plan = plan_carrier(input(&long));

        // The cover is the same words either way: its character count still
        // follows the fixed-size pointer and never the draft.
        let (short_carrier, long_carrier) = (
            short_plan.cover_text().unwrap(),
            long_plan.cover_text().unwrap(),
        );
        assert_eq!(short_carrier.chars().count(), long_carrier.chars().count());
        assert_eq!(
            short_carrier.split_whitespace().collect::<Vec<_>>(),
            long_carrier.split_whitespace().collect::<Vec<_>>()
        );

        // But a 400-grapheme draft paints over 14 rows in this column, so its
        // carrier is shaped to 14 rows while the one-grapheme draft's sits at the
        // payload's own three-row floor. This is the accepted structural leak.
        assert_eq!(short_plan.target.unwrap().line_count, 3);
        assert_eq!(long_plan.target.unwrap().line_count, 14);
        assert_ne!(short_carrier, long_carrier);
    }

    #[test]
    fn row_geometry_tracks_the_measured_column_and_zoom() {
        let visible = plain(&[5]);
        // Capacity 30 characters: a five-grapheme draft asks for one row, and the
        // 77-character flagtext cannot fit in fewer than three, so the row is the
        // payload's floor of three.
        let normal = plan_carrier(input(&visible));
        assert_eq!(normal.target.unwrap().line_count, 3);
        assert_eq!(normal.target.unwrap().target_height_px, 60.0);

        let mut scaled = input(&visible);
        scaled.metrics.as_mut().unwrap().zoom = 2.0;
        let scaled = plan_carrier(scaled);
        // Half the column capacity, so more rows and a taller row box.
        assert_eq!(scaled.target.unwrap().line_count, 6);
        assert_eq!(scaled.target.unwrap().target_height_px, 240.0);
    }

    /// Inverted. This used to assert padding was a recorded preference that could
    /// not "resize a payload-bearing row". Shaping makes the mode real: it picks
    /// the row budget. The payload is still untouched — only which row count is
    /// aimed for changes.
    #[test]
    fn padding_mode_chooses_the_row_budget_and_never_alters_the_payload() {
        let visible = plain(&[1]);
        let mut fixed = input(&visible);
        fixed.padding = PrivacyPaddingMode::FixedSize(FixedPaddingSize::Standard);
        let fixed = plan_carrier(fixed);
        let shape_matched = plan_carrier(input(&visible));

        assert_eq!(
            fixed.padding,
            PrivacyPaddingMode::FixedSize(FixedPaddingSize::Standard)
        );
        // A message-independent four-row bucket, versus the payload's own
        // three-row floor for this one-grapheme draft.
        assert_eq!(
            fixed.target.unwrap().line_count,
            FixedPaddingSize::Standard.line_count()
        );
        assert_eq!(shape_matched.target.unwrap().line_count, 3);
        assert_ne!(fixed.target, shape_matched.target);

        // Same words, same character count: the mode moved separators, nothing else.
        for plan in [&fixed, &shape_matched] {
            let carrier = plan.cover_text().unwrap();
            assert_eq!(
                carrier.split_whitespace().collect::<Vec<_>>(),
                FLAGTEXT.split_whitespace().collect::<Vec<_>>()
            );
            assert_eq!(carrier.chars().count(), FLAGTEXT.chars().count());
        }
    }

    /// A fixed-size bucket is the better privacy option only while it is at least
    /// as tall as the text painted over it. A bucket below the protected text's
    /// own height must refuse the row, not place one the plaintext overflows.
    #[test]
    fn a_fixed_bucket_shorter_than_the_protected_text_refuses_the_row() {
        // 400 graphemes in a 30-character column is 14 painted rows; the Compact
        // bucket is 2.
        let visible = plain(&[400]);
        let mut compact = input(&visible);
        compact.padding = PrivacyPaddingMode::FixedSize(FixedPaddingSize::Compact);
        let compact = plan_carrier(compact);
        assert_eq!(
            compact.decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::CarrierTooShortForRow)
        );
        assert_eq!(compact.cover_text(), None);
        // The refusal still reports the operator's preference back.
        assert_eq!(
            compact.padding,
            PrivacyPaddingMode::FixedSize(FixedPaddingSize::Compact)
        );
    }

    /// A cover taller than the plaintext needs is accepted on purpose: the
    /// overlay is sized to the *carrier* row, so painted plaintext still lands
    /// inside it. Dropping payload to shrink the bubble is never the fix.
    #[test]
    fn a_row_taller_than_the_plaintext_needs_is_accepted() {
        let visible = plain(&[5]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.decision, CarrierDecision::RowOverlay);
        let rows = plan.target.unwrap().line_count;
        // One painted row, three carrier rows: benign, and every word is still there.
        assert_eq!(rows, 3);
        assert!(rows > stego::rows_for_hard_lines(&visible.hard_line_graphemes, 30));
        assert_eq!(
            plan.cover_text().unwrap().split_whitespace().count(),
            FLAGTEXT.split_whitespace().count()
        );
    }

    /// A plaintext taller than the payload can ever be stretched to is the
    /// direction that breaks the illusion, so it fails closed.
    #[test]
    fn a_carrier_that_cannot_be_made_tall_enough_refuses_the_row() {
        // Forty hard lines paint forty rows; a fifteen-word cover cannot exceed
        // fifteen rows without inventing words, which would destroy the pointer.
        let visible = plain(&[1; 40]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(
            plan.decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::CarrierTooShortForRow)
        );
        assert_eq!(plan.cover_text(), None);
        assert_eq!(plan.target, None);
    }

    /// The planner's row ceiling and the shaper's must be the same number, or one
    /// of them produces a row the other refuses.
    #[test]
    fn mirrored_row_ceilings_agree() {
        assert_eq!(MAX_TARGET_LINES, stego::MAX_SHAPED_ROWS);
    }

    #[test]
    fn a_missing_encrypted_copy_fails_closed_to_the_viewport() {
        let visible = plain(&[10]);
        let mut absent = input(&visible);
        absent.flagtext = "";
        assert_eq!(
            plan_carrier(absent).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::FlagtextUnavailable)
        );
        assert_eq!(plan_carrier(absent).cover_text(), None);
    }

    /// Inverted. This used to refuse `"first row\nsecond row"` as "not a single
    /// placeable line". A `'\n'` is now the separator shaping itself writes, and
    /// refusing it would refuse every shaped cover, so it is admitted — matching
    /// `native_discord_adapter::valid_cover`, which always allowed it. Every other
    /// control character, and the character cap, still refuse.
    #[test]
    fn flagtext_that_is_not_placeable_cover_is_refused() {
        let visible = plain(&[10]);
        for rejected in [
            "carrier\rreturn".to_owned(),
            "carrier\ttab".to_owned(),
            "a ".repeat(DISCORD_CHARACTER_CAP),
        ] {
            let mut bad = input(&visible);
            bad.flagtext = &rejected;
            assert_eq!(
                plan_carrier(bad).decision,
                CarrierDecision::ProtectedViewportFallback(FallbackReason::FlagtextRejected)
            );
        }

        // A line break is a separator, not a defect: an already-shaped cover
        // must still be placeable.
        let mut shaped = input(&visible);
        shaped.flagtext = "first row\nsecond row";
        assert_eq!(
            plan_carrier(shaped).decision,
            CarrierDecision::RowOverlay,
            "refusing a newline would refuse every shaped cover"
        );

        // Whitespace with no word in it carries no pointer and cannot hide
        // anything, so it is refused rather than typed as an empty row.
        let mut blank = input(&visible);
        blank.flagtext = "   ";
        assert!(matches!(
            plan_carrier(blank).decision,
            CarrierDecision::ProtectedViewportFallback(
                FallbackReason::CarrierTooShortForRow | FallbackReason::FlagtextRejected
            )
        ));
    }

    #[test]
    fn unicode_is_already_reduced_to_graphemeish_counts() {
        // Five visible grapheme clusters can represent ASCII, emoji ZWJ
        // sequences, or combining Unicode without ever passing text here.
        let visible = plain(&[5]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.decision, CarrierDecision::RowOverlay);
    }

    #[test]
    fn invalid_or_missing_measurements_fail_closed() {
        let visible = plain(&[10]);
        let mut missing = input(&visible);
        missing.metrics = None;
        assert_eq!(
            plan_carrier(missing).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::UnknownMetrics)
        );

        let mut invalid = input(&visible);
        invalid.metrics.as_mut().unwrap().line_height_px = f64::NAN;
        assert_eq!(
            plan_carrier(invalid).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::InvalidMetrics)
        );
    }

    #[test]
    fn rich_discord_rows_use_the_protected_viewport() {
        for expected in [
            (true, false, false, FallbackReason::Markdown),
            (false, true, false, FallbackReason::Media),
            (false, false, true, FallbackReason::Reply),
        ] {
            let visible = VisibleStructure {
                hard_line_graphemes: vec![10],
                has_markdown: expected.0,
                has_media: expected.1,
                has_reply: expected.2,
            };
            assert_eq!(
                plan_carrier(input(&visible)).decision,
                CarrierDecision::ProtectedViewportFallback(expected.3)
            );
        }
    }

    #[test]
    fn structure_limits_are_bounded() {
        let too_many_hard_lines = plain(&vec![1; MAX_HARD_LINES + 1]);
        assert_eq!(
            plan_carrier(input(&too_many_hard_lines)).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::StructureLimit)
        );

        let unknown = plain(&[]);
        assert_eq!(
            plan_carrier(input(&unknown)).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::UnknownStructure)
        );
    }

    #[test]
    fn a_column_too_narrow_for_the_payload_row_fails_closed() {
        let visible = plain(&[1]);
        let long_flagtext = "x ".repeat(MAX_TARGET_LINES + 24);
        let mut narrow = input(&visible);
        // One character per row: the flagtext would need more rows than the
        // planner will ever admit, so it must not be placed.
        narrow.content_width_px = 8.0;
        narrow.flagtext = long_flagtext.trim_end();
        assert_eq!(
            plan_carrier(narrow).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::TargetLineLimit)
        );
    }

    /// Inverted. This used to exercise a private `wrapped_line_count` that
    /// tokenised with `split_whitespace` and so treated a `'\n'` as an ordinary
    /// space — it undercounted every shaped cover. The row rule is now
    /// `stego::rendered_rows`, which splits on `'\n'` first and then applies the
    /// identical greedy wrap per hard line. The three word-wrap cases carry over
    /// unchanged; the hard-break case is what the old function got wrong.
    #[test]
    fn row_counting_is_the_shapers_own_rule_and_honours_hard_breaks() {
        assert_eq!(stego::rendered_rows("abcde fgh ijkl", 10), 2);
        assert_eq!(stego::rendered_rows("abcde fgh", 10), 1);
        // A 25-character word in a 10-character column occupies three rows.
        assert_eq!(stego::rendered_rows(&"x".repeat(25), 10), 3);
        // The behaviour the old local wrap could not express: two rows, not one.
        assert_eq!(stego::rendered_rows("abcde\nfgh", 10), 2);
        assert_eq!(stego::rendered_rows("abcde fgh ijkl", 100), 1);
    }

    /// Inverted. The warning used to promise the row disclosed neither length nor
    /// line count. The row count is now shaped to the protected text's, so the
    /// warning has to name that leak; the character count still follows the
    /// pointer, so it keeps saying so too.
    #[test]
    fn warning_states_that_the_row_count_tracks_the_protected_line_count() {
        let visible = plain(&[10]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.metadata_warning, LENGTH_METADATA_LEAKAGE_WARNING);
        assert!(plan.metadata_warning.contains("fixed-size message pointer"));
        assert!(plan.metadata_warning.contains("row count"));
        assert!(plan.metadata_warning.contains("rendered line count"));
        // The retired promise must not come back: the row structure is visible.
        assert!(!plan
            .metadata_warning
            .contains("only the presence of a row is visible"));
    }
}
