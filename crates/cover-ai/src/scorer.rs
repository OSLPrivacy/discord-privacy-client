//! Selection-only interface for already-rendered carrier covers.
//!
//! A scorer may inspect rendered text and rank it, but it never receives a
//! mutable cover or a carrier encoder. Rendering remains outside the AI path.

/// Carrier text produced by the canonical renderer before AI selection.
///
/// The bytes are private and immutable after construction so a scorer can
/// inspect a candidate without changing the payload-bearing cover.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedCover {
    text: String,
}

impl RenderedCover {
    /// Wrap text that has already been rendered by the carrier codec.
    pub fn from_canonical_text(text: String) -> Self {
        Self { text }
    }

    /// The exact carrier text available for read-only scoring.
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// A scorer's assessment of one candidate in the supplied slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverScore {
    pub candidate_index: usize,
    pub score: i32,
}

/// Ranks already-rendered carrier covers.
///
/// This trait intentionally has no generation, transport, persistence, or
/// context-retrieval capability. Its only input is an immutable slice of
/// rendered candidates, and its only output is candidate indices with scores.
pub trait CoverScorer {
    fn score(&self, covers: &[RenderedCover]) -> Vec<CoverScore>;
}
