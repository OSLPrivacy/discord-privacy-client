# Telegram Adapter Verdict

Anchor: TelegramSupportVerdict

Date re-issued: 2026-08-01

## Verdict

Telegram Desktop remains `externally blocked` for the protected native adapter release gate.

This is not an `unsupported` product decision for Telegram as a service. The earlier probe reached
only a login surface, so it did not measure message-row accessibility and cannot establish that
the adapter is technically blocked. The public release label nevertheless remains externally
blocked: OSL cannot mark Telegram Desktop as `supported` until a signed-client bench proves stable,
text-exposed message rows through Windows UI Automation.

## Evidence

- The gate in `docs/design/osl-master-decision-2026-07-26.md` says stable Telegram remains
  externally blocked if it cannot expose reliable accessible message rows.
- The claim allowlist requires Signal, WhatsApp, Telegram and Outlook support claims to carry
  `Coming soon`, `Experimental`, or `Externally blocked` unless the relevant adapter evidence exists.
- `t2` added `TelegramA11yRowProbe` to
  `/home/<user>/osl-telegram-qa/infra/azure/telegram-qa/inspect-telegram-login-ui.ps1` at commit
  `0ee282e`. The probe emits only bounded row counts, control-type counts, timing, and a verdict:
  `supported`, `externally blocked`, or `unsupported`.
- That recorded run was on a login wall, not an actual conversation surface. It therefore supplies
  no candidate-row or text-exposure measurement and does not clear the promotion rule.
- Telegram Desktop has since shipped upstream message-list accessibility, but this is not a
  substitute for the required signed-client measurement. The re-measurement must use a current
  (at least 6.8.3), logged-in Telegram Desktop build and the UI Automation backend.

## Promotion Rule

Telegram Desktop may move to `supported` only after a logged-in, signed Telegram Desktop run of
`TelegramA11yRowProbe` returns `supported` on an actual conversation surface, with at least two
stable candidate rows and at least two text-exposed rows. A login wall or absent conversation
surface does not clear the gate.

Until that artifact exists, product, website and in-app copy must continue to treat Telegram
Desktop as `externally blocked` rather than available support.

## 2026-08-04 — the row measurement was taken; the block is NOT lifted

**Read this section as evidence, not as a claim. OSL does not claim Telegram Desktop support. The
verdict above stands unchanged and the versioned public row still reads `externally_blocked`.**

An earlier revision of this section was wrong in three ways and was corrected under D-206. What it
got wrong is recorded here rather than deleted, because the corrections are the useful part.

### The measurement

`TelegramA11yRowProbe` was re-bound to the owner's own signed, logged-in Telegram Desktop on an
actual conversation. The original in
`~/osl-telegram-qa/infra/azure/telegram-qa/inspect-telegram-login-ui.ps1:51` is hard-wired to the
Azure QA VM's `osltest` session, which is why its recorded run reached only a login wall. The control
types, geometry thresholds, `hasStableRows` predicate and verdict ladder are byte-verbatim.

Probe: `plan-test/probes/A03d-telegram-row-probe.ps1`.
Result: `plan-test/runlogs/tg-ungate-rowprobe.json`.

```
TelegramVersion      7.0.8.0          (rule requires >= 6.8.3)
SignatureStatus      Valid            CN=Telegram FZ-LLC
Phase                unrecognized     (not a login surface)
ConversationSurface  true             (a composer is present)
PaneElementCount     365              of 873 visible; 508 sit outside the conversation pane
Verdict field        supported
Exposure             stable-accessible-rows
CandidateRowCount    109              (rule requires >= 2)
TextExposedRowCount  109              (rule requires >= 2)
```

### Correction 1 — the first run counted the chat list, and 554 was never a verdict

The first revision reported **554 candidate rows / 553 text-exposed**. That scan took *every*
row-typed descendant of the window. Most of them are the **chat list** down the left-hand side — one
row per conversation — and the promotion rule names **message** rows. Counting the chat list and
reporting a verdict was measuring the wrong thing with the right code.

The probe now binds every candidate to the **conversation pane**, derived from the composer exactly
as the shipping adapter derives it (`native_telegram_adapter::telegram_transcript_candidate`): a row
must sit above the composer and share at least two thirds of its column. That is what produces the
109 above. The unbounded figure is still emitted, under
`UnboundedProbe_NotAVerdict` / `OutsidePaneProbe_NotAVerdict`, so the correction stays legible and
the old number can never again be quoted as a result.

The probe still starves: `-SelfTest` removes row-typed elements from the pane and nothing else, and
the same code returns `CandidateRowCount 0`, `no-accessible-rows`, `externally blocked`
(`plan-test/runlogs/tg-ungate-rowprobe-selftest.json`).

**Residual, stated rather than hidden:** the pane binding is geometric. It proves these rows are in
the transcript column above the composer; it does not read their text, deliberately, because that is
a real person's conversation.

### Correction 2 — "the premise is now false" was too strong

The earlier revision said the premise behind `externally blocked` "is now false". That overstated a
single run on a single client. What the run establishes is narrower and is all that is claimed here:
**on this client, at this version, the conversation pane exposes accessible rows, so the promotion
rule's necessary condition is satisfied.** It is one measurement, on one machine, by one prober. The
rule's own words are "may move to `supported` **only after**" — a necessary condition, not a
sufficient one — and nothing here makes Telegram Desktop supportable, because OSL's Telegram adapter
has no verb that can commit a message. The carry runs cover text into the live composer and back out
again, and stops there.

### Correction 3 — the label did NOT move, and the earlier claim that `beta` was permitted was false

The earlier revision said the in-app status had moved to `SupportLevel::Experimental` and that "the
claim allowlist already permits" it. **That was false.** `native_app_support_status` maps **both**
`Supported` and `Experimental` to `NativeAppSupportStatus::Beta` (`native_apps.rs`), so what would
have shipped is `beta` — a label `docs/design/osl-public-claim-allowlist.md:191` **forbids** for
Telegram, which must carry `Coming soon`, `Experimental` or `Externally blocked`, and which
lines 271-272 reserve for `runtime-proven` / `test-proven-only` rows.

**Nothing was ungated. `adapter_support` stays `SupportLevel::ComingSoon` and the TypeScript catalog
stays `comingSoon`.** `protectedMode` stays `unavailable`. The versioned public row stays
`externally_blocked`.

The real finding underneath is a product gap, not a labelling choice: **OSL cannot currently express
`Experimental`, the one non-blocked label the allowlist permits for Telegram.** `ExternallyBlocked`
exists and would agree with the matrix, but it is a *stronger* negative claim than this measurement
supports, so moving there would trade one wrong label for another. The label is held until a status
that renders as `Experimental` exists, and that decision is the owner's.

This is now enforced mechanically rather than by intention:
`native_apps::tests::rust_never_claims_more_than_the_public_support_matrix` fails if Rust sits in the
claim tier while `support-matrix.json` does not, and
`native_apps::tests::no_native_app_is_published_above_coming_soon_without_an_earned_live_receipt`
fails if any provider leaves `ComingSoon` without a live carry receipt bound by content hash to the
adapter that earned it.
