//! Rendered-line shaping for payload-bearing cover text.
//!
//! # Why this exists
//!
//! Discord sizes a message row from the **rendered** line count of the text it
//! received. The OSL eye paints decrypted plaintext over that row, in place. If
//! the cover renders as one line and the plaintext is three, the painted text
//! overflows into the neighbouring rows and the illusion breaks. This module
//! makes the cover render with a chosen number of rows so the painted plaintext
//! lands inside the row it is meant to hide behind.
//!
//! # The only lever that is safe to pull
//!
//! Every cover produced by this crate is **word-exact**: the recipient recovers
//! the payload by splitting the message on whitespace and replaying the word
//! sequence, and both decoders reject a stream with an added, removed, altered
//! or reordered word ([`crate::decode_token`] additionally re-derives the
//! canonical word sequence and compares it). So the shaper may not add filler
//! words, drop words, or substitute them.
//!
//! What it *may* do is change **which whitespace byte** separates two words.
//! Both decoders tokenise with `split_whitespace` / `split_ascii_whitespace`,
//! so replacing a `' '` with a `'\n'` is invisible to them while being the one
//! thing Discord's renderer reacts to. Shaping therefore only ever rewrites
//! separators:
//!
//! * the whitespace-separated word sequence is bit-identical to the unshaped
//!   cover, so the payload survives;
//! * exactly one separator byte sits between any two words, so the character
//!   count is unchanged and Discord's 2000-character cap is unaffected;
//! * no run of whitespace, no leading or trailing whitespace, and no blank
//!   line is ever emitted, so Chromium never sees a collapsing space to rewrite
//!   as `\u{a0}` and the readback comparator gets back exactly what was typed.
//!
//! # What is achievable, and what is not
//!
//! The payload sets a **floor**. A cover carrying `n` words cannot render in
//! fewer rows than the greedy word-wrap of those `n` words in the measured
//! column ([`min_rows`](ShapedCover::min_rows)), and it cannot render in more
//! rows than one word per row ([`max_rows`](ShapedCover::max_rows)). Every row
//! count in between is reachable, so:
//!
//! * `min_rows <= target <= max_rows` → [`RowMatch::Exact`];
//! * `target < min_rows` → [`RowMatch::Taller`]. The payload cannot be made
//!   shorter, so the row is taller than the plaintext needs. This is the benign
//!   direction: the overlay is sized to the carrier row, so the painted
//!   plaintext still sits inside it — the bubble is merely taller than it had
//!   to be.
//! * `target > max_rows` → [`RowMatch::Shorter`]. This is the direction that
//!   breaks the illusion, and the shaper refuses to fake it: it reports the
//!   shortfall so the caller can fail closed to the protected viewport instead
//!   of painting over its neighbours.
//!
//! Nothing here silently produces a wrong row count. The outcome is always
//! reported alongside the text.
//!
//! # Structural side channel
//!
//! A cover shaped to the plaintext's row count discloses that row count to
//! anyone reading the Discord message. That leak is inherent to the
//! requirement and is accepted, but note its size: it is `log2(rows)` bits,
//! roughly 6 bits at the [`MAX_SHAPED_ROWS`] ceiling and typically 1-2 bits for
//! ordinary chat. Nothing else about the plaintext is used — in particular the
//! shaper never matches per-line *lengths*, which would leak far more (a length
//! per line rather than a single count). The distribution of words across rows
//! is derived only from the cover's own words and the row count.
//!
//! # Privacy of this module's own output
//!
//! [`ShapedCover`] holds cover text, so its [`fmt::Debug`] prints counts only.
//! No function here logs, hashes, persists or prints cover or plaintext bytes.

use crate::{ConversationCipher, Result, TOKEN_ID_BYTES};
use core::fmt;

/// Hard ceiling on the rows a shaped cover may occupy.
///
/// Must stay less than or equal to the carrier planner's `MAX_TARGET_LINES`
/// (currently 96 in `apps/osl-hub/src/discord_carrier_geometry.rs`); a shaped
/// cover taller than the planner admits would be refused at plan time.
pub const MAX_SHAPED_ROWS: usize = 96;

