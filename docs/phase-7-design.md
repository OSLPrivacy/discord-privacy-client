# Phase 7 — Whitelist Trust Model + Scoped Burns

**Status:** SUPERSEDED by 9-C1 in the handshake area — see
[`phase-9-c1-permissive-decrypt.md`](./phase-9-c1-permissive-decrypt.md).
The §2 trust model + §3 scope semantics still apply; the §4
invitation handshake (control messages 0x02 / 0x03, `incoming_decrypt_accepted`,
accept/decline banners) was removed. Recv is now permissive: if we
have the keys, we decrypt — Discord's block list is the trust
boundary, not an OSL-internal accept queue.

**Replaces:** original Phase 6b (panic-button burn spec, shelved)
**Migration:** lossless one-shot in `ipc::migration` at boot (9-C1)
**Estimated scope:** 6–8 weeks solo dev

---

## 1. Goal

Replace the implicit "if I have your key, I encrypt to you" trust model with an explicit user-controlled whitelist system, and implement scoped burn functionality on top of it.

## 2. Trust model

### 2.1 Whitelist scopes

A whitelist is **scope-aware**. The same user can be whitelisted in multiple scopes independently. The scopes are:

| Scope | Description | Broadenable? |
|---|---|---|
| **DM** | Per-user. Whitelisting a user in their DM with you. | YES — automatically grants decryption access in any GC/server you share with them. |
| **GC (full)** | Every member of a group chat can decrypt your messages in that GC. | NO |
| **GC (per-user)** | Specific user(s) in a GC can decrypt; others see ciphertext. | NO |
| **Server channel (full)** | Every channel member can decrypt your messages in that channel. | NO |
| **Server channel (per-user)** | Specific user(s) in a channel can decrypt; others see ciphertext. | NO |
| **Entire server (full)** | All server members with channel access can decrypt your messages anywhere in the server. | NO |
| **Entire server (per-user)** | Specific user(s) in a server can decrypt your messages anywhere in the server. | NO |

### 2.2 Whitelist precedence

- **No blacklist concept.** Only whitelist exists. To remove access: un-whitelist (which functions as a burn — see §3).
- **Most-permissive wins** when scopes overlap. If Henry is DM-whitelisted (broadened) AND you full-GC whitelist a GC he's in, he can decrypt in both.
- DM broadening is automatic and additive. It never overrides; it only grants.

### 2.3 Encryption auto-enable

Whitelisting a scope **auto-enables encryption** for that scope. Sending in that scope encrypts by default.

### 2.4 Per-scope toggle override

Each scope (DM, GC, channel, full server) has a separate **encryption toggle** that overrides the auto-enable:
- ON (default after whitelist) → messages encrypt
- OFF → messages send as plaintext, even if scope has whitelisted users

The toggle is **grayed out / unavailable** when no whitelist exists in that scope. (Prevents accidental encrypt-to-nobody.)

### 2.5 Receiver-side accept/decline

When Liam whitelists Henry in any scope, Henry's client receives a **persistent notification** that does not dismiss until Henry explicitly accepts or declines.

- Notification queues offline. Shown on next launch if Henry was offline.
- Accept → Henry's client decrypts Liam's messages in that scope going forward.
- Decline → Henry's client treats Liam's messages as ciphertext (no decryption attempted) in that scope.
- Henry can change his mind later via OSL settings menu.

Notification delivery: encrypted control message via Discord (DM channel for DM-scope, target channel for channel/GC/server scopes), backed up by keyserver flag for offline reliability.

---

## 3. Burn semantics

### 3.1 Scope hierarchy

Burns are **never broadenable except account burn.** Each burn affects only the named scope.

| Burn | Effect | Cascade |
|---|---|---|
| **DM burn** | Your messages in that DM → ciphertext for both you and peer. Peer's messages unaffected. | None |
| **GC burn** | Your messages in that GC → ciphertext for everyone. Other members' messages unaffected. | None |
| **Server channel burn** | Your messages in that one channel → ciphertext. Your messages in other channels of same server unaffected. | None |
| **Entire server burn** | Your messages in ALL channels of that server → ciphertext. Other servers/DMs/GCs unaffected. | All channels in that server |
| **Account burn** | All your sent messages everywhere → permanent ciphertext. Local state wiped. Fresh-install state. | EVERYTHING |

