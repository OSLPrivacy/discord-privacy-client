//! Local-only helpers for optional AI-assisted carrier selection.

pub mod candidates;
pub mod capability_probe;
pub mod cold_start;
pub mod context;
// T13-C5 added `cover_history.rs` without this line, so for its first four days
// the module was not part of the crate at all: the only thing that compiled it
// was a `#[path]` recompile in `tests/cover_history.rs`, which built a private
// second copy inside that one test binary. That is why a reachability sweep read
// it as "a complete second cover-history implementation used only by its own
// test" (D-252) -- it was dead because it had never been declared, not because
// `apps/osl-hub/src/pro_context_cover.rs` replaced it. The two are not rivals:
// the hub owns the shipping Discord-overlay store, and this is the crate-side
// T13-C5 store that T13-C6 (`cold_start`) is specified to draw on.
pub mod cover_history;
pub mod fallback;
// Gated with its runtime: this module is the only consumer of llama-cpp-2, and
// compiling it without the feature would fail on the missing crate rather than
// on anything the caller did.
#[cfg(feature = "local-model")]
pub mod local_model;
pub mod pool;
pub mod pool_at_rest;
pub mod progress;
pub mod scorer;
pub mod warm_model;

// Gated with its only consumer, `local_model`. Selecting from raw llama.cpp
// logits has no meaning without the runtime that produces them, so without the
// feature this module is dead code rather than something worth compiling.
// `tests/local_model_logits.rs` includes the file directly via `#[path]`, so it
// keeps covering this logic on a default (feature-off) `cargo test`.
#[cfg(feature = "local-model")]
mod logit_selection;