/// Word-final punctuation treated as a natural place to break a line, so a
/// shaped cover breaks where a human writing chat would.
const CLAUSE_ENDINGS: [char; 6] = ['.', ',', '!', '?', ';', ':'];

/// The rendered-row budget a cover should hit.
///
/// `target_rows` comes from the plaintext's de-identified visual structure (see
/// [`rows_for_hard_lines`]); `column_graphemes` is the measured Discord column
/// capacity in grapheme-ish units. Neither carries message content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowBudget {
    pub target_rows: usize,
    pub column_graphemes: usize,
}

impl RowBudget {
    pub const fn new(target_rows: usize, column_graphemes: usize) -> Self {
        Self {
            target_rows,
            column_graphemes,
        }
    }
}

/// How the shaped cover's rendered row count relates to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowMatch {
    /// The cover renders in exactly `target_rows` rows.
    Exact,
    /// The payload floor exceeded the target by `extra_rows`. Benign: the
    /// overlay covers the whole carrier row, so the plaintext still fits.
    Taller { extra_rows: usize },
    /// The cover cannot be stretched to the target; it is `missing_rows`
    /// short. Painting over it would overflow into neighbouring rows, so the
    /// caller must fail closed rather than place this row.
    Shorter { missing_rows: usize },
}

impl RowMatch {
    pub const fn is_exact(self) -> bool {
        matches!(self, RowMatch::Exact)
    }

    /// True when painting the plaintext over this row would overflow it.
    pub const fn overflows_row(self) -> bool {
        matches!(self, RowMatch::Shorter { .. })
    }
}

/// A cover whose separators have been rewritten to hit a row budget.
///
/// The text is deliberately not a public field and `Debug` omits it: cover
/// content must never reach a log or a test failure message.
#[derive(Clone, PartialEq, Eq)]
pub struct ShapedCover {
    text: String,
    /// Rows this text actually renders in, at the budget's column width.
    pub rows: usize,
    /// The row count that was asked for.
    pub target_rows: usize,
    /// Fewest rows this payload can occupy in this column.
    pub min_rows: usize,
    /// Most rows this payload can occupy without inventing words.
    pub max_rows: usize,
    pub outcome: RowMatch,
}

impl ShapedCover {
    /// The exact text to type into Discord.
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn into_text(self) -> String {
        self.text
    }

    pub const fn is_exact(&self) -> bool {
        self.outcome.is_exact()
    }
}

impl fmt::Debug for ShapedCover {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Counts only. Never the cover text.
        f.debug_struct("ShapedCover")
            .field("bytes", &self.text.len())
            .field("rows", &self.rows)
            .field("target_rows", &self.target_rows)
            .field("min_rows", &self.min_rows)
            .field("max_rows", &self.max_rows)
            .field("outcome", &self.outcome)
            .finish()
    }
}

/// Rows a plaintext of this de-identified shape will render in.
///
/// `hard_line_graphemes` is the per-hard-line grapheme count produced by the
/// hub's `deidentify_prepared_visual_structure`; it carries no text. Each hard
/// line occupies at least one row and wraps every `column_graphemes` units.
///
/// This is a character-wrap estimate, because a grapheme count is all the
/// de-identified structure exposes — the real renderer wraps on word
/// boundaries and so may use one more row for a given line. Treat the result
/// as the row count to aim for, not a proof of the plaintext's exact height.
pub fn rows_for_hard_lines(hard_line_graphemes: &[u32], column_graphemes: usize) -> usize {
    let capacity = column_graphemes.max(1);
    hard_line_graphemes
        .iter()
        .map(|graphemes| {
            let graphemes = *graphemes as usize;
            graphemes.div_ceil(capacity).max(1)
        })
        .fold(0usize, |acc, rows| acc.saturating_add(rows))
}

