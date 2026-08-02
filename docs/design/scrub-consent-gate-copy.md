# Scrub account-termination consent copy

This document supplies the copy for Scrub’s rank-4, per-service consent gate.
It is a decision screen, not marketing copy. The application must show the
selected service’s version on the path to its first UI-automated run, with no
preselected choice. The user must type the acknowledgement exactly before the
run can continue.

## Discord

### Title

Discord can permanently terminate your account

### Warning

OSL would automate actions in your signed-in Discord account. Discord says:
“Automating normal user accounts (generally called \"self-bots\") outside of the
OAuth2/bot API is forbidden, and can result in an account termination if
found.” This may get your Discord account permanently terminated.

If Discord terminates your account, you can lose content that Scrub never
touched. This includes your messages, files, contacts, servers, and purchases.
A termination does not reverse deletion already done by Scrub.

The run can stop partway through because of a challenge, a rate limit, a sign
out, a service change, or another problem. The run can stop early. Some
selected items may remain. The job can be half-done. OSL will show what it
verified, what is unknown, and what remains. It will not continue on its own.

### Required acknowledgement

Type `I accept the Discord account termination risk` to continue.

### Source

Discord, “Automated User Accounts (Self-Bots),” fetched 2026-07-31. The quoted
provider language is preserved in
[`scrub-provider-routes-2026-07-31.md`](../legal/scrub-provider-routes-2026-07-31.md).

## Implementation requirements

- Show this warning separately for each service. Consent for Discord does not
  cover another service.
- Do not preselect, default, bury, or visually de-emphasize this warning.
- Require the typed acknowledgement before the user can start the run.
- Re-show the service warning when its provider posture or confirmed schema
  version changes. Let the user revoke consent; revocation stops in-flight and
  scheduled runs for that service.
- This consent accepts the method’s risk. It does not replace Scrub’s dry run,
  identity binding, batch confirmation, step-up, or one-shot deletion consent.

## Test vector

```json
{
  "id": "SCR-K1",
  "service": "Discord",
  "typedAcknowledgement": "I accept the Discord account termination risk",
  "warning": {
    "termination": "This may get your Discord account permanently terminated.",
    "untouchedContent": "A termination can take content that Scrub never touched. This includes your messages, files, and contacts.",
    "completedDeletes": "A termination does not reverse deletion already done by Scrub.",
    "partialRun": "The run can stop early. Some selected items may remain. The job can be half-done."
  },
  "providerEvidence": "Automating normal user accounts (generally called self-bots) outside of the OAuth2/bot API is forbidden, and can result in an account termination if found."
}
```
