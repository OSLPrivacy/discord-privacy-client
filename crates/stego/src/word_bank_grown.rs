//! Grown word bank — third layer over the fixed-width substitution codec
//! (task 0076).
//!
//! Tasks 0074/0075 shrank the cover handle from a 20-byte seed to an 8-byte
//! shared-key handle and then added capitalisation as a second, independent
//! bit riding on the same 64-word bank. This layer takes the direct route
//! instead of stacking another bit on top: it is a wider word bank on its
//! own, built from a dedicated 256-word list rather than the bigram model's
//! vocabulary, so growing it never perturbs [`crate::bigram`]'s trained
//! probabilities (and therefore never changes the arithmetic-coded token
//! path's cover length or the entropy any other test measures).
//!
//! 256 is a clean power of two: `log2(256) = 8` bits per word, against the
//! base 64-word bank's 6 bits per word. More words to choose from means more
//! payload carried per word, so the same fixed payload needs fewer words.
//! It is a switch — callers choose the plain 64-word form or this 256-word
//! form; turning it off restores the original word count exactly.
use std::sync::OnceLock;

/// Word count of the grown bank. A clean power of two so every chunk carries
/// a whole number of bits with no wasted fractional slot.
pub const GROWN_BANK_SIZE: usize = 256;

/// Bits carried by one grown-bank word: `log2(GROWN_BANK_SIZE)`.
pub const GROWN_BANK_WORD_BITS: usize = 8;

/// The grown word bank: ordinary-conversation vocabulary, one entry per
/// index `0..GROWN_BANK_SIZE`. Kept disjoint in *purpose* from the bigram
/// corpus — this list is never fed to the language model, so growing it
/// carries zero risk to the arithmetic codec's measured entropy.
static GROWN_BANK: [&str; GROWN_BANK_SIZE] = [
    "i",
    "you",
    "he",
    "she",
    "we",
    "they",
    "it",
    "me",
    "him",
    "her",
    "us",
    "them",
    "my",
    "your",
    "his",
    "its",
    "our",
    "their",
    "this",
    "that",
    "these",
    "those",
    "here",
    "there",
    "now",
    "then",
    "today",
    "tomorrow",
    "yesterday",
    "tonight",
    "morning",
    "afternoon",
    "evening",
    "night",
    "week",
    "weekend",
    "month",
    "year",
    "hour",
    "minute",
    "second",
    "good",
    "bad",
    "great",
    "nice",
    "fine",
    "okay",
    "sure",
    "right",
    "wrong",
    "true",
    "false",
    "real",
    "fake",
    "happy",
    "sad",
    "tired",
    "excited",
    "bored",
    "angry",
    "calm",
    "relaxed",
    "stressed",
    "worried",
    "nervous",
    "love",
    "like",
    "hate",
    "want",
    "need",
    "miss",
    "hope",
    "wish",
    "plan",
    "think",
    "know",
    "feel",
    "believe",
    "say",
    "tell",
    "ask",
    "answer",
    "talk",
    "chat",
    "speak",
    "listen",
    "hear",
    "see",
    "look",
    "watch",
    "read",
    "write",
    "go",
    "come",
    "stay",
    "leave",
    "arrive",
    "return",
    "walk",
    "run",
    "drive",
    "ride",
    "fly",
    "travel",
    "move",
    "eat",
    "drink",
    "cook",
    "bake",
    "order",
    "shop",
    "buy",
    "sell",
    "pay",
    "spend",
    "save",
    "earn",
    "borrow",
    "lend",
    "work",
    "study",
    "learn",
    "teach",
    "practice",
    "train",
    "build",
    "fix",
    "break",
    "clean",
    "wash",
    "play",
    "game",
    "sport",
    "team",
    "match",
    "win",
    "lose",
    "score",
    "goal",
    "point",
    "rule",
    "field",
    "court",
    "music",
    "song",
    "album",
    "band",
    "concert",
    "show",
    "movie",
    "film",
    "series",
    "episode",
    "story",
    "book",
    "page",
    "chapter",
    "novel",
    "poem",
    "letter",
    "note",
    "message",
    "email",
    "call",
    "text",
    "friend",
    "family",
    "brother",
    "sister",
    "mother",
    "father",
    "parent",
    "child",
    "kid",
    "baby",
    "neighbor",
    "home",
    "house",
    "room",
    "kitchen",
    "bedroom",
    "bathroom",
    "garden",
    "yard",
    "street",
    "city",
    "town",
    "car",
    "bus",
    "plane",
    "bike",
    "boat",
    "road",
    "path",
    "bridge",
    "station",
    "airport",
    "port",
    "phone",
    "computer",
    "laptop",
    "tablet",
    "screen",
    "camera",
    "photo",
    "video",
    "internet",
    "website",
    "weather",
    "rain",
    "snow",
    "sun",
    "cloud",
    "wind",
    "storm",
    "heat",
    "cold",
    "warm",
    "cool",
    "mild",
    "humid",
    "color",
    "red",
    "blue",
    "green",
    "yellow",
    "black",
    "white",
    "gray",
    "purple",
    "orange",
    "pink",
    "brown",
    "food",
    "breakfast",
    "lunch",
    "dinner",
    "snack",
    "meal",
    "recipe",
    "restaurant",
    "cafe",
    "bakery",
    "water",
    "coffee",
    "tea",
    "juice",
    "soda",
    "milk",
    "bread",
    "rice",
    "pasta",
    "salad",
    "soup",
    "fruit",
    "door",
];

