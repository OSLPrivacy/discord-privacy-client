# Web-surface substrate probe

**Task:** T4-E3  
**Status:** not measured — `SUBSTRATE-MATRIX` has not been earned.

This authoring environment cannot start the Windows desktop shell, sign into
twelve third-party services, or inspect their embedded browser states. The
matrix below is intentionally blank rather than a prediction. In particular,
Gmail's expected embedded-webview refusal is not a result until the exact
message is captured from the live run.

| Surface | Account used | Embedded sign-in result | Exact refusal/degradation | Evidence path |
| --- | --- | --- | --- | --- |
| Gmail | not run | — | — | — |
| Outlook | not run | — | — | — |
| Proton | not run | — | — | — |
| Yahoo | not run | — | — | — |
| AOL | not run | — | — | — |
| GMX | not run | — | — | — |
| Mail.com | not run | — | — | — |
| Instagram | not run | — | — | — |
| Snapchat | not run | — | — | — |
| Messenger | not run | — | — | — |
| X | not run | — | — | — |
| WhatsApp Web | not run | — | — | — |

## Required live run

Run `WEB-E2` first on the Windows VM and retain its binary hash. For every
surface, open its fixed official origin in the shipping embedded host, attempt
normal interactive sign-in, and record `reached inbox`, `blocked`, or
`degraded` plus the exact provider text. Save UIA/log evidence with the row.
No substrate decision, onboarding copy, or L2/L3 availability claim may cite
this report until all twelve rows are measured.
