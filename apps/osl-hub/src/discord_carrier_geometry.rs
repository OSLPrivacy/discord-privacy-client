//! Data-only planning for a harmless Discord row cover.
//!
//! The planner deliberately accepts only de-identified visible structure
//! (counts per hard line), never message text or ciphertext. It is not a text
//! shaper: callers must measure the actual Discord row locally and fall back
//! to the protected viewport whenever the row contains richer content.

/// Matching the cover to a message's row count discloses coarse length
/// metadata even though it never discloses the message itself.
pub const LENGTH_METADATA_LEAKAGE_WARNING: &str =
    "Shape-matched cover reveals the protected message's approximate line count; use fixed-size padding to hide it.";

pub const DISCORD_CHARACTER_CAP: usize = 2_000;
pub const MAX_HARD_LINES: usize = 64;
pub const MAX_VISIBLE_GRAPHEMES: u32 = 20_000;
pub const MAX_TARGET_LINES: usize = 96;

// Fixed, local-only prose. Selection depends solely on the output row index,
// not on message content or ciphertext.
const SAFE_COVER_LINES: [&str; 4] = [
    "OSL protected message.",
    "Private message protected.",
    "Protected placeholder.",
    "OSL private placeholder.",
];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivacyPaddingMode {
    /// Match only the coarse rendered line count of the protected row.
    ShapeMatched,
    /// Use a message-independent height bucket to reduce length leakage.
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
    CoverWouldWrap,
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
    /// One entry per intended rendered row. Empty on fallback.
    pub cover_lines: Vec<&'static str>,
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
            cover_lines: Vec::new(),
            target,
            padding,
            metadata_warning: LENGTH_METADATA_LEAKAGE_WARNING,
        }
    }

    pub fn cover_text(&self) -> Option<String> {
        (self.decision == CarrierDecision::RowOverlay).then(|| self.cover_lines.join("\n"))
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
    let capacity = capacity as u32;
    let line_count = match padding {
        PrivacyPaddingMode::ShapeMatched => {
            visible
                .hard_line_graphemes
                .iter()
                .try_fold(0usize, |total, count| {
                    // Explicit blank lines occupy one rendered line.
                    let wrapped = usize::try_from((*count).max(1).div_ceil(capacity)).ok()?;
                    total.checked_add(wrapped)
                })
        }
        PrivacyPaddingMode::FixedSize(size) => Some(size.line_count()),
    };
    let Some(line_count) = line_count else {
        return CarrierPlan::fallback(FallbackReason::TargetLineLimit, padding, None);
    };
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

    let shortest_cover = SAFE_COVER_LINES
        .iter()
        .map(|line| line.chars().count())
        .min()
        .unwrap_or(usize::MAX);
    if usize::try_from(capacity).map_or(true, |value| value < shortest_cover) {
        return CarrierPlan::fallback(FallbackReason::CoverWouldWrap, padding, Some(target));
    }

    let cover_lines: Vec<_> = (0..line_count)
        .map(|index| SAFE_COVER_LINES[index % SAFE_COVER_LINES.len()])
        .collect();
    let character_count = cover_lines
        .iter()
        .map(|line| line.chars().count())
        .sum::<usize>()
        + cover_lines.len().saturating_sub(1);
    if character_count > DISCORD_CHARACTER_CAP {
        return CarrierPlan::fallback(FallbackReason::DiscordCharacterCap, padding, Some(target));
    }

    CarrierPlan {
        decision: CarrierDecision::RowOverlay,
        cover_lines,
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
        }
    }

    #[test]
    fn unicode_is_already_reduced_to_graphemeish_counts() {
        // Five visible grapheme clusters can represent ASCII, emoji ZWJ
        // sequences, or combining Unicode without ever passing text here.
        let visible = plain(&[5]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.decision, CarrierDecision::RowOverlay);
        assert_eq!(plan.target.unwrap().line_count, 1);
        assert_eq!(plan.cover_lines.len(), 1);
    }

    #[test]
    fn explicit_and_trailing_newlines_each_keep_a_row() {
        let visible = plain(&[3, 0, 2, 0]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.target.unwrap().line_count, 4);
        assert_eq!(plan.cover_text().unwrap().lines().count(), 4);
    }

    #[test]
    fn zoom_and_density_change_wrapping_and_height() {
        let visible = plain(&[40]);
        let normal = plan_carrier(input(&visible));
        let mut scaled_input = input(&visible);
        scaled_input.metrics.as_mut().unwrap().zoom = 1.25;
        scaled_input.metrics.as_mut().unwrap().density = 1.5;
        let scaled = plan_carrier(scaled_input);
        assert_eq!(normal.target.unwrap().line_count, 2);
        assert_eq!(scaled.target.unwrap().line_count, 3);
        assert_eq!(scaled.target.unwrap().target_height_px, 112.5);
    }

    #[test]
    fn fixed_padding_does_not_follow_visible_length() {
        let short = plain(&[1]);
        let long = plain(&[400]);
        let mut short_input = input(&short);
        short_input.padding = PrivacyPaddingMode::FixedSize(FixedPaddingSize::Standard);
        let mut long_input = input(&long);
        long_input.padding = PrivacyPaddingMode::FixedSize(FixedPaddingSize::Standard);
        assert_eq!(plan_carrier(short_input).cover_lines.len(), 4);
        assert_eq!(plan_carrier(long_input).cover_lines.len(), 4);
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
    fn structure_and_target_limits_are_bounded() {
        let too_many_hard_lines = plain(&vec![1; MAX_HARD_LINES + 1]);
        assert_eq!(
            plan_carrier(input(&too_many_hard_lines)).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::StructureLimit)
        );

        let too_many_wrapped_lines = plain(&[MAX_VISIBLE_GRAPHEMES]);
        let mut narrow = input(&too_many_wrapped_lines);
        narrow.content_width_px = 96.0;
        assert_eq!(
            plan_carrier(narrow).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::TargetLineLimit)
        );
    }

    #[test]
    fn narrow_rows_and_discord_character_cap_fail_closed() {
        let visible = plain(&[1]);
        let mut narrow = input(&visible);
        narrow.content_width_px = 80.0;
        assert_eq!(
            plan_carrier(narrow).decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::CoverWouldWrap)
        );

        let cap = plain(&[2_880]);
        let plan = plan_carrier(input(&cap));
        assert_eq!(plan.target.unwrap().line_count, MAX_TARGET_LINES);
        assert_eq!(
            plan.decision,
            CarrierDecision::ProtectedViewportFallback(FallbackReason::DiscordCharacterCap)
        );
    }

    #[test]
    fn warning_makes_shape_leakage_explicit() {
        let visible = plain(&[10]);
        let plan = plan_carrier(input(&visible));
        assert_eq!(plan.metadata_warning, LENGTH_METADATA_LEAKAGE_WARNING);
        assert!(plan.metadata_warning.contains("line count"));
    }
}
