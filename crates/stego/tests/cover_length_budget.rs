//! Empirical cover-length budget for the pending pointer-width decision.
//!
//! This deliberately drives the current arithmetic cover codec at 96, 128,
//! and 160 payload bits without changing its shipping 96-bit wire format.
//! The printed table is the D2 input: the 96-bit column is the baseline.

use stego::{bigram, rendered_rows};

const WIDTHS: [u32; 3] = [96, 128, 160];
const SAMPLES: usize = 256;
/// A representative narrow Discord message column. `rendered_rows` uses the
/// same greedy word-wrap model as the carrier planner.
const COLUMN_GRAPHEMES: usize = 48;

#[derive(Clone, Copy, Debug, Default)]
struct Measurements {
    chars: usize,
    words: usize,
    rows: usize,
}

impl Measurements {
    fn add(&mut self, cover: &str) {
        self.chars += cover.chars().count();
        self.words += cover.split_ascii_whitespace().count();
        self.rows += rendered_rows(cover, COLUMN_GRAPHEMES);
    }

    fn average(self) -> Self {
        Self {
            chars: self.chars / SAMPLES,
            words: self.words / SAMPLES,
            rows: self.rows / SAMPLES,
        }
    }
}

fn sample_bits(width: u32, sample: usize) -> Vec<bool> {
    // Deterministic, varied inputs: cover length depends on the arithmetic
    // path, so a single all-zero pointer would not be a useful measurement.
    let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ ((width as u64) << 32) ^ sample as u64;
    (0..width)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state & 1 == 1
        })
        .collect()
}

#[test]
fn reports_cover_budget_for_candidate_pointer_widths() {
    let mut measured = Vec::with_capacity(WIDTHS.len());

    for width in WIDTHS {
        let mut totals = Measurements::default();
        for sample in 0..SAMPLES {
            let bits = sample_bits(width, sample);
            let words = bigram::arithmetic_decode_bits(&bits, width);
            assert!(
                !words.is_empty(),
                "{width}-bit sample {sample} produced an empty cover"
            );
            assert_eq!(
                bigram::arithmetic_encode_words(&words, width),
                bits,
                "{width}-bit sample {sample} did not round-trip"
            );
            totals.add(&bigram::render_words(&words));
        }
        measured.push((width, totals.average()));
    }

    let baseline = measured[0].1;
    println!("bits | avg chars | vs 96 | avg words | vs 96 | avg rows@{COLUMN_GRAPHEMES} | vs 96");
    for (width, stats) in &measured {
        println!(
            "{width:>4} | {:>9} | {:>5.2}x | {:>9} | {:>5.2}x | {:>18} | {:>5.2}x",
            stats.chars,
            stats.chars as f64 / baseline.chars as f64,
            stats.words,
            stats.words as f64 / baseline.words as f64,
            stats.rows,
            stats.rows as f64 / baseline.rows as f64,
        );
    }

    assert_eq!(
        measured.iter().map(|(width, _)| *width).collect::<Vec<_>>(),
        WIDTHS,
        "the D2 measurement must cover each candidate width"
    );
}
