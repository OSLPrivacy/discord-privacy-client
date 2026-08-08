//! Misspelling layer — the fourth and last layer over the fixed-width
//! substitution codec (task 0077).
//!
//! Deliberately last. The three layers before it are invisible to a stranger
//! reading the cover: a shorter handle (0074), a capital letter (0075) and a
//! wider vocabulary (0076) all still spell every word correctly. A
//! misspelling is the thing a stranger *notices*, and we have no way to
//! measure whether ours look natural — so it ships behind its own switch and
//! that switch is off until a human has looked at it. See
//! [`MisspellingChoice`], whose `Default` is [`MisspellingChoice::Off`]: a
//! fresh install with no saved preference reads as off.
//!
//! ## What the layer does
//!
//! It rides on the grown 256-word bank from task 0076 rather than replacing
//! it. Every bank word gets exactly one deterministic misspelling, so each
//! slot now has two written forms — correct and misspelled — and the choice
//! between them is one extra bit:
//!
//! * grown bank alone: 256 forms, `log2(256) = 8` bits/word,
//!   `ceil(80 / 8) = 10` words for the 80-bit handle+detector.
//! * grown bank + misspellings: 512 forms, `log2(512) = 9` bits/word,
//!   `ceil(80 / 9) = 9` words for the same payload.
//!
//! Turning the switch off emits the layer-3 cover byte for byte, so the word
//! count returns exactly and the same handle — and therefore the same private
//! message — reads back either way.
//!
//! ## How the misspellings are built
//!
//! One misspelling per bank word, generated once from an ordered list of
//! ordinary typo shapes (transpose the last two letters, double a letter,
//! drop a letter, …) — the first candidate that is not already taken by
//! another written form wins. Seeding the taken-set with all 256 correct
//! spellings guarantees the 512 forms are pairwise distinct, which is what
//! makes the parse direction unambiguous. `misspelled_forms_are_distinct`
//! below checks that property rather than assuming it.
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::word_bank_grown::{grown_bank_word, GROWN_BANK_SIZE};

/// Written forms addressable by one cover word: every grown-bank word in its
/// correct spelling plus its one misspelling.
pub const MISSPELL_FORMS: usize = GROWN_BANK_SIZE * 2;

/// Bits carried by one cover word with misspellings on: `log2(512)`.
pub const MISSPELL_WORD_BITS: usize = 9;

/// The user-facing switch for the misspelling layer.
///
/// `Off` is the `Default` on purpose: a misspelling is the one layer a
/// stranger can notice, and nothing measures whether ours read naturally, so
/// a fresh install (no preferences file, or a legacy file written before this
/// layer existed) must come up with the layer off.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MisspellingChoice {
    On,
    #[default]
    Off,
}

impl MisspellingChoice {
    /// The choice's on/off word, as shown and stored.
    pub fn label(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }

    /// Whether covers should be written with misspellings.
    pub fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    /// Parse a stored or typed on/off word.
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.trim().to_ascii_lowercase().as_str() {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            other => Err(format!("OSL: unknown misspelling choice '{other}'")),
        }
    }
}

/// Push `candidate` if it is a real change and not already proposed.
fn propose(out: &mut Vec<String>, word: &str, candidate: String) {
    if candidate != word && !out.iter().any(|c| c == &candidate) {
        out.push(candidate);
    }
}

/// Ordinary typo shapes for `word`, most typo-like first. Ascii only — every
/// grown-bank entry is lowercase ascii.
fn misspelling_candidates(word: &str) -> Vec<String> {
    let bytes = word.as_bytes();
    let n = bytes.len();
    let mut out = Vec::new();

    // Transposed last two letters — the "teh" typo.
    if n >= 2 {
        let mut v = bytes.to_vec();
        v.swap(n - 2, n - 1);
        propose(&mut out, word, String::from_utf8(v).expect("ascii"));
    }
    // Doubled final letter — "goo" for "go".
    {
        let mut s = word.to_string();
        s.push(bytes[n - 1] as char);
        propose(&mut out, word, s);
    }
    // Transposed second and third letters.
    if n >= 3 {
        let mut v = bytes.to_vec();
        v.swap(1, 2);
        propose(&mut out, word, String::from_utf8(v).expect("ascii"));
    }
    // Doubled first letter.
    {
        let mut s = String::with_capacity(n + 1);
        s.push(bytes[0] as char);
        s.push_str(word);
        propose(&mut out, word, s);
    }
    // Dropped final letter.
    if n >= 3 {
        propose(&mut out, word, word[..n - 1].to_string());
    }
    // Dropped second letter.
    if n >= 3 {
        let mut s = String::with_capacity(n - 1);
        s.push(bytes[0] as char);
        s.push_str(&word[2..]);
        propose(&mut out, word, s);
    }
    // Stray 'e' before the last letter.
    if n >= 2 {
        let mut s = word[..n - 1].to_string();
        s.push('e');
        s.push(bytes[n - 1] as char);
        propose(&mut out, word, s);
    }
    // Trailing 'e'.
    {
        let mut s = word.to_string();
        s.push('e');
        propose(&mut out, word, s);
    }
    out
}

