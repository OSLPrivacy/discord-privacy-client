# Task 4358 Mail Message Words Audit

Date: 2026-08-07

This checkout has 9 fixed mail services in the browser shell and mail service allowlist:
Gmail, Outlook, Proton, Yahoo, AOL, GMX, Mail.com, iCloud, and Tuta. I found no
tenth mail service in the measured source set.

## Protocol Routes

- Whole IMAP message command: `UID FETCH <uid> (BODY.PEEK[])`.
- Single MIME part command: `UID FETCH <uid> (BODY.PEEK[1])`.
- RFC measurement source: RFC 9051 says `FETCH` retrieves message data, and RFC
  3501/9051 section fetch forms include message body, MIME body part, and
  header fetches.

## Source Routes

- Whole message body in this repo: `open_shared_mailbox_message` returns
  `body: message.body.clone()` in `apps/osl-hub/src/services.rs:1637` and
  `apps/osl-hub/src/services.rs:1669`.
- Fixture reader route: `SharedMailboxReader::open_message` is in
  `apps/osl-hub/src/shared_mailbox_reader.rs:161`.
- Selected web page route: `read_protected_email_open_message_with_driver_and_state`
  returns `cover_message: selected.body` in
  `apps/osl-hub/src/hub_command_surface.rs:450` and
  `apps/osl-hub/src/hub_command_surface.rs:455`.
- Proton/iCloud web mailbox routes are summary-only: `ProtonMailboxForScrubMessage`
  has subject, time, sender, owner marker, and yours, with no body field in
  `apps/osl-hub/src/hub_command_surface.rs:126`.
- Yahoo's hosted wrapper delegates to the shared body opener in
  `apps/osl-hub/src/scrub_hosted/yahoo_mail.rs:84`.
- Outlook desktop fixture delegates to the shared body opener in
  `apps/osl-hub/src/native_outlook_adapter.rs:355`.

## Service Winners

| Service | Routes tried and winner |
| --- | --- |
| Gmail | IMAP advertised on `imap.gmail.com:993`; Gmail API `users.messages.get` can return full/raw; web route is selected-page body only; OSL fixture body opener accepts `gmail`. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| Outlook | IMAP advertised on `outlook.office365.com:993`; Microsoft Graph message exposes `body`; Outlook web adapter has reading pane targets only; Outlook desktop fixture opens bodies. Winner: Microsoft Graph `GET /messages/{id}?$select=body`. |
| Proton | Direct `imap.protonmail.com` DNS failed; official Proton Mail Bridge creates local IMAP/SMTP but no helper was installed/listening here; Proton web mailbox command returns summaries without body; selected-page route can return open page body. Winner: Proton Mail Bridge local IMAP when installed. |
| Yahoo | IMAP advertised on `imap.mail.yahoo.com:993`; no repo Yahoo API helper; hosted scrub lists folders/messages; `open_yahoo_mailbox_message_for_scrub` opens fixture body. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| AOL | IMAP advertised on `imap.aol.com:993`; no repo AOL API/helper; no AOL-specific web body command; shared opener accepts `aol`. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| GMX | IMAP advertised on `imap.gmx.com:993`; no repo GMX API/helper; web adapter has reading pane targets only; shared opener accepts `gmx`. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| Mail.com | IMAP advertised on `imap.mail.com:993`, with official Premium/enablement limits; no repo API/helper; no service-specific web body command; shared opener accepts `maildotcom`. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| iCloud | IMAP advertised on `imap.mail.me.com:993`; Apple app-specific password route exists; iCloud web commands read summaries/pages only; shared opener accepts `icloud`. Winner: IMAP `BODY.PEEK[]` / `BODY.PEEK[part]`. |
| Tuta | Official Tuta source says no IMAP/POP; guessed `imap.tuta.com` DNS failed; no repo API/helper; browser shell lists Tuta but no body command; shared opener accepts fixture bodies only. Winner: none in current OSL production routes. |

## Exact Body Proof

The audit harness `node scripts/audit/task-4358-mail-message-words.mjs` returned
these three exact Gmail fixture bodies character for character:

```text
"First exact mail body.\nLine two stays here."
"Second exact mail body: punctuation, spaces, and 123."
"Third exact mail body\twith a tab and a final period."
```

## Verification Limits

`cargo test --manifest-path apps/osl-hub/Cargo.toml --features core --test task_3043_shared_mail_page_through --locked -- --nocapture`
failed before test execution because `llama-cpp-sys-2` could not find
`libclang`.

`cargo test --manifest-path apps/osl-hub/Cargo.toml --no-default-features --test task_3043_shared_mail_page_through --locked -- --nocapture`
then reached the hub crate and failed on pre-existing syntax errors in
`src/bad_message_rules.rs` and `src/burn_review_state.rs`. The task 4358 audit
therefore uses the standalone Node harness rather than claiming a green Cargo
gate.