/// Rows `text` renders in: hard line breaks are honoured, and each hard line is
/// greedily word-wrapped at `column_graphemes`.
///
/// This mirrors the carrier planner's greedy wrap so both sides agree on the
/// row count; the planner should call this rather than keeping a second copy.
pub fn rendered_rows(text: &str, column_graphemes: usize) -> usize {
    let capacity = column_graphemes.max(1);
    text.split('\n')
        .map(|line| {
            let widths: Vec<usize> = line
                .split_whitespace()
                .map(|word| word.chars().count())
                .collect();
            wrap_rows(&widths, capacity).max(1)
        })
        .fold(0usize, |acc, rows| acc.saturating_add(rows))
}

/// Rewrite `cover`'s separators so it renders in `budget.target_rows` rows
/// where the payload allows, and report the outcome when it does not.
///
/// The whitespace-separated word sequence of the result is always identical to
/// that of `cover`, so any cover this crate produces still decodes.
pub fn shape_cover(cover: &str, budget: RowBudget) -> ShapedCover {
    let capacity = budget.column_graphemes.max(1);
    let words: Vec<&str> = cover.split_whitespace().collect();
    if words.is_empty() {
        // Nothing to shape and nothing to hide behind. `rows` is 0 so the
        // caller sees the shortfall rather than an apparently placeable row.
        return ShapedCover {
            text: String::new(),
            rows: 0,
            target_rows: budget.target_rows,
            min_rows: 0,
            max_rows: 0,
            outcome: if budget.target_rows == 0 {
                RowMatch::Exact
            } else {
                RowMatch::Shorter {
                    missing_rows: budget.target_rows,
                }
            },
        };
    }

    let widths: Vec<usize> = words.iter().map(|word| word.chars().count()).collect();
    let mut groups = greedy_groups(&widths, capacity);
    let min_rows = group_rows_total(&groups, &widths, capacity);
    // One word per row, each word still wrapping if it outgrows the column.
    let max_rows = widths
        .iter()
        .map(|width| width.div_ceil(capacity).max(1))
        .fold(0usize, |acc, rows| acc.saturating_add(rows))
        .min(MAX_SHAPED_ROWS.max(min_rows));

    let desired = budget
        .target_rows
        .clamp(min_rows, max_rows.max(min_rows))
        .min(MAX_SHAPED_ROWS.max(min_rows));

    let mut rows = min_rows;
    // Every accepted split adds at least one row, so the loop runs at most
    // `desired - min_rows` times and always terminates.
    while rows < desired {
        let Some(split) = choose_split_that_adds_a_row(&words, &widths, &groups, capacity) else {
            break;
        };
        groups.splice(split.index..split.index + 1, [split.left, split.right]);
        rows += split.added_rows;
    }

    balance_groups(&widths, &mut groups, capacity);

    let text = render_groups(&words, &groups);
    // Measured from the text that will actually be typed, so the reported row
    // count is the rendered one even if a future change to the layout above
    // stops matching its own bookkeeping.
    let rows = rendered_rows(&text, capacity);
    let outcome = if rows == budget.target_rows {
        RowMatch::Exact
    } else if rows > budget.target_rows {
        RowMatch::Taller {
            extra_rows: rows - budget.target_rows,
        }
    } else {
        RowMatch::Shorter {
            missing_rows: budget.target_rows - rows,
        }
    };

    ShapedCover {
        text,
        rows,
        target_rows: budget.target_rows,
        min_rows,
        max_rows,
        outcome,
    }
}

/// [`crate::encode_token`] followed by [`shape_cover`]: the live carrier path,
/// where only a fixed-size pointer rides in the text and the floor is small.
pub fn encode_token_shaped(
    cipher: &ConversationCipher,
    mac_key: &[u8],
    id: &[u8; TOKEN_ID_BYTES],
    budget: RowBudget,
) -> ShapedCover {
    shape_cover(&crate::encode_token(cipher, mac_key, id), budget)
}

