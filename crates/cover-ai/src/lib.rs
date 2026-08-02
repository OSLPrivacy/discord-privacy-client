//! Local-only helpers for optional AI-assisted carrier selection.

pub mod context;
pub mod cold_start;
// Gated with its runtime: this module is the only consumer of llama-cpp-2, and
// compiling it without the feature would fail on the missing crate rather than
// on anything the caller did.
#[cfg(feature = "local-model")]
pub mod local_model;
pub mod scorer;

mod logit_selection;
