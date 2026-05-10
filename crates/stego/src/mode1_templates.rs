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
    Template::new(
        &["Today", SLOT_TOKEN, "saw", "a", SLOT_TOKEN, "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Yesterday",
            "the",
            SLOT_TOKEN,
            "found",
            "a",
            SLOT_TOKEN,
            ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Maybe", "I", "should", "buy", "a", SLOT_TOKEN, SLOT_TOKEN, ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &["My", "neighbor", "owns", "a", SLOT_TOKEN, SLOT_TOKEN, "."],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "She", "said", "the", SLOT_TOKEN, "was", "near", "the", SLOT_TOKEN, ".",
        ],
        &[SlotKind::Noun, SlotKind::Noun],
    ),
    Template::new(
        &[
            "He", "asked", "if", "the", SLOT_TOKEN, "had", "any", SLOT_TOKEN, ".",
        ],
        &[SlotKind::Noun, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Apparently",
            SLOT_TOKEN,
            "is",
            "quite",
            SLOT_TOKEN,
            "today",
            ".",
        ],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    Template::new(
        &[
            "Honestly", "the", SLOT_TOKEN, "looked", "more", SLOT_TOKEN, "than", "usual", ".",
        ],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    Template::new(
        &[
            "Nobody", "noticed", "how", SLOT_TOKEN, "the", SLOT_TOKEN, "had", "become", ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Everybody",
            "wondered",
            "where",
            "the",
            SLOT_TOKEN,
            SLOT_TOKEN,
            "went",
            ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Last", "week", "we", "rented", "a", SLOT_TOKEN, SLOT_TOKEN, ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Every", "morning", "the", SLOT_TOKEN, "feels", SLOT_TOKEN, ".",
        ],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    Template::new(
        &[
            "Sometimes",
            "I",
            "miss",
            "that",
            SLOT_TOKEN,
            SLOT_TOKEN,
            ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Without", "warning", "the", SLOT_TOKEN, "turned", SLOT_TOKEN, ".",
        ],
        &[SlotKind::Noun, SlotKind::Adj],
    ),
    Template::new(
        &[
            "Across", "the", "street", "a", SLOT_TOKEN, SLOT_TOKEN, "appeared", ".",
        ],
        &[SlotKind::Adj, SlotKind::Noun],
    ),
    Template::new(
        &[
            "Until", "tomorrow", "leave", "the", SLOT_TOKEN, "near", "the", SLOT_TOKEN, ".",
        ],
        &[SlotKind::Noun, SlotKind::Noun],
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
