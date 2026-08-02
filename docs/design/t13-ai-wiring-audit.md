# T13 AI wiring audit

This record distinguishes test and handoff artefacts from code that must be
reached by a shipping binary.  It was made while wiring the T13 follow-up, not
as a replacement for a feature-level reachability test.

## T13-A1

`crates/cover-ai/tests/coupling_is_deterministic.rs` is an integration test.
It is intentionally not imported by shipping code; Cargo discovers it as a
test target.  Its only purpose is to constrain `stego::encode_token`, so it
has no runtime call site to add.
