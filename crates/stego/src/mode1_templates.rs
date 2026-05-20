//! Static template pool for Mode 1 stego.
//!
//! Each template is a tuple `(skeleton_tokens, slot_kinds)`:
//! - `skeleton_tokens` is the list of fixed words (and trailing
//!   punctuation) interspersed with `SLOT_TOKEN` (`"\x00"`) markers
//!   for slot positions. Reusing a sentinel token rather than
//!   regex-matching keeps the parser trivial.
//! - `slot_kinds` lists the wordlist used for each slot in order.
//!
//! Decoder strategy: tokenise the input sentence into whitespace-
//! separated tokens, strip the trailing punctuation from the last
//! token, and pattern-match against each template's skeleton. The
//! first template whose fixed tokens line up wins, with the
//! intervening tokens read off as slot values.
//!
//! Constraints (enforced by tests in [`crate::mode1::tests`]):
//! - Exactly [`TEMPLATES_LEN`] (16) templates → 4 bits per
//!   template choice.
//! - Each template has the same total slot count (`TOTAL_SLOTS`)
//!   so the bit budget per sentence is uniform. This keeps encode
//!   /decode bit accounting simple.
//! - Skeleton tokens are unique per template position (no two
//!   templates have the same fixed tokens at the same positions
//!   when slot kinds differ — pattern match would be ambiguous).
//! - No skeleton token may collide with any wordlist entry.
//!
//! Phase 2 (prose-token pivot): templates rewritten to read like
//! short Discord chat ("lol such a cute dog .", "honestly work is
//! rough .") instead of the prior story-narrative phrasing. Same
//! 16-template / 2-slot / 20-bit-per-sentence budget; only the
//! surface text changes.

use crate::mode1_wordlists::{ADJECTIVES, NOUNS};

/// Slot fill kind. Tags which wordlist to draw the slot value from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Noun,
    Adj,
}

impl SlotKind {
    pub fn wordlist(&self) -> &'static [&'static str] {
        match self {
            SlotKind::Noun => &NOUNS,
            SlotKind::Adj => &ADJECTIVES,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SlotKind::Noun => "noun",
            SlotKind::Adj => "adj",
        }
    }
}

/// Sentinel token used in `skeleton_tokens` to mark slot positions.
/// Choosing `"\x00"` guarantees no collision with any English word
/// or punctuation.
pub const SLOT_TOKEN: &str = "\x00";

/// Static template pool. Each entry is
/// `(skeleton_tokens, slot_kinds)`. The encoder picks one of these
/// 16 by reading 4 bits from the bit stream; the decoder identifies
/// the template by skeleton match.
///
/// Each template has 2 slots → bit budget = 4 (template) + 16 (two
/// 8-bit slot fills) = **20 bits per sentence**.
pub static TEMPLATES: [Template; TEMPLATES_LEN] = [
    // 0: [Adj, Noun]   "lol such a cute dog ."
    Template::new(
        &["lol", "such", "a", SLOT_TOKEN, SLOT_TOKEN, "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    // 1: [Adj, Noun]   "ngl that's a wild idea ."
    Template::new(
        &["ngl", "that's", "a", SLOT_TOKEN, SLOT_TOKEN, "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    // 2: [Adj, Noun]   "wow what a busy day today ."
    Template::new(
        &["wow", "what", "a", SLOT_TOKEN, SLOT_TOKEN, "today", "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    // 3: [Adj, Noun]   "i need a long break rn ."
    Template::new(
        &["i", "need", "a", SLOT_TOKEN, SLOT_TOKEN, "rn", "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    // 4: [Noun, Adj]   "lol work is so busy ."
    Template::new(
        &["lol", SLOT_TOKEN, "is", "so", SLOT_TOKEN, "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 5: [Noun, Adj]   "honestly homework is rough ."
    Template::new(
        &["honestly", SLOT_TOKEN, "is", SLOT_TOKEN, "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 6: [Noun, Adj]   "btw mom looks tired today ."
    Template::new(
        &["btw", SLOT_TOKEN, "looks", SLOT_TOKEN, "today", "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 7: [Noun, Adj]   "this song is super good ."
    Template::new(
        &["this", SLOT_TOKEN, "is", "super", SLOT_TOKEN, "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 8: [Noun, Adj]   "my brother has been weird lately ."
    Template::new(
        &["my", SLOT_TOKEN, "has", "been", SLOT_TOKEN, "lately", "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 9: [Noun, Adj]   "wait was the movie good ?"
    Template::new(
        &["wait", "was", "the", SLOT_TOKEN, SLOT_TOKEN, "?"],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 10: [Noun, Adj]  "ok so dinner is ready now ."
    Template::new(
        &["ok", "so", SLOT_TOKEN, "is", SLOT_TOKEN, "now", "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 11: [Noun, Adj]  "ngl work feels rough tho ."
    Template::new(
        &["ngl", SLOT_TOKEN, "feels", SLOT_TOKEN, "tho", "."],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    // 12: [Noun, Noun] "from work to gym today ."
    Template::new(
        &["from", SLOT_TOKEN, "to", SLOT_TOKEN, "today", "."],
        &[SlotKind::Noun, SlotKind::Noun],
    ),
    // 13: [Noun, Noun] "between work and school again ."
    Template::new(
        &["between", SLOT_TOKEN, "and", SLOT_TOKEN, "again", "."],
        &[SlotKind::Noun, SlotKind::Noun],
    ),
    // 14: [Adj, Adj]   "feeling tired and hungry tonight ."
    Template::new(
        &["feeling", SLOT_TOKEN, "and", SLOT_TOKEN, "tonight", "."],
        &[SlotKind::Adj, SlotKind::Adj],
    ),
    // 15: [Adj, Adj]   "kinda tired but also hungry ."
    Template::new(
        &["kinda", SLOT_TOKEN, "but", "also", SLOT_TOKEN, "."],
        &[SlotKind::Adj, SlotKind::Adj],
    ),
];

pub const TEMPLATES_LEN: usize = 16;
pub const TOTAL_SLOTS: usize = 2;
pub const TEMPLATE_BITS: u32 = 4;
pub const SLOT_BITS: u32 = 8;
/// Bits encoded per emitted sentence: 4 template + 2 × 8 slots = 20.
pub const BITS_PER_SENTENCE: u32 = TEMPLATE_BITS + (TOTAL_SLOTS as u32) * SLOT_BITS;

#[derive(Clone, Copy, Debug)]
pub struct Template {
    pub skeleton: &'static [&'static str],
    pub slots: &'static [SlotKind],
}

impl Template {
    pub const fn new(skeleton: &'static [&'static str], slots: &'static [SlotKind]) -> Self {
        Template { skeleton, slots }
    }
}
