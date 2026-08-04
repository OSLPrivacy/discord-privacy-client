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

## 2026-08-04 — the artifact now exists

The measurement above was owed and has been taken. It was run against the **owner's own signed,
logged-in Telegram Desktop, on an actual conversation**, with the same `TelegramA11yRowProbe` — the
same control types, the same geometry thresholds, the same `hasStableRows` predicate and the same
verdict ladder. Only the host binding changed, because the original is hard-wired to the Azure QA
VM's `osltest` session, which is precisely why its recorded run measured a login wall.

Probe: `plan-test/probes/A03d-telegram-row-probe.ps1`.
Result: `plan-test/runlogs/tg-ungate-rowprobe.json`.

```
TelegramVersion   7.0.8.0            (rule requires >= 6.8.3)
SignatureStatus   Valid              CN=Telegram FZ-LLC
Phase             unrecognized       (not a login surface)
ConversationSurface true             (a composer is present)
Verdict           supported
Exposure          stable-accessible-rows
CandidateRowCount   554              (rule requires >= 2)
TextExposedRowCount 553              (rule requires >= 2)
```

The probe is not insensitive: run with `-SelfTest`, which starves it of row-typed elements and
changes nothing else, the same code returns `CandidateRowCount 0`, `no-accessible-rows`,
`externally blocked` (`plan-test/runlogs/tg-ungate-rowprobe-selftest.json`).

**What this settles.** The necessary condition named in the promotion rule is met, and the factual
premise behind the `externally blocked` label is now false: Telegram Desktop *does* expose stable,
text-exposed message rows through UI Automation. The label was always the honest reading of an
absent measurement — the verdict says so itself — and the measurement is no longer absent.

**What this does not settle, and why the public row has not been moved here.** `externally blocked`
and `supported` are not the only two answers. `supported` would claim protected Telegram
*messaging*, and OSL's Telegram adapter deliberately has no verb that can commit a message: the
carry is proven from cover text into the live composer and back out again, and stops there. Moving
the versioned public row is therefore an owner-facing decision about which non-blocked label
applies, not a mechanical consequence of this run, and it is left to the conductor with the
artifacts above.

**What did move**: the in-app native-app support status, `SupportLevel::ComingSoon` →
`SupportLevel::Experimental` (public `beta`), which the claim allowlist already permits for Telegram
without adapter evidence and which is now additionally backed by it. `protectedMode` stays
`unavailable`.