/// [`crate::encode_mode1`] followed by [`shape_cover`]. Mode 1 carries payload
/// in the text itself, so its floor is far higher — see the crate tests for the
/// measured numbers.
pub fn encode_mode1_shaped(
    cipher: &ConversationCipher,
    ciphertext: &[u8],
    budget: RowBudget,
) -> Result<ShapedCover> {
    Ok(shape_cover(
        &crate::encode_mode1(cipher, ciphertext)?,
        budget,
    ))
}

// ==========================================================================
// Internals
// ==========================================================================

/// Greedy word wrap, byte-for-byte the same rule the carrier planner uses: a
/// word joins the current row when it and one separator still fit, otherwise it
/// starts a fresh row, splitting only if it outgrows the column on its own.
fn wrap_rows(widths: &[usize], capacity: usize) -> usize {
    let mut rows = 0usize;
    let mut used = 0usize;
    for &width in widths {
        let mut remaining = width;
        if used > 0 && used + 1 + remaining <= capacity {
            used = used + 1 + remaining;
            continue;
        }
        while remaining > capacity {
            rows = rows.saturating_add(1);
            remaining -= capacity;
        }
        rows = rows.saturating_add(1);
        used = remaining;
    }
    rows
}

/// Partition word indices into the groups the greedy wrap would put on each
/// row. Returns half-open `[start, end)` ranges covering every word exactly
/// once, in order.
fn greedy_groups(widths: &[usize], capacity: usize) -> Vec<(usize, usize)> {
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut used = 0usize;
    for (index, &width) in widths.iter().enumerate() {
        if used > 0 && used + 1 + width <= capacity {
            if let Some(last) = groups.last_mut() {
                last.1 = index + 1;
                used = used + 1 + width;
                continue;
            }
        }
        groups.push((index, index + 1));
        let mut remaining = width;
        while remaining > capacity {
            remaining -= capacity;
        }
        used = remaining;
    }
    groups
}

fn group_rows_total(groups: &[(usize, usize)], widths: &[usize], capacity: usize) -> usize {
    groups
        .iter()
        .map(|group| wrap_rows(&widths[group.0..group.1], capacity).max(1))
        .fold(0usize, |acc, rows| acc.saturating_add(rows))
}

struct ChosenSplit {
    index: usize,
    left: (usize, usize),
    right: (usize, usize),
    added_rows: usize,
}

/// Pick the next group to break in two: the one holding the most words whose
/// split actually adds a row, so rows stay evenly filled and the caller's loop
/// always makes progress. Ties go to the earliest group for determinism.
/// `None` when no remaining split can add a row.
fn choose_split_that_adds_a_row(
    words: &[&str],
    widths: &[usize],
    groups: &[(usize, usize)],
    capacity: usize,
) -> Option<ChosenSplit> {
    let mut best: Option<(ChosenSplit, usize)> = None;
    for (index, group) in groups.iter().enumerate() {
        let word_count = group.1 - group.0;
        if word_count < 2 {
            continue;
        }
        if best
            .as_ref()
            .is_some_and(|(_, best_words)| word_count <= *best_words)
        {
            continue;
        }
        let (left, right) = split_group(words, *group);
        let before = wrap_rows(&widths[group.0..group.1], capacity);
        let after = wrap_rows(&widths[left.0..left.1], capacity)
            + wrap_rows(&widths[right.0..right.1], capacity);
        if after <= before {
            continue;
        }
        best = Some((
            ChosenSplit {
                index,
                left,
                right,
                added_rows: after - before,
            },
            word_count,
        ));
    }
    best.map(|(split, _)| split)
}

/// Split a group in two. The break lands on the clause boundary nearest the
/// middle when the group has one, so the shaped cover breaks where a human
/// writing chat would; otherwise it lands on the middle word boundary.
fn split_group(words: &[&str], group: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    let (start, end) = group;
    debug_assert!(end - start >= 2);
    let middle = (start + (end - start) / 2).clamp(start + 1, end - 1);
    let mut chosen = middle;
    let mut best_distance = usize::MAX;
    for point in (start + 1)..end {
        if ends_clause(words[point - 1]) {
            let distance = point.abs_diff(middle);
            if distance < best_distance {
                best_distance = distance;
                chosen = point;
            }
        }
    }
    ((start, chosen), (chosen, end))
}