### 3.2 Burn = key rotation

Burn is implemented via **per-message ephemeral key destruction**:
- Each encrypted message uses an ephemeral symmetric key K (AES-256-GCM)
- K is wrapped with the static-static ECDH shared secret, included in wire format
- Burn = wipe K from local SQLite on both sides
- Without K and without ability to derive K from the wrapped form, the ciphertext on Discord's servers is mathematically opaque

This gives burns real teeth: a recipient can no longer decrypt even with developer tools or local data access.

### 3.3 Un-whitelist = burn

Un-whitelisting a scope is **functionally identical to burning that scope.** The user clicks "un-whitelist" and the system:
1. Triggers the burn for that scope (key rotation, ciphertext for everyone going forward AND retroactively)
2. Removes whitelist entry
3. Sends control message to peer(s) so they wipe their decryption capability

### 3.4 DM un-whitelist broaden choice

When un-whitelisting a DM, the user is prompted:
> "Un-whitelist Henry from DM. Henry currently has broadened access in 3 shared GCs/servers. Also revoke his broadened access there?"
> [ Yes, revoke everywhere ]  [ No, just this DM ]

- Yes → burns DM AND removes broadened access (he loses decryption in all scopes where his access came from the DM broaden)
- No → burns just the DM. Any independent whitelists in shared scopes remain. Broadened access in unaffected.