/// The correctly spelled grown-bank word at `index`.
///
/// Exposed for the task-0077 misspelling layer, which builds one distinct
/// alternate spelling for every grown-bank slot without duplicating this
/// bank or letting the two lists drift apart.
pub fn grown_bank_word(index: usize) -> Option<&'static str> {
    GROWN_BANK.get(index).copied()
}

/// `word -> index` lookup, built once from [`GROWN_BANK`].
static INDEX_OF: OnceLock<std::collections::HashMap<&'static str, usize>> = OnceLock::new();

fn index_of() -> &'static std::collections::HashMap<&'static str, usize> {
    INDEX_OF.get_or_init(|| {
        GROWN_BANK
            .iter()
            .enumerate()
            .map(|(i, w)| (*w, i))
            .collect()
    })
}

/// Word count a grown-bank cover uses for `target_bits`: `ceil(bits / 8)`.
pub fn grown_bank_word_count(target_bits: u32) -> usize {
    (target_bits as usize).div_ceil(GROWN_BANK_WORD_BITS)
}

/// Bits -> word-index sequence, `GROWN_BANK_WORD_BITS` bits at a time.
/// Mirrors [`crate::bigram::legacy_wide_decode_bits`] but with an 8-bit
/// chunk indexing the full 256-word bank directly (no `+1` BOS offset — this
/// list has no reserved sentinel slot).
pub fn grown_bank_decode_bits(bits: &[bool], target_bits: u32) -> Vec<usize> {
    bits.iter()
        .copied()
        .chain(std::iter::repeat(false))
        .take(target_bits as usize)
        .collect::<Vec<_>>()
        .chunks(GROWN_BANK_WORD_BITS)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0usize, |value, bit| (value << 1) | *bit as usize)
        })
        .collect()
}

/// Inverse of [`grown_bank_decode_bits`]. Returns an all-false payload of
/// the requested width on any structural mismatch (wrong word count,
/// out-of-bank index), exactly as the plain wide inverse does, so a bad
/// cover simply fails the detector rather than panicking.
pub fn grown_bank_encode_words(words: &[usize], target_bits: u32) -> Vec<bool> {
    if words.len() != grown_bank_word_count(target_bits) {
        return vec![false; target_bits as usize];
    }
    let mut bits = Vec::with_capacity(target_bits as usize);
    for (index, &word) in words.iter().enumerate() {
        if word >= GROWN_BANK_SIZE {
            return vec![false; target_bits as usize];
        }
        let remaining = target_bits as usize - index * GROWN_BANK_WORD_BITS;
        let width = remaining.min(GROWN_BANK_WORD_BITS);
        for shift in (0..width).rev() {
            bits.push((word >> shift) & 1 == 1);
        }
    }
    bits.truncate(target_bits as usize);
    if bits.len() == target_bits as usize {
        bits
    } else {
        vec![false; target_bits as usize]
    }
}

/// Render a grown-bank word-index sequence as space-separated lowercase text
/// with a trailing period.
pub fn render_grown_words(words: &[usize]) -> String {
    let mut out = String::with_capacity(words.len() * 6);
    let mut first = true;
    for &w in words {
        if w >= GROWN_BANK_SIZE {
            continue;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(GROWN_BANK[w]);
        first = false;
    }
    if !out.is_empty() {
        out.push('.');
    }
    out
}

/// Parse a candidate string into grown-bank indices. Returns `None` if any
/// token isn't in the bank — the cheap "this isn't a grown-bank cover"
/// rejection callers rely on.
pub fn parse_grown_words(s: &str) -> Option<Vec<usize>> {
    let map = index_of();
    let mut out = Vec::new();
    for tok in s.split_ascii_whitespace() {
        let mut t = tok.to_ascii_lowercase();
        while let Some(c) = t.chars().last() {
            if matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '"' | ')' | ']') {
                t.pop();
            } else {
                break;
            }
        }
        while let Some(c) = t.chars().next() {
            if matches!(c, '"' | '(' | '[') {
                t.remove(0);
            } else {
                break;
            }
        }
        if t.is_empty() {
            continue;
        }
        let idx = *map.get(t.as_str())?;
        out.push(idx);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn grown_bank_size_matches_bit_width() {
        assert_eq!(GROWN_BANK.len(), GROWN_BANK_SIZE);
        assert_eq!(1usize << GROWN_BANK_WORD_BITS, GROWN_BANK_SIZE);
    }

    #[test]
    fn grown_bank_no_duplicate_entries() {
        let set: HashSet<&str> = GROWN_BANK.iter().copied().collect();
        assert_eq!(
            set.len(),
            GROWN_BANK.len(),
            "GROWN_BANK contains a duplicate word"
        );
    }

    #[test]
    fn render_then_parse_round_trip() {
        let words: Vec<usize> = (0..GROWN_BANK_SIZE).step_by(3).collect();
        let s = render_grown_words(&words);
        let parsed = parse_grown_words(&s).expect("parse own output");
        assert_eq!(parsed, words);
    }

    #[test]
    fn bits_round_trip_through_words() {
        let target_bits = 80u32;
        let mut x: u64 = 0x1234_5678_9abc_def0;
        let bits: Vec<bool> = (0..target_bits)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (x >> 63) & 1 == 1
            })
            .collect();
        let words = grown_bank_decode_bits(&bits, target_bits);
        assert_eq!(words.len(), grown_bank_word_count(target_bits));
        let back = grown_bank_encode_words(&words, target_bits);
        assert_eq!(back, bits);
    }
}