/// Width a group occupies on one row: its words plus one separator between
/// each pair.
fn group_width(widths: &[usize], group: (usize, usize)) -> usize {
    widths[group.0..group.1].iter().sum::<usize>() + (group.1 - group.0).saturating_sub(1)
}

/// Even out the rows without changing their count or the word order.
///
/// Greedy wrapping fills the early rows to the column edge and leaves a stub at
/// the end, and splitting to reach a taller target inherits that lopsidedness.
/// A cover of eight words on one row followed by single-word rows reads as
/// machine output even though every word is untouched. This pass smooths the
/// rows by repeatedly sliding the boundary word between two adjacent rows
/// whenever that brings their widths closer together, which cascades a trailing
/// stub back through the whole cover.
///
/// A word only ever moves between adjacent rows, and never when the move would
/// empty a row or push one past the column, so the row count is unchanged and
/// the word order is preserved. Groups holding a word wider than the column are
/// left alone: they already occupy more than one row, and shuffling words around
/// them would change the count.
fn balance_groups(widths: &[usize], groups: &mut [(usize, usize)], capacity: usize) {
    if groups.len() < 2 || widths.iter().any(|&width| width > capacity) {
        return;
    }
    for _ in 0..(widths.len() * 4 + 8) {
        let mut moved = false;
        for index in 0..groups.len() - 1 {
            let (left, right) = (groups[index], groups[index + 1]);
            let (left_width, right_width) = (group_width(widths, left), group_width(widths, right));
            let spread = left_width.abs_diff(right_width);

            // Slide the boundary word down into the right-hand row.
            if left.1 - left.0 >= 2 {
                let word = widths[left.1 - 1];
                let (shrunk, grown) = (left_width - word - 1, right_width + word + 1);
                if grown <= capacity && shrunk.abs_diff(grown) < spread {
                    groups[index].1 -= 1;
                    groups[index + 1].0 -= 1;
                    moved = true;
                    continue;
                }
            }
            // Or pull it up into the left-hand row.
            if right.1 - right.0 >= 2 {
                let word = widths[right.0];
                let (grown, shrunk) = (left_width + word + 1, right_width - word - 1);
                if grown <= capacity && shrunk.abs_diff(grown) < spread {
                    groups[index].1 += 1;
                    groups[index + 1].0 += 1;
                    moved = true;
                }
            }
        }
        if !moved {
            return;
        }
    }
}

fn ends_clause(word: &str) -> bool {
    word.chars()
        .next_back()
        .is_some_and(|character| CLAUSE_ENDINGS.contains(&character))
}