For GC/server un-whitelist: always just burns that scope. Never affects DM (since GC/server whitelists aren't broadenable).

### 3.5 Re-enabling encryption after burn

Past burned messages stay gibberish forever. New messages encrypt with new keys.
- Whitelist the scope again → fresh keys → new messages encrypt and decrypt normally
- Both parties re-acknowledge whitelist (receiver gets notification again)

### 3.6 Account burn aftermath

Account burn:
1. Wipes identity keypair, peer_map, channels.json, whitelist tables, messages.sqlite
2. Wipes keyserver entries tied to user
3. Client returns to fresh-install state — looks like never used OSL
4. To use OSL again: reinstall (or re-onboard from scratch)
5. All prior messages across all channels → permanent gibberish

---

## 4. Wire format v=2

### 4.1 Old format (v=1)

`DPC0::<base64(version=1 || ciphertext)>` where ciphertext is encrypted with shared secret.

### 4.2 New format (v=2)

`DPC0::<base64(version=2 || type || wrapped_K || GCM(K, body))>`

- `version` = 2 (1 byte)
- `type` = 1 byte:
  - `0x00` = content message (normal text)
  - `0x01` = burn marker (control)
  - `0x02` = whitelist invitation (control)
  - `0x03` = whitelist response (control)
  - `0x04..0xFF` = reserved for future
- `wrapped_K` = ECDH-encrypted ephemeral AES-256-GCM key (32 bytes plaintext, ~76 bytes wrapped)
- `GCM(K, body)` = the actual encrypted payload using K

### 4.3 Backward compat

v=1 messages keep working for read-only access during migration window. After Phase 7 fresh start, all new messages are v=2. v=1 path is deprecated.

### 4.4 Per-recipient wrapping (multi-user scopes)

For GC/server scopes with multiple whitelisted users, the wire format wraps K once per recipient:
`wrapped_K_list = [(recipient_pubkey_hash, wrapped_K_for_recipient), ...]`

Each recipient finds their entry by pubkey hash, unwraps K, decrypts body. This adds bytes per recipient but enables true per-user revocation (un-whitelist Henry in GC = stop including his wrapped K in future messages, no rekey needed for others).

---

## 5. Schema changes

### 5.1 peer_map.json

```json
{
  "henry_discord_id": {
    "pubkey": "base64...",
    "discord_id": "...",
    "first_seen": "iso8601",
    "incoming_decrypt_accepted": true,        // did I accept their whitelist invitation
    "outgoing_whitelists": [
      { "scope": "dm", "broadened": true, "enabled_at": "iso8601" },
      { "scope": "gc", "id": "1234...", "user_specific": false },
      { "scope": "server_channel", "server_id": "...", "channel_id": "...", "user_specific": true },
      { "scope": "server_full", "server_id": "...", "user_specific": false }
    ],
    "burned_scopes": [
      { "scope": "dm", "burned_at": "iso8601" }
    ]
  }
}
```

### 5.2 New: whitelist_state.json

Tracks scope-level state independent of per-peer entries:

```json
{
  "dm:henry_discord_id": { "encrypt_toggle": true, "auto_enabled": true },
  "gc:1234567890": { "encrypt_toggle": true, "full_whitelist": true, "members": ["liam", "henry", "alice"] },
  "server_channel:9876:5432": { "encrypt_toggle": true, "full_whitelist": false, "whitelisted_users": ["henry"] },
  "server_full:9876": { "encrypt_toggle": false, "full_whitelist": false, "whitelisted_users": [] }
}
```

### 5.3 New: pending_invitations.json

Receiver-side queue for whitelist notifications:

```json
{
  "from_liam_dm": { "from": "liam_id", "scope": "dm", "received_at": "iso8601", "status": "pending" },
  "from_alice_gc_xyz": { "from": "alice_id", "scope": "gc", "scope_id": "xyz", "status": "pending" }
}
```

UI shows persistent banner per pending entry.

### 5.4 messages.sqlite additions

- `burned INTEGER DEFAULT 0`
- `burned_at INTEGER`
- `wrapped_key BLOB` — the per-message K wrapped for this recipient (NULL if burned)
- `scope_type TEXT` — dm, gc, server_channel, server_full (for query filtering on burns)
- `scope_id TEXT` — channel/GC/server ID

---

## 6. UI injection points

All require DevTools inspection to find current Discord class names. Selectors documented separately in §6.5 once gathered.

### 6.1 Whitelist button

**Location:** user profile popup (modal that appears when clicking username/avatar).
**UI:** dropdown button "Whitelist user…" → menu with scope options:
- Whitelist in DM (with broaden checkbox)
- Whitelist in current GC/channel/server (only shown if applicable to current view)
- Manage all whitelists for this user… (opens settings panel showing all scopes)

### 6.2 Encryption toggle

**Location:** channel header (top bar).
**UI:** small icon (lock/unlock) showing current state. Click toggles. Grayed-out when no whitelist exists in scope.
**Tooltip:** explains current state and reason if disabled.

### 6.3 Burn buttons

**Location:** channel header (next to encrypt toggle), styled red.
**UI:** "Burn" button → confirmation modal:
- Lists what will be burned (your messages count, scope name)
- "Are you sure? This cannot be undone."
- Confirm / Cancel
**Account burn:** lives in OSL settings menu only (not in any channel header) — too dangerous to be accidentally clickable.

### 6.4 OSL settings menu

**Location:** new dedicated settings panel, accessible via:
- A gear icon injected into Discord's user area (bottom-left, near voice controls)
- Or hotkey (default: configurable)

**Contents:**
- Identity / keypair info
- All current whitelists (list, grouped by scope, with un-whitelist buttons)
- All pending whitelist invitations (accept/decline)
- Keybind configuration for all actions:
  - Burn current scope
  - Burn account (with confirmation always required, even from keybind)
  - Toggle encryption in current scope
  - Open settings menu
- Duress password setup
- Fresh-start / reinstall option

### 6.5 Selector documentation (TODO)

Before implementation, document all current Discord class selectors via DevTools:
- User profile popup container
- Channel header container
- Bottom-left user area (for settings gear injection)
- Channel name element (to identify current scope context)
- DM channel vs GC channel vs server channel distinguishing markers
- Server icon / sidebar (to identify server context)

---

## 7. Receiver-side notification flow

### 7.1 Whitelist invitation delivery

When Liam whitelists Henry in scope X:
1. Liam's client computes scope identifier
2. Liam's client sends control message (type=0x02 whitelist invitation) via:
   - DM channel (for DM scope)
   - Target channel (for GC/server scopes — may be visible to others as DPC0:: blob, that's fine)
3. Liam's client posts whitelist flag to keyserver: `{ from: liam_id, to: henry_id, scope: X, sent_at: ... }`
4. Henry's client polls keyserver on launch + receives via Discord observer

### 7.2 Notification UI

Persistent banner in OSL settings panel and as floating notification in Discord UI:
> "Liam has invited you to decrypt their messages in [scope description]."
> [ Accept ]  [ Decline ]

Banner persists across app restarts until explicitly clicked. Only one banner per (sender, scope) pair.

### 7.3 Accept

- Henry's client stores: `incoming_decrypt_accepted[liam][scope] = true`
- Future messages from Liam in that scope decrypt automatically
- Sends control message (type=0x03 accept response) so Liam's client can mark "Henry accepted" in UI

### 7.4 Decline

- Henry's client stores: `incoming_decrypt_accepted[liam][scope] = false`
- Future messages stay as ciphertext on Henry's side
- Sends control message (type=0x03 decline response)
- Liam's UI shows "Henry declined" status; he can re-invite later

---

## 8. Implementation phases (sub-phases of 7)

This is too big for one phase. Break into:

### 7a — Foundation (wire format v=2 + schema)
- Wire format v=2 implementation in Rust (encrypt/decrypt)
- Schema migrations for peer_map, whitelist_state, pending_invitations, messages.sqlite
- Fresh-start flow (wipe local state, regen identity)
- Tests for v=2 round-trip with various type bytes
- **Acceptance:** can encrypt/decrypt v=2 messages with no UI, verified via Rust tests

### 7b — Whitelist primitives (no UI yet)
- Whitelist scope resolution logic
- Per-recipient key wrapping for multi-user scopes
- Send-path checks: "can I encrypt to this recipient in this scope?"
- Recv-path checks: "should I decrypt this from this sender in this scope?"
- Control message handling (whitelist invite/accept/decline, burn marker)
- **Acceptance:** Rust tests pass for all whitelist combinations and burn scenarios

### 7c — Whitelist UI
- DevTools selector survey (document all needed selectors)
- User profile popup injection (whitelist button + scope dropdown)
- Channel header injection (encrypt toggle, burn button)
- Persistent invitation notification UI
- **Acceptance:** can manually whitelist a peer through Discord UI, verified end-to-end with Henry

### 7d — Settings menu + duress + keybinds
- OSL settings panel UI (accessible via gear or hotkey)
- All current whitelists list with management
- Pending invitations list
- Keybind configuration
- Duress password setup and behavior
- **Acceptance:** all listed features work, duress password verified to wipe state silently

### Sequencing
7a → 7b → 7c → 7d, no parallelism (each builds on previous).

---

## 9. Things explicitly OUT of scope for Phase 7

- Forward secrecy / Signal-style ratchet (still future work)
- Encrypted attachments (was 6c, now Phase 8)
- Closed beta with Henry (post-7)
- Production hardening, keyserver scaling, audit (Layer 11, post-7)
- Mobile client (far future)

---

## 10. Risks and open questions

- **Discord UI churn:** Discord's React app reshuffles class names with updates. Need a maintenance plan for selector breakage.
- **Multi-recipient wire format size:** for large servers with many whitelisted users, wrapped_K_list grows. Discord's message size limit (4000 chars) caps practical recipient count. May need a "group key" optimization for full-server-whitelist scopes.
- **Keyserver reliability:** notification fallback path depends on keyserver being reachable. If keyserver is down, notifications only deliver via Discord. Document this.
- **Race condition: invitation accepted after sender burned:** if Liam burns DM with Henry, then Henry accepts the prior invitation, what happens? Answer: accept is no-op (Liam's burn revoked the keys, Henry can't decrypt anything anyway).
- **Per-user GC whitelist scaling:** if you whitelist 5 of 10 GC members, every message wraps K for 5 recipients. Adds wire format size. Acceptable for Phase 7.

---

## 11. Sign-off checklist

Before implementation begins, confirm:
- [ ] All scope semantics in §2 match user intent
- [ ] All burn semantics in §3 match user intent
- [ ] Wire format v=2 design is acceptable
- [ ] Schema design is acceptable
- [ ] UI injection points cover all needed surfaces
- [ ] Implementation phases (7a–7d) sequencing is correct
- [ ] Out-of-scope items in §9 are correctly deferred

Once signed, this doc is committed to `docs/phase-7-design.md` and Claude Code prompts begin against it.
