# Scrub deletion: signed-in session first

Status: protocol, provider preloads, shared assisted adapter, and fake-driven tests are implemented. The native hosted-webview injection/reply seam is intentionally not enabled yet. No live Gmail, Discord, or Telegram account has been exercised by this worktree.

## Decision

Scrub's primary deletion path for email web, Discord, and Telegram Web reuses the exact account already signed in inside OSL's retained hosted service webview. It does not create a second login, ask for an app password, create an API session, use a proxy, or navigate an invisible automation browser.

IMAP and Telegram TDLib remain supported as optional secondary adapters. They are not deleted or silently selected. IMAP is the lower-ban-risk email option because it uses a documented mailbox protocol instead of operating a consumer web UI. TDLib remains unavailable until its client is packaged and its session is live-confirmed.

The encrypted local Scrub index continues to report `deletion_enabled: false`. A transport capability is separate and account-specific; it can become active only after the live hosted path confirms the exact provider, account, session epoch, and current UI schema. A unit test or compiled preload is not a live confirmation.

## In-boundary command port

The page-facing authority is exactly:

```text
scrollHistory({ maxScrolls, maxItems, beforeUnixMs })
listOwnItems()
deleteOwnItem(id)
verifyGone(id)
```

`scrollHistory` is bounded to at most ten scrolls and 500 loaded items per call. `listOwnItems` returns only IDs and deletion-relevant metadata for items the preload proves are authored by the signed-in account. `deleteOwnItem` can address only one ID previously listed as owned. `verifyGone` is a UI readback and reports both its answer and whether the current surface covers that ID.

The boundary has no generic `invoke`, JavaScript evaluation, selector, click, keyboard, pointer, HTTP, fetch, RPC, proxy, send, post, direct-message, friend, join, react, or upload operation. Provider preload code contains its fixed semantic UI recipe internally; neither OSL's main webview nor a remote page chooses selectors or supplies scripts. Runtime parsing rejects malformed identities, non-owned items, overlarge results, ambiguous readbacks, and unknown friction values.

Provider implementations are separate:

- Gmail Web is concrete first and only recognizes an allowlisted `mail.google.com` surface. It lists sent/own loaded rows, uses the provider's visible delete action, and verifies absence in the covered live surface.
- Discord recognizes only `discord.com`, own-message markers, and its fixed delete-message action.
- Telegram Web recognizes only `web.telegram.org`, outgoing-message markers, and its fixed delete action.

Selectors are versioned. A missing root, missing ownership marker, malformed ID, missing delete action, changed account, or changed schema is permanent friction for that run; it is never treated as an empty mailbox or successful deletion.

## Assisted state machine

One reusable adapter maps all three hosted providers to the existing deletion engine:

```text
human presence -> fixed 1.5 s rest -> bounded scroll -> list own items
       |                                      |
       +-- stale/away/max batch -> PARKED      +-- friction -> STOPPED

reviewed scope -> dry-run -> inspect own item -> semantic delete -> verifyGone
                                   |                    |
                       non-retractable:             absent: confirmed-deleted
                       surface-only                 present: confirmed-not-deleted
                                                    ambiguous: UNKNOWN + stop
```

The adapter is single-account and provider-bound. Pacing is fixed and overt, not randomized or tuned to evade detection. It automatically parks without recent human presence and after a bounded batch. Captcha, challenge, rate-limit signal, signed-out state, account change, schema drift, malformed response, thrown error, or any unknown result permanently stops the run. An `UNKNOWN` receipt is not automatically retried.

The existing engine still enforces explicit final confirmation, an exact no-delete dry run, scope-only shrink, protected scopes, own-content-only, age/count limits, three-state results, and readback before any deletion claim.

## Non-retractable and retained copies

If the current UI cannot retract a remote item, Scrub performs no delete call and reports `confirmed-not-deleted` with `surface-only` detail. Even a confirmed UI deletion covers only the stated live provider surface. Recipient copies, exports, backups, notifications, legal retention, caches, and other devices may remain.

## Provider and ban risk

| Path | Default | Risk posture | Coverage |
|---|---:|---|---|
| Gmail Web signed-in UI | Primary | Elevated. UI-assisted deletion may violate service expectations or trigger account protection; treat it like the Discord path. | Currently loaded own/sent items and UI readback only. |
| Discord signed-in UI | Primary | High. Assistance may trigger restrictions or an account ban. | Currently loaded own messages and UI readback only. |
| Telegram Web signed-in UI | Primary | Elevated. UI/schema and flood controls can stop the run. | Currently loaded outgoing messages and UI readback only. |
| IMAP | Optional | Lower ban risk for email, but requires separate credentials/session and mailbox semantics. | Message-ID/date search and IMAP readback. |
| Telegram TDLib | Optional | API/session and flood-wait risk; packaging and live verification still required. | Adapter-defined message readback when packaged. |

These are risk classifications, not guarantees about provider enforcement. OSL never promises that assisted deletion is safe from sanctions.

## Native host coordination flag

The current `service_host` owns creation, retention, suspension, navigation policy, and profile identity for the native child webview. It does not yet expose a reply-capable isolated-world/preload channel. Wiring that deeply while concurrent host work is active would risk the shared lifecycle.

The additive integration seam is `HostedSessionCommandChannel.request(HostedSessionCommand)`. Host work should:

1. install exactly one provider preload when creating the allowlisted hosted webview;
2. bind it to the already-proven `ActiveServiceHost` service/account/generation;
3. accept only the four tagged commands above and return checked envelopes;
4. invalidate the channel on navigation, suspension, account change, generation change, or shutdown;
5. expose no generic evaluation or page-controlled native IPC;
6. require a live list/readback probe before advertising `liveConfirmed`.

Until that seam exists and a real account confirms it, the UI shows the session-reuse paths first but parked, and all delete controls remain disabled. Deep changes to `service_host.rs` or `native_window_host.rs` require coordination with their owner.

## Verification boundary

Automated tests here cover protocol shape, runtime envelope rejection, provider schema separation, own-items-only filtering, bounded fixed pacing, presence parking, permanent stop-on-friction, surface-only behavior, dry-run, three-state readback, and scope shrink.

The user must verify in a running desktop app that OSL can inject the port into an already signed-in Gmail/Discord/Telegram hosted session, scroll the real current UI, identify only owned items, invoke one visible delete operation, and read back its disappearance. That live behavior is not claimed by this implementation.