/// Join groups into text: one `' '` between words on a row, one `'\n'` between
/// rows. Never a run of whitespace, never a blank line, never a leading or
/// trailing separator — so nothing here can be rewritten in transit.
fn render_groups(words: &[&str], groups: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(words.iter().map(|word| word.len() + 1).sum());
    for (group_index, group) in groups.iter().enumerate() {
        if group_index > 0 {
            out.push('\n');
        }
        for (offset, word) in words[group.0..group.1].iter().enumerate() {
            if offset > 0 {
                out.push(' ');
            }
            out.push_str(word);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in cover with the shape a wordbank cover has: short lowercase
    /// words, single-space separated.
    const WORDS: &str =
        "lol that same thing little know if oh that sounds good one going today honestly";

    #[test]
    fn wrap_matches_the_carrier_planner_greedy_rule() {
        assert_eq!(wrap_rows(&[5, 3, 4], 10), 2);
        assert_eq!(wrap_rows(&[5, 3], 10), 1);
        assert_eq!(wrap_rows(&[25], 10), 3);
        assert_eq!(wrap_rows(&[], 10), 0);
    }

    #[test]
    fn rendered_rows_honours_hard_breaks_and_soft_wrap() {
        assert_eq!(rendered_rows("aaaa bbbb", 20), 1);
        assert_eq!(rendered_rows("aaaa\nbbbb", 20), 2);
        // A hard line that still outgrows the column wraps on top of the break.
        assert_eq!(rendered_rows(&format!("{}\nshort", "x".repeat(25)), 10), 4);
    }

    #[test]
    fn hard_line_rows_charge_one_row_per_blank_line() {
        assert_eq!(rows_for_hard_lines(&[0], 80), 1);
        assert_eq!(rows_for_hard_lines(&[0, 0, 0], 80), 3);
        assert_eq!(rows_for_hard_lines(&[10], 80), 1);
        assert_eq!(rows_for_hard_lines(&[160], 80), 2);
        assert_eq!(rows_for_hard_lines(&[161], 80), 3);
    }

    #[test]
    fn greedy_groups_cover_every_word_in_order() {
        let widths: Vec<usize> = WORDS.split(' ').map(|w| w.len()).collect();
        for capacity in [1usize, 4, 7, 20, 40, 200] {
            let groups = greedy_groups(&widths, capacity);
            assert_eq!(groups.first().unwrap().0, 0);
            assert_eq!(groups.last().unwrap().1, widths.len());
            for pair in groups.windows(2) {
                assert_eq!(pair[0].1, pair[1].0, "groups must tile the word list");
            }
        }
    }

    #[test]
    fn splitting_prefers_a_clause_boundary_near_the_middle() {
        let words = ["a", "b.", "c", "d", "e", "f"];
        // Middle boundary is 3; the clause boundary after "b." is at 2.
        assert_eq!(split_group(&words, (0, 6)), ((0, 2), (2, 6)));
        let plain = ["a", "b", "c", "d"];
        assert_eq!(split_group(&plain, (0, 4)), ((0, 2), (2, 4)));
    }

    #[test]
    fn shaping_never_alters_the_word_sequence_or_the_character_count() {
        for target in 1..=WORDS.split(' ').count() {
            let shaped = shape_cover(WORDS, RowBudget::new(target, 30));
            assert_eq!(
                shaped.text().split_whitespace().collect::<Vec<_>>(),
                WORDS.split_whitespace().collect::<Vec<_>>(),
                "shaping must only rewrite separators"
            );
            assert_eq!(shaped.text().chars().count(), WORDS.chars().count());
        }
    }

    #[test]
    fn shaped_text_has_no_round_trip_hazard() {
        for target in 1..=20 {
            let shaped = shape_cover(WORDS, RowBudget::new(target, 30));
            let text = shaped.text();
            assert!(!text.contains("  "), "no collapsing space run");
            assert!(!text.contains(" \n") && !text.contains("\n "));
            assert!(!text.contains("\n\n"), "no blank line");
            assert!(!text.contains('\r'));
            assert!(!text.contains('\u{a0}') && !text.contains('\u{feff}'));
            assert!(!text.starts_with(char::is_whitespace));
            assert!(!text.ends_with(char::is_whitespace));
            assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
        }
    }

    #[test]
    fn every_row_count_between_the_floor_and_the_ceiling_is_hit_exactly() {
        let probe = shape_cover(WORDS, RowBudget::new(1, 30));
        let (floor, ceiling) = (probe.min_rows, probe.max_rows);
        assert!(floor >= 1 && ceiling >= floor);
        for target in floor..=ceiling {
            let shaped = shape_cover(WORDS, RowBudget::new(target, 30));
            assert_eq!(shaped.rows, target, "target {target} was not hit");
            assert_eq!(shaped.outcome, RowMatch::Exact);
        }
    }

    #[test]
    fn a_target_under_the_floor_reports_the_extra_rows() {
        // Column narrow enough that the words cannot fit on one row.
        let shaped = shape_cover(WORDS, RowBudget::new(1, 12));
        assert!(shaped.min_rows > 1);
        assert_eq!(shaped.rows, shaped.min_rows);
        assert_eq!(
            shaped.outcome,
            RowMatch::Taller {
                extra_rows: shaped.min_rows - 1
            }
        );
        assert!(!shaped.outcome.overflows_row(), "taller must stay benign");
    }

    #[test]
    fn a_target_over_the_ceiling_reports_the_shortfall_instead_of_faking_it() {
        let words = WORDS.split(' ').count();
        let shaped = shape_cover(WORDS, RowBudget::new(words + 9, 30));
        assert_eq!(shaped.rows, shaped.max_rows);
        assert_eq!(
            shaped.outcome,
            RowMatch::Shorter {
                missing_rows: words + 9 - shaped.max_rows
            }
        );
        assert!(shaped.outcome.overflows_row());
        // The shortfall must never be papered over with blank lines.
        assert!(!shaped.text().contains("\n\n"));
    }

    #[test]
    fn the_row_ceiling_is_clamped_so_the_planner_can_still_place_the_row() {
        let many: String = std::iter::repeat_n("ok", MAX_SHAPED_ROWS * 2)
            .collect::<Vec<_>>()
            .join(" ");
        let shaped = shape_cover(&many, RowBudget::new(MAX_SHAPED_ROWS * 2, 40));
        assert!(shaped.rows <= MAX_SHAPED_ROWS);
        assert_eq!(shaped.max_rows, MAX_SHAPED_ROWS);
    }

    #[test]
    fn an_empty_cover_reports_a_shortfall_rather_than_a_placeable_row() {
        let shaped = shape_cover("   ", RowBudget::new(3, 40));
        assert_eq!(shaped.rows, 0);
        assert!(shaped.text().is_empty());
        assert_eq!(shaped.outcome, RowMatch::Shorter { missing_rows: 3 });
    }

    #[test]
    fn an_overlong_single_word_still_shapes_without_losing_it() {
        let long = "x".repeat(50);
        let cover = format!("hey {long} ok");
        let shaped = shape_cover(&cover, RowBudget::new(2, 10));
        assert_eq!(
            shaped.text().split_whitespace().collect::<Vec<_>>(),
            cover.split_whitespace().collect::<Vec<_>>()
        );
        // The overlong word alone occupies five rows, so the floor exceeds two.
        assert!(shaped.rows >= 5);
        assert!(matches!(shaped.outcome, RowMatch::Taller { .. }));
    }

    #[test]
    fn rows_stay_evenly_filled_so_the_cover_reads_as_broken_chat_lines() {
        // The failure this guards against is a degenerate shape — one long row
        // plus a run of one-word rows — which reads as machine output even
        // though every word is unchanged. Splitting the widest row first keeps
        // the rows within a word or two of each other.
        let mut worst_spread = 0usize;
        for target in 2..=8 {
            let shaped = shape_cover(WORDS, RowBudget::new(target, 40));
            let per_row: Vec<usize> = shaped
                .text()
                .split('\n')
                .map(|row| row.split_whitespace().count())
                .collect();
            assert!(per_row.iter().all(|&count| count >= 1), "no blank rows");
            let spread = per_row.iter().max().unwrap() - per_row.iter().min().unwrap();
            println!("target {target}: rows={} per_row={per_row:?}", shaped.rows);
            worst_spread = worst_spread.max(spread);
        }
        println!("widest words-per-row spread across targets 2..=8: {worst_spread}");
        assert!(
            worst_spread <= 2,
            "rows drifted too far apart to read as chat: spread {worst_spread}"
        );
    }

    #[test]
    fn debug_output_never_carries_cover_text() {
        let shaped = shape_cover(WORDS, RowBudget::new(3, 30));
        let rendered = format!("{shaped:?}");
        for word in WORDS.split(' ') {
            assert!(
                !rendered.contains(word),
                "Debug must print counts, never cover words"
            );
        }
    }
}
