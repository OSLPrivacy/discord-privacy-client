# Scrub claim audit — shipping baseline

**Task:** T12-E6
**Audited:** 2026-07-31
**Verdict:** **CLEAN — no shipping Scrub deletion claim is made.**

This is a source audit of the shipping Hub UI, not evidence that a deletion
workflow works. It is the `SCRUB-CLAIM-BASELINE` for T11: re-audit as soon as a
Scrub implementation task adds a path that can change a third-party service.

## Evidence reviewed

| Surface | Current shipping wording / status | Allowlist result |
|---|---|---|
| Scrub settings | The UI says a user chooses a TXT, CSV, or JSON export; OSL suggests items and the user decides what to review. It also says the scan and review stay local. | Allowed for the current local-export review scope. It does not claim account discovery, provider coverage, completeness, or deletion. |
| Scrub safety disclosure | The UI says automatic deletion is unavailable, the build only gives manual directions, does not delete app messages, and the user must delete each message in the original app. | Matches the allowlist’s required current boundary: Scrub guided deletion is Planned and OSL does not erase anything for the user. |
| Review confirmation | The selected-item dialog says: “Nothing is deleted by this build” and that confirmation only prepares manual directions; it does not contact or change an app. | Allowed and is the controlling baseline sentence. |
| AutoScrub disclosure | The assistant is marked “Coming soon”; it says future actions must stop on limits, challenges, changed content, or failed checks, and removal remains unconfirmed until the app shows it is gone. | Allowed only as future/limitation copy. It makes no present-tense automatic-deletion claim. |
| Public support matrix | `scrub-discovery`, `scrub-guided-deletion`, and `autoscrub` are all `Planned`. The Discord connector’s `scrub` column is also `Planned`. | Consistent with the app: no public availability or service-support claim is earned. |

## Source anchors

- `apps/osl-hub-ui/src/main.ts:4976-4981` — current Scrub settings and manual-only safety disclosure.
- `apps/osl-hub-ui/src/main.ts:4982-4990` — AutoScrub is Coming soon; outcomes stay unconfirmed until the original app shows removal.
- `apps/osl-hub-ui/src/main.ts:5036-5045` — selected-list confirmation says nothing is deleted and does not contact or change an app.
- `docs/design/osl-public-claim-allowlist.md:164-166` — discovery, guided deletion, and AutoScrub are `Planned`; guided deletion must always say OSL does not erase anything for the user.
- `docs/status/support-matrix.json` — current matrix records Scrub capability rows as `Planned`.

## Claim-boundary decision

The shipping screen is compliant because its concrete promise is narrower than
the unearned capability: local review of a user-selected export plus manual
directions. “Your messages never leave this device” is scoped by the same
screen to the scan and review; it must not be widened to a claim about a
future hosted, IMAP, or UI-automation deletion path.

The following claims remain prohibited until the corresponding capability has
exact-build evidence and the allowlist/status matrix are updated:

- Any claim that Scrub discovers accounts, covers a complete history, or works
  with named providers.
- Any claim that OSL deletes, erases, removes, or verifies deletion of app or
  provider messages.
- Any claim that AutoScrub runs unattended or automatically deletes.
- Any deletion count that includes an `Unknown` result as deleted.

## Re-audit trigger for T11

The moment a Stage I, P, or H task makes any deletion path real, the sentence
“Nothing is deleted by this build” is false and must be changed in the same
wave. T11 must then update the public surface only to the exact capability
shown by the changed allowlist row and dated support-matrix evidence; it must
not infer provider-wide or automatic deletion from a partial path.

Owner decision D55 permits consent-gated, human-rate UI automation despite
provider-policy risk. It does not make that automation a current shipping
claim, nor does it relax the need for explicit per-service consent, account
termination warning, verification, or an `Unknown` outcome.
