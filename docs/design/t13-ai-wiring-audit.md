# T13 AI wiring audit

This record distinguishes test and handoff artefacts from code that must be
reached by a shipping binary.  It was made while wiring the T13 follow-up, not
as a replacement for a feature-level reachability test.

## T13-A1

`crates/cover-ai/tests/coupling_is_deterministic.rs` is an integration test.
It is intentionally not imported by shipping code; Cargo discovers it as a
test target.  Its only purpose is to constrain `stego::encode_token`, so it
has no runtime call site to add.

## T13-A2

`crates/cover-ai/tests/entropy_budget.rs` is likewise a Cargo integration
test.  It measures the compiled deterministic codec and emits no runtime
component; adding a production import would be incorrect.

## T13-A5

`crates/cover-ai/src/fallback.rs` has no non-test consumer.  It cannot be
wired honestly yet: its plain-word-bank floor assumes Mode 1, while the
shipping IPC selector coerces Mode 1 to Mode 0.  Calling the table from that
Mode-0 path would report a word-bank carrier while sending `DPC0::` text.
That is a false safety signal, so this remains blocked on the T1 carrier
revival rather than receiving a decorative call site.

## T13-B1

`crates/cover-ai/tests/floor_survives_uninstall.rs` is a Cargo integration
test.  It deliberately compiles the codec and fallback table without an AI
runtime; it is not a production module or endpoint.

## T13-B4

`crates/cover-ai/tests/tier_is_not_observable.rs` is a regression test for
the intended shared codec output.  It has no shipping import by design.

## T13-B5

`crates/cover-ai/corpus/chat-en-expanded-v1.txt` and
`crates/cover-ai/tests/corpus_quality.rs` are a candidate corpus handoff for
DEP-T1-4.  T1 owns the compiled codec table, so importing this artefact into
the current binary would create a second carrier model and break decode
compatibility.  It remains a non-shipping handoff until T1 adopts it.

## T13-B2

The original `FREE_COVER` and 16-phrase code no longer exists:
`pro_context_cover.rs` was repurposed by T13-B3 into encrypted cover-history
state.  The planned B2 replacement also has no shippable route today because
the IPC send selector still forces Mode 1 to Mode 0.  Reintroducing a
word-bank call here would either be disconnected from the real carrier or
silently claim a Mode-1 guarantee that the actual send cannot provide.

## T13-C1

`CoverScorer` is exported from `cover-ai`, but no shipping package depends on
that crate and there is neither a candidate sampler (T13-C2) nor a concrete
scorer selected for the product.  The trait's contract is deliberately
selection-only; invoking it on the one already-chosen Mode-0 carrier cannot
select anything and would be a no-op.  It is therefore a deferred extension
point, not a legitimate shipping entry point, until C2 and the Mode-1 carrier
path exist.
