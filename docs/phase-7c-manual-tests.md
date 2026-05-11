# Phase 7c — Manual Test Checklist

Phase 7c shipped the first Whitelist UI surfaces in `boot.js` (profile
popout button, channel-header encrypt toggle + burn button, persistent
invitation banner) plus the v=2 send-path gate. Because this is a
WebView2 injection layer, none of the UI behavior is reachable from
`cargo test` — these scenarios must be exercised by hand against a
live discord.com session inside the Tauri build.

Build & run:

```pwsh
cargo tauri dev
```

Open DevTools (Ctrl+Shift+I) and keep the console visible — every 7c
surface emits `[OSL]` log lines for diagnosis.

## A. Pre-flight

- [ ] App opens, you can sign in to Discord, identity loads.
- [ ] DevTools console shows no red errors before any interaction.
- [ ] `window.__oslDebugSendV2` is defined (sanity probe — Phase 7b
      harness).
- [ ] `oslInstallPhase7c` was called (look for the install log line, or
      type `document.getElementById('__osl_toast_stack')` after the
      first toast — it should resolve to a `<div>`).

## B. Profile popout — Whitelist button

1. Open a DM with any other Discord user.
2. Click the peer's avatar in the message header to open the **profile
   popout** (the small floating card, not the full user panel).
3. **Expect:** a "Whitelist…" button appears below the existing action
   buttons (Add Friend / Message / etc.), with the lock icon.
   - [ ] Button renders.
   - [ ] Button has `data-osl-whitelist-btn="1"`.
   - [ ] Button is *idempotent* — closing & reopening the popout does
         not produce duplicate buttons.
4. Click the button.
   - [ ] A scope-selection dropdown appears anchored under the button.
   - [ ] Options shown match what's contextually valid:
         - In a DM: only "DM" should appear (server scopes hidden).
         - In a server channel: "This channel" and "This server".
         - In a GC: only "This group chat".
5. Pick "DM" (or whichever scope you want to test).
   - [ ] Toast: `"Invitation sent to <peer> for <scope label>"`.
   - [ ] Console shows `[OSL] oslSendControlMessage OK` for the
         invitation control message.
   - [ ] The peer (run a second client) sees the invitation arrive
         (see Section E).

## C. Channel header — encrypt toggle + burn button

1. In any text channel or DM, look at the header icon row (right side
   of the channel title, where Pins / Inbox / Members live).
2. **Expect:** two new buttons inserted *before* the native icons:
   - A lock icon (encrypt toggle).
   - A flame icon (burn).
3. Layout & idempotency:
   - [ ] Buttons render.
   - [ ] Switching channels rebinds them (no duplicates, no stale
         state from the previous channel).
   - [ ] Lock icon initial state reflects `osl_get_scope_encryption_state`
         — locked-closed if `encrypt_toggle && has_whitelist`, otherwise
         locked-open.
4. Click the lock with NO whitelist set:
   - [ ] Toast: `"Whitelist a user first to enable encryption."`
   - [ ] Lock state unchanged.
5. Add a whitelist entry (via Section B), then click the lock:
   - [ ] Toast: `"Encryption ON in <scope>"`.
   - [ ] Lock icon flips to closed.
   - [ ] Subsequent sends in this channel produce **v=2** wire (see
         Section D).
6. Click the lock again:
   - [ ] Toast: `"Encryption OFF in <scope>"`.
   - [ ] Lock flips to open.
   - [ ] Subsequent sends revert to **v=1**.
7. Click the flame:
   - [ ] Confirmation modal appears with the danger styling, with
         "Burn" and "Cancel" buttons.
   - [ ] Cancel dismisses the modal with no side effects.
   - [ ] Confirm fires a burn marker (control message). Console:
         `[OSL] oslSendControlMessage OK` then a local burn log.
   - [ ] In the channel, all your prior messages now render as
         permanent ciphertext to both you and the peer (post-Phase 7
         burn semantics).

## D. v=2 send-path gate (Coexist mode)

The interceptBody gate is the load-bearing piece for "Coexist: v=2 if
scope+toggle on, else v=1".

