//! Regression guard for the capacity of the shipping bigram carrier.
//!
//! This is intentionally a standalone test until `cover-ai` becomes a Cargo
//! crate. Run it with:
//!
//! ```text
//! rustc --edition=2021 --test crates/cover-ai/tests/entropy_budget.rs -o /tmp/entropy_budget
//! /tmp/entropy_budget --nocapture
//! ```

#[path = "../../stego/src/bigram.rs"]
mod bigram;

const STATIONARY_ITERATIONS: usize = 256;

fn row_entropy(cumulative_counts: &[u32; bigram::VOCAB_SIZE]) -> f64 {
    let total = f64::from(cumulative_counts[bigram::VOCAB_SIZE - 1]);

    (1..bigram::VOCAB_SIZE)
        .map(|index| {
            let count = cumulative_counts[index] - cumulative_counts[index - 1];
            if count == 0 {
                return 0.0;
            }

            let probability = f64::from(count) / total;
            -probability * probability.log2()
        })
        .sum()
}

fn stationary_distribution() -> Vec<f64> {
    let model = bigram::model();
    let mut distribution = vec![1.0 / bigram::VOCAB_SIZE as f64; bigram::VOCAB_SIZE];

    for _ in 0..STATIONARY_ITERATIONS {
        let mut next = vec![0.0; bigram::VOCAB_SIZE];
        for (previous, cumulative_counts) in model.cum.iter().enumerate() {
            let total = f64::from(cumulative_counts[bigram::VOCAB_SIZE - 1]);
            for word in 1..bigram::VOCAB_SIZE {
                let count = cumulative_counts[word] - cumulative_counts[word - 1];
                next[word] += distribution[previous] * f64::from(count) / total;
            }
        }
        distribution = next;
    }

    distribution
}

#[test]
fn shipped_bigram_conditional_entropy_is_about_five_bits_per_word() {
    let model = bigram::model();
    let stationary = stationary_distribution();
    let entropy: f64 = stationary
        .iter()
        .zip(&model.cum)
        .map(|(probability, cumulative_counts)| probability * row_entropy(cumulative_counts))
        .sum();

    // This is the entropy rate of the actual smoothed transition model, not
    // the average of its rows. A material corpus or smoothing change must
    // re-measure the budget before changing the pointer width.
    assert!(
        (entropy - 5.05).abs() < 0.02,
        "shipped bigram conditional entropy changed: {entropy:.9} bits/word"
    );

    let expected_word_chars: f64 = stationary
        .iter()
        .enumerate()
        .map(|(word, probability)| probability * model.vocab[word].len() as f64)
        .sum();

    for payload_bits in [96.0, 128.0, 160.0, 192.0] {
        let words = payload_bits / entropy;
        let characters = words * expected_word_chars + (words - 1.0);
        println!(
            "{payload_bits:.0} bits: {words:.1} ideal words, {characters:.0} ideal characters"
        );
    }
}