/// One misspelling per bank slot, index-aligned with the grown bank.
static MISSPELLINGS: OnceLock<Vec<String>> = OnceLock::new();

fn misspellings() -> &'static Vec<String> {
    MISSPELLINGS.get_or_init(|| {
        // Every correct spelling is taken before the first misspelling is
        // chosen, so no misspelling can ever collide with a real bank word.
        let mut taken: HashSet<String> = (0..GROWN_BANK_SIZE)
            .map(|i| grown_bank_word(i).expect("bank index in range").to_string())
            .collect();
        let mut out = Vec::with_capacity(GROWN_BANK_SIZE);
        for index in 0..GROWN_BANK_SIZE {
            let word = grown_bank_word(index).expect("bank index in range");
            let chosen = misspelling_candidates(word)
                .into_iter()
                .find(|c| !taken.contains(c))
                .unwrap_or_else(|| {
                    // Deterministic last resort: lengthen with 'e' until free.
                    // Terminates because each step is strictly longer.
                    let mut s = word.to_string();
                    loop {
                        s.push('e');
                        if !taken.contains(&s) {
                            return s;
                        }
                    }
                });
            taken.insert(chosen.clone());
            out.push(chosen);
        }
        out
    })
}

/// `written form -> form value` over all [`MISSPELL_FORMS`] forms.
static FORM_INDEX: OnceLock<HashMap<String, usize>> = OnceLock::new();

fn form_index() -> &'static HashMap<String, usize> {
    FORM_INDEX.get_or_init(|| {
        let mut map = HashMap::with_capacity(MISSPELL_FORMS);
        for index in 0..GROWN_BANK_SIZE {
            map.insert(
                grown_bank_word(index)
                    .expect("bank index in range")
                    .to_string(),
                index,
            );
            map.insert(misspellings()[index].clone(), GROWN_BANK_SIZE + index);
        }
        map
    })
}

/// The misspelling of the bank word at `index`, or `None` when `index` is
/// outside the bank.
pub fn misspelled_word(index: usize) -> Option<&'static str> {
    misspellings().get(index).map(|s| s.as_str())
}

/// The written form for `form` (`0..GROWN_BANK_SIZE` correct,
/// `GROWN_BANK_SIZE..MISSPELL_FORMS` misspelled), or `None` if out of range.
pub fn form_word(form: usize) -> Option<&'static str> {
    if form < GROWN_BANK_SIZE {
        grown_bank_word(form)
    } else {
        misspelled_word(form - GROWN_BANK_SIZE)
    }
}

/// Whether `form` is one of the misspelled forms.
pub fn form_is_misspelled(form: usize) -> bool {
    (GROWN_BANK_SIZE..MISSPELL_FORMS).contains(&form)
}

/// How many of `forms` are written misspelled.
pub fn misspelled_count(forms: &[usize]) -> usize {
    forms
        .iter()
        .copied()
        .filter(|f| form_is_misspelled(*f))
        .count()
}

/// Word count a misspelling-on cover uses for `target_bits`: `ceil(bits / 9)`.
pub fn misspell_word_count(target_bits: u32) -> usize {
    (target_bits as usize).div_ceil(MISSPELL_WORD_BITS)
}

/// Bits -> form-value sequence, [`MISSPELL_WORD_BITS`] bits at a time. Same
/// shape as [`crate::word_bank_grown::grown_bank_decode_bits`], one bit wider.
pub fn misspell_decode_bits(bits: &[bool], target_bits: u32) -> Vec<usize> {
    bits.iter()
        .copied()
        .chain(std::iter::repeat(false))
        .take(target_bits as usize)
        .collect::<Vec<_>>()
        .chunks(MISSPELL_WORD_BITS)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0usize, |value, bit| (value << 1) | *bit as usize)
        })
        .collect()
}