1. With encrypt toggle **OFF** in the current channel, send a normal
   message.
   - [ ] Console: a `__OSL_INTERCEPT__` log line (v=1 path).
   - [ ] No `[OSL] v=2 send gate` log line for this send.
   - [ ] Recipient sees the message decrypted via the v=1 path.
2. Turn encrypt toggle **ON** (after whitelisting), then send again.
   - [ ] Console: `[OSL] v=2 send gate ... wire=DPC0:: channel=...`
   - [ ] **NO** `__OSL_INTERCEPT__` log line for this send.
   - [ ] Recipient (also running this build) decrypts the v=2 wire —
         observe `[OSL] handleDecryptResult ok` (or equivalent) on the
         receiver.
3. Turn encrypt toggle **OFF** again, send.
   - [ ] Back to v=1 path. No `v=2 send gate` log.
4. Pre-encrypted passthrough (no double-wrap):
   - [ ] When `interceptBody` sees content already starting with
         `DPC0::` (e.g. from `oslSendControlMessage`), it short-circuits
         BEFORE the v=2 gate. Verify by triggering a burn or invitation
         control message and confirming no `[OSL] v=2 send gate` log
         line appears for it.

## E. Pending invitation banner (recipient side)

1. On the recipient client, the moment Client A's invitation arrives:
   - [ ] A banner appears at the top of the channel view (above the
         message list).
   - [ ] Banner text identifies the sender and scope.
   - [ ] Banner has Accept and Decline buttons.
2. Click **Decline**:
   - [ ] Banner removes itself.
   - [ ] Toast: `"Declined invitation from <sender>"` (or equivalent).
   - [ ] Console: `osl_decline_invitation` invoke succeeded.
   - [ ] Sender sees a corresponding response control message arrive
         and the scope state remains "not whitelisted" on sender side.
3. Repeat the invitation, then click **Accept**:
   - [ ] Banner removes itself.
   - [ ] Toast: `"Accepted invitation from <sender>"`.
   - [ ] Sender sees the response and the scope auto-enables
         (encrypt_toggle ON for that scope on sender side).
   - [ ] Both sides can now send v=2 to each other in that scope.
4. Multiple pending invitations:
   - [ ] Banners stack (one per pending invitation).
   - [ ] Acting on one banner leaves the others intact.
   - [ ] Reloading the page re-renders all still-pending banners on
         next sweep (driven by `osl_list_pending_invitations`).

## F. Persistence

1. Set toggle ON in a DM, send a v=2 message, then **reload the page**
   (Ctrl+R).
   - [ ] Lock icon comes back in the closed state (state persisted via
         `osl_get_scope_encryption_state`).
   - [ ] Banner state is consistent with current pending invitations.
2. Quit the app entirely, relaunch.
   - [ ] Same persistence checks hold across full restart.

## G. Negative paths / fail-closed

- [ ] If the v=2 gate path can't determine `selfId`, send aborts (no
      plaintext leak) and console shows
      `[OSL] v=2 send gate ... no selfId; ABORT (fail-closed)`.
- [ ] If `oslEncryptV2` returns non-string, send aborts with
      `v2_encrypt_failed`.
- [ ] If `osl_get_scope_encryption_state` fails, code falls through to
      v=1 (legacy behavior) rather than failing.
- [ ] Triggering UI without an identity loaded shows the existing
      "no identity" guards (preserved from earlier phases).

## H. Selector resilience

Selectors are drawn from `docs/phase-7c-selectors.md` (build 541436)
using `[class*="prefix"]` prefix-matching so post-hash-rotation builds
should still bind. To spot-check after a Discord update:

- [ ] Profile button still injects on first popout open.
- [ ] Header buttons appear in the icon row.
- [ ] Banner mounts above the message list.

If any of these no longer find their anchor, run the survey script:

```js
// In DevTools console:
// (paste contents of scripts/survey-discord-selectors.js)
```

and update `docs/phase-7c-selectors.md` + adjust the relevant
selector in `boot.js`.

## I. What is NOT in scope for 7c

These belong to Phase 7d:

- Settings menu surface (manage all whitelists / global view).
- Keybinds for toggle / burn.
- Right-click context-menu integration.
- Multi-select whitelist editing.
