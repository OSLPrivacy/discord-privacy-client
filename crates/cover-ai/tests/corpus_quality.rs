//! Quality and capacity checks for the candidate corpus supplied to T1.
//!
//! Run without Cargo:
//! `rustc --edition=2021 --test crates/cover-ai/tests/corpus_quality.rs -o /tmp/corpus_quality && /tmp/corpus_quality --nocapture`

use std::collections::{HashMap, HashSet};

const CORPUS: &str = include_str!("../corpus/chat-en-expanded-v1.txt");
const VOCAB_SIZE: usize = 256;
const COUNT_SCALE: u32 = 32;

fn tokens(line: &str) -> impl Iterator<Item = &str> {
    line.split_ascii_whitespace().filter_map(|token| {
        let token = token.trim_matches(|character: char| character.is_ascii_punctuation());
        (!token.is_empty()).then_some(token)
    })
}

fn candidate_entropy() -> f64 {
    let mut frequencies = HashMap::<&str, u32>::new();
    for token in CORPUS.lines().flat_map(tokens) {
        *frequencies.entry(token).or_default() += 1;
    }
    let mut ranked: Vec<_> = frequencies.into_iter().collect();
    ranked.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(right.0)));
    let vocabulary: HashSet<_> = ranked
        .into_iter()
        .take(VOCAB_SIZE - 1)
        .map(|(word, _)| word)
        .collect();

    let mut counts = vec![vec![0u32; VOCAB_SIZE]; VOCAB_SIZE];
    let indices: HashMap<_, _> = vocabulary
        .iter()
        .enumerate()
        .map(|(index, word)| (*word, index + 1))
        .collect();
    for line in CORPUS.lines() {
        let mut previous = 0;
        for token in tokens(line) {
            if let Some(&index) = indices.get(token) {
                counts[previous][index] += 1;
                previous = index;
            } else {
                previous = 0;
            }
        }
    }

    let transition: Vec<Vec<f64>> = counts
        .into_iter()
        .map(|row| {
            let total: f64 = row
                .iter()
                .skip(1)
                .map(|count| f64::from(*count * COUNT_SCALE + 1))
                .sum();
            row.into_iter()
                .enumerate()
                .map(|(index, count)| {
                    if index == 0 {
                        0.0
                    } else {
                        f64::from(count * COUNT_SCALE + 1) / total
                    }
                })
                .collect()
        })
        .collect();
    let mut stationary = vec![1.0 / VOCAB_SIZE as f64; VOCAB_SIZE];
    for _ in 0..512 {
        let mut next = vec![0.0; VOCAB_SIZE];
        for (previous, row) in transition.iter().enumerate() {
            for (word, probability) in row.iter().enumerate() {
                next[word] += stationary[previous] * probability;
            }
        }
        stationary = next;
    }
    stationary
        .iter()
        .zip(&transition)
        .map(|(weight, row)| {
            let entropy: f64 = row
                .iter()
                .filter(|probability| **probability > 0.0)
                .map(|probability| -probability * probability.log2())
                .sum();
            weight * entropy
        })
        .sum()
}

#[test]
fn candidate_is_materially_wider_than_the_shipped_corpus() {
    let lines = CORPUS.lines().count();
    let unique: HashSet<_> = CORPUS.lines().flat_map(tokens).collect();
    assert!(
        lines >= 600,
        "candidate needs at least 600 chat lines, found {lines}"
    );
    assert!(
        CORPUS.len() >= 30_000,
        "candidate needs at least 30 KB, found {} bytes",
        CORPUS.len()
    );
    assert!(
        unique.len() >= 650,
        "candidate needs at least 650 distinct tokens, found {}",
        unique.len()
    );
}

#[test]
fn candidate_256_word_table_has_a_measured_capacity_budget() {
    let entropy = candidate_entropy();
    assert!(
        (6.0..7.8).contains(&entropy),
        "candidate entropy needs re-measurement: {entropy:.9} bits/word"
    );
    println!("candidate 256-word conditional entropy: {entropy:.9} bits/word");
    let mean_word_chars: f64 = CORPUS.lines().flat_map(tokens).map(str::len).sum::<usize>() as f64
        / CORPUS.lines().flat_map(tokens).count() as f64;
    for payload_bits in [96.0, 128.0, 160.0, 192.0] {
        let words = payload_bits / entropy;
        let characters = words * mean_word_chars + words - 1.0;
        println!(
            "{payload_bits:.0} bits: {words:.1} ideal words, {characters:.0} ideal characters"
        );
    }
}
