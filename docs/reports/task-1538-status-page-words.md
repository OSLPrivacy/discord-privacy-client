# TASK 1538 status page words audit

Audited: 2026-08-06

Scope: `data/pricing.json` connector-matrix status words, checked against the
current service evidence in `docs/status/support-matrix.json` and the current
Scrub boundary in `docs/reports/scrub-claim-audit.md`.

Extractor result:

```text
TASK1538_AUDIT claim_count=25 listed=25 supported=22 unsupported=3 unmarked=0
```

## Evidence Sources

- Status claims: `data/pricing.json:1682-1864`.
- Current service evidence: `docs/status/support-matrix.json:16-360`.
- Discord current capability: `docs/status/support-matrix.json:207-275` says
  Discord is `unavailable`, with `protected_send`, `protected_receive`,
  `attachments`, and `burn` all `unavailable`; D6 and W9 block runtime,
  verified-live, release-qualified, or available Discord claims.
- Signal and WhatsApp current capability:
  `docs/status/support-matrix.json:84-119` keeps both rows `coming_soon` with
  no claim allowed, and `docs/status/support-matrix.json:175-205` records only
  signed data-only profile evidence.
- Telegram current capability: `docs/status/support-matrix.json:121-153` and
  `docs/status/support-matrix.json:326-352` keep Telegram
  `externally_blocked`; the OSL adapter has no live delivery proof.
- Outlook / OSL Mail current capability:
  `docs/status/support-matrix.json:155-171` and
  `docs/status/support-matrix.json:354-360` keep the OSL Mail gate
  unsupported until mailbox binding, recipient authority, safe send behavior,
  draft handling, and runtime proof exist.
- Scrub stage evidence: `data/pricing.json:1504-1518` keeps
  `scrub-discovery`, `scrub-guided-deletion`, and `autoscrub` at `Planned`;
  `docs/reports/scrub-claim-audit.md:1-27` says no shipping Scrub deletion
  claim is made and the public matrix rows are `Planned`.

## Claim List

| # | Claim | Status word | Verdict | Current service capability | Scrub stage |
|---:|---|---|---|---|---|
| 01 | Discord::protected_send | Beta | unsupported | unavailable | Planned |
| 02 | Discord::protected_receive | Beta | unsupported | unavailable | Planned |
| 03 | Discord::attachments | Planned | supported | unavailable | Planned |
| 04 | Discord::scrub | Planned | supported | Planned | Planned |
| 05 | Discord::status | Beta | unsupported | unavailable | Planned |
| 06 | Signal::protected_send | Planned | supported | unavailable | Not applicable |
| 07 | Signal::protected_receive | Planned | supported | unavailable | Not applicable |
| 08 | Signal::attachments | Planned | supported | unavailable | Not applicable |
| 09 | Signal::scrub | Not applicable | supported | Not applicable | Not applicable |
| 10 | Signal::status | Coming soon | supported | coming_soon | Not applicable |
| 11 | WhatsApp::protected_send | Planned | supported | unavailable_without_two_peer_proof | Not applicable |
| 12 | WhatsApp::protected_receive | Planned | supported | unavailable_without_two_peer_proof | Not applicable |
| 13 | WhatsApp::attachments | Planned | supported | unavailable_without_two_peer_proof | Not applicable |
| 14 | WhatsApp::scrub | Not applicable | supported | Not applicable | Not applicable |
| 15 | WhatsApp::status | Coming soon | supported | coming_soon | Not applicable |
| 16 | Telegram::protected_send | Externally blocked | supported | externally_blocked_release_gate | Not applicable |
| 17 | Telegram::protected_receive | Externally blocked | supported | externally_blocked_release_gate | Not applicable |
| 18 | Telegram::attachments | Externally blocked | supported | externally_blocked_release_gate | Not applicable |
| 19 | Telegram::scrub | Not applicable | supported | Not applicable | Not applicable |
| 20 | Telegram::status | Externally blocked | supported | externally_blocked | Not applicable |
| 21 | Outlook / OSL Mail::protected_send | Planned | supported | unsupported_mail_gate | Planned |
| 22 | Outlook / OSL Mail::protected_receive | Planned | supported | unsupported_mail_gate | Planned |
| 23 | Outlook / OSL Mail::attachments | Planned | supported | unsupported_mail_gate | Planned |
| 24 | Outlook / OSL Mail::scrub | Planned | supported | Planned | Planned |
| 25 | Outlook / OSL Mail::status | Coming soon | supported | unsupported | Planned |

## Decision

The saved audit has a nonzero status claim count, `25`, and lists exactly 25
claims. Every claim is marked supported or unsupported, every row names service
evidence, every row names its Scrub stage, and `unmarked=0`.

The unsupported words are all Discord `Beta` words in the website connector
matrix: `Discord::protected_send`, `Discord::protected_receive`, and
`Discord::status`. They conflict with the current support matrix, which records
Discord as unavailable and blocks available/runtime claims with D6 and W9.
