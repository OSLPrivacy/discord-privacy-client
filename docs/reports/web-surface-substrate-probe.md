# Web-surface substrate probe

**Task:** T4-E3
**Status:** not measured — this is deliberately not a `SUBSTRATE-MATRIX` result.

The required probe needs a Windows VM running the desktop shell with WebView2
and twelve real, user-controlled sign-ins.  This checkout is a Linux-only
authoring environment.  No embedded sign-in was attempted and no provider
result, refusal string, or substrate assignment has been inferred from source
code or public documentation.

T4-E2's Windows-only `WEB-E2` harness is present in
`apps/osl-hub/tests/web_surface_a11y_spike.rs`, and the embedded host supplies
`--force-renderer-accessibility=complete` through `additional_browser_args`.
That implementation is a prerequisite for the measurement; it is not evidence
that any service accepts an embedded sign-in.

## Required Windows measurement

Run the desktop shell on a Windows VM for each row below.  Start with a fresh
OSL-owned embedded-host profile, attempt a real sign-in that the account owner
performs, and record the exact visible refusal/error text (or the reached
inbox).  Do not substitute a normal browser result, an HTTP request, or an
assumption about the provider's OAuth policy. Retain the UI Automation and log
evidence for each row.

| Surface | Embedded URL | Result | Exact refusal / observed degradation |
| --- | --- | --- | --- |
| Instagram | `https://www.instagram.com/` | unmeasured | — |
| Snapchat | `https://web.snapchat.com/` | unmeasured | — |
| X | `https://x.com/` | unmeasured | — |
| Messenger | `https://www.facebook.com/messages/` | unmeasured | — |
| Gmail | `https://mail.google.com/` | unmeasured | — |
| Outlook web | `https://outlook.live.com/mail/` | unmeasured | — |
| Proton Mail | `https://mail.proton.me/` | unmeasured | — |
| Yahoo Mail | `https://mail.yahoo.com/` | unmeasured | — |
| AOL Mail | `https://mail.aol.com/` | unmeasured | — |
| GMX | `https://www.gmx.com/` | unmeasured | — |
| Mail.com | `https://www.mail.com/` | unmeasured | — |
| iCloud Mail | `https://www.icloud.com/mail/` | unmeasured | — |

Gmail is expected to reject an embedded-webview OAuth flow, but that expectation
must not be promoted to a result: the exact current refusal belongs in this
table after the live attempt.

## Consumer rule

Until every required row has a recorded live result, consumers must treat the
surface as unproven and fail closed to S3/L1-only.  In particular, this document
does **not** authorize T4-S1 to choose S1 or S2 for any service.
