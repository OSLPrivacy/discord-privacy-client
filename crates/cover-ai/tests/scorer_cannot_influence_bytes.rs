//! Compile and runtime guard for the selection-only scorer boundary.
//!
//! Run with:
//!
//! ```text
//! rustc --edition=2021 --test crates/cover-ai/tests/scorer_cannot_influence_bytes.rs -o /tmp/scorer_cannot_influence_bytes
//! /tmp/scorer_cannot_influence_bytes --nocapture
//! ```

#[path = "../src/scorer.rs"]
mod scorer;

use scorer::{CoverScore, CoverScorer, RenderedCover};

struct LengthScorer;

impl CoverScorer for LengthScorer {
    fn score(&self, covers: &[RenderedCover]) -> Vec<CoverScore> {
        covers
            .iter()
            .enumerate()
            .map(|(candidate_index, cover)| CoverScore {
                candidate_index,
                score: cover.as_str().len() as i32,
            })
            .collect()
    }
}

#[test]
fn scorer_only_observes_rendered_bytes_and_returns_indices_and_scores() {
    let covers = [
        RenderedCover::from_canonical_text("meet me after lunch".to_owned()),
        RenderedCover::from_canonical_text("the train was late again".to_owned()),
    ];
    let before: Vec<String> = covers
        .iter()
        .map(|cover| cover.as_str().to_owned())
        .collect();

    let scores = LengthScorer.score(&covers);

    assert_eq!(
        scores,
        [
            CoverScore {
                candidate_index: 0,
                score: 19,
            },
            CoverScore {
                candidate_index: 1,
                score: 24,
            },
        ]
    );
    assert_eq!(
        covers
            .iter()
            .map(|cover| cover.as_str())
            .collect::<Vec<_>>(),
        before.iter().map(String::as_str).collect::<Vec<_>>(),
        "scoring must leave the canonical carrier bytes unchanged"
    );
}