/// Inverse of [`misspell_decode_bits`]. Returns an all-false payload of the
/// requested width on any structural mismatch (wrong word count, out-of-range
/// form), so a cover that is not one of ours simply fails the detector.
pub fn misspell_encode_words(forms: &[usize], target_bits: u32) -> Vec<bool> {
    if forms.len() != misspell_word_count(target_bits) {
        return vec![false; target_bits as usize];
    }
    let mut bits = Vec::with_capacity(target_bits as usize);
    for (index, &form) in forms.iter().enumerate() {
        if form >= MISSPELL_FORMS {
            return vec![false; target_bits as usize];
        }
        let remaining = target_bits as usize - index * MISSPELL_WORD_BITS;
        let width = remaining.min(MISSPELL_WORD_BITS);
        for shift in (0..width).rev() {
            bits.push((form >> shift) & 1 == 1);
        }
    }
    bits.truncate(target_bits as usize);
    if bits.len() == target_bits as usize {
        bits
    } else {
        vec![false; target_bits as usize]
    }
}

/// Render a form-value sequence as space-separated lowercase text with a
/// trailing period, matching the grown bank's rendering.
pub fn render_misspelled_forms(forms: &[usize]) -> String {
    let mut out = String::with_capacity(forms.len() * 7);
    let mut first = true;
    for &form in forms {
        let Some(word) = form_word(form) else {
            continue;
        };
        if !first {
            out.push(' ');
        }
        out.push_str(word);
        first = false;
    }
    if !out.is_empty() {
        out.push('.');
    }
    out
}

/// Parse a candidate string into form values. `None` if any token is neither
/// a bank word nor one of the misspellings — the cheap "this isn't a
/// misspelling-layer cover" rejection callers rely on.
pub fn parse_misspelled_forms(s: &str) -> Option<Vec<usize>> {
    let map = form_index();
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
        out.push(*map.get(t.as_str())?);
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_is_off_by_default() {
        assert_eq!(MisspellingChoice::default(), MisspellingChoice::Off);
        assert!(!MisspellingChoice::default().is_on());
        assert_eq!(MisspellingChoice::default().label(), "off");
    }

    #[test]
    fn form_count_matches_bit_width() {
        assert_eq!(MISSPELL_FORMS, GROWN_BANK_SIZE * 2);
        assert_eq!(1usize << MISSPELL_WORD_BITS, MISSPELL_FORMS);
    }

    #[test]
    fn misspelled_forms_are_distinct() {
        let mut seen = HashSet::new();
        for form in 0..MISSPELL_FORMS {
            let word = form_word(form).expect("every form renders");
            assert!(seen.insert(word), "duplicate written form {word:?}");
        }
        assert_eq!(seen.len(), MISSPELL_FORMS);
    }

    #[test]
    fn every_misspelling_differs_from_its_word() {
        for index in 0..GROWN_BANK_SIZE {
            let correct = grown_bank_word(index).expect("bank index in range");
            let wrong = misspelled_word(index).expect("bank index in range");
            assert_ne!(correct, wrong, "slot {index} was not misspelled");
            assert!(
                wrong.bytes().all(|b| b.is_ascii_lowercase()),
                "misspelling {wrong:?} is not lowercase ascii"
            );
        }
    }

    #[test]
    fn render_then_parse_round_trip() {
        let forms: Vec<usize> = (0..MISSPELL_FORMS).step_by(7).collect();
        let text = render_misspelled_forms(&forms);
        let parsed = parse_misspelled_forms(&text).expect("parse own output");
        assert_eq!(parsed, forms);
    }

    #[test]
    fn bits_round_trip_through_forms() {
        let target_bits = 80u32;
        let mut x: u64 = 0x0f1e_2d3c_4b5a_6978;
        let bits: Vec<bool> = (0..target_bits)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (x >> 63) & 1 == 1
            })
            .collect();
        let forms = misspell_decode_bits(&bits, target_bits);
        assert_eq!(forms.len(), misspell_word_count(target_bits));
        assert_eq!(misspell_encode_words(&forms, target_bits), bits);
    }

    #[test]
    fn misspelling_layer_needs_fewer_words_than_the_grown_bank() {
        let bits = 80u32;
        assert_eq!(misspell_word_count(bits), 9);
        assert_eq!(crate::word_bank_grown::grown_bank_word_count(bits), 10);
    }
}
