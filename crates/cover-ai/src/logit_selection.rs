//! Selection from an unmodified full-vocabulary logit distribution.

/// A token chosen directly from the raw vocabulary logits returned by llama.cpp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LogitSelection {
    pub(crate) token_index: usize,
    pub(crate) logit: f32,
}

/// Chooses the highest finite logit without applying a sampler or truncation.
///
/// `LlamaContext::get_logits()` returns one logit per vocabulary entry. Keeping
/// this operation small and separate makes it auditable that the adapter does
/// not accidentally introduce top-k, top-p, temperature, or grammar filtering.
pub(crate) fn select_from_full_logits(logits: &[f32]) -> Option<LogitSelection> {
    logits
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, logit)| logit.is_finite())
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(token_index, logit)| LogitSelection { token_index, logit })
}
