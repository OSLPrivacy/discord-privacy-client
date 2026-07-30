# Telegram Adapter Verdict

Anchor: TelegramSupportVerdict

Date recorded: 2026-07-30

## Verdict

Telegram Desktop remains `externally blocked` for the protected native adapter release gate.

This is not an `unsupported` product decision for Telegram as a service. It is the current adapter
verdict for the minimal-touch native Windows surface: OSL cannot mark Telegram Desktop as
`supported` until the signed-client bench proves stable, text-exposed message rows through Windows
UI Automation.

## Evidence

- The gate in `docs/design/osl-master-decision-2026-07-26.md` says stable Telegram remains
  externally blocked if it cannot expose reliable accessible message rows.
- The claim allowlist requires Signal, WhatsApp, Telegram and Outlook support claims to carry
  `Coming soon`, `Experimental`, or `Externally blocked` unless the relevant adapter evidence exists.
- `t2` added `TelegramA11yRowProbe` to
  `/home/<user>/osl-telegram-qa/infra/azure/telegram-qa/inspect-telegram-login-ui.ps1` at commit
  `0ee282e`. The probe emits only bounded row counts, control-type counts, timing, and a verdict:
  `supported`, `externally blocked`, or `unsupported`.

## Promotion Rule

Telegram Desktop may move to `supported` only after a logged-in, signed Telegram Desktop run of
`TelegramA11yRowProbe` returns `supported` on an actual conversation surface, with at least two
stable candidate rows and at least two text-exposed rows. A login wall or absent conversation
surface does not clear the gate.

Until that artifact exists, product, website and in-app copy must continue to treat Telegram
Desktop as `externally blocked` rather than available support.
