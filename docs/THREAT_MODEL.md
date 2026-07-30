# Threat Model

Status: Living document. Updated as features ship.

> **Source-verified correction — 2026-07-26.** Until this date the sections below
> described the *designed* v1 construction as if it were the shipping one. It is
> not. This revision restates every cryptographic row against what the current
> worktree actually executes, and marks the rest `Planned`.
>
> Read alongside the status vocabulary in
> [`design/osl-master-decision-2026-07-26.md`](design/osl-master-decision-2026-07-26.md) §0.3
> and the public wording rules in
> [`design/osl-public-claim-allowlist.md`](design/osl-public-claim-allowlist.md).
>
> Verified against worktree `osl-eye-and-features-2026-07-26`, HEAD
> `fc6b9830c679032692e812c90639a3838b4b26fd`, dirty (104 changed paths). Four other
> tabs are editing this tree; line anchors below were re-verified at the time of
> writing and may drift. Every claim here is a **source** claim
> (`implemented-unwired` / `test-proven-only`), not a runtime one — no statement in
> this file is `verified-live` on a named release build.
>
> **What actually carries traffic today:** one stateless scheme, wire `v=3`
> (`crates/ipc/src/wire_v2.rs:685`). The send dispatcher checks the sealed
> OSL-RN version pin before allowing that legacy `v=3` path, but this is a
> downgrade-refusal guard, not RN traffic: `RN_WIRE_IN_ENABLED` remains `false`.
> Both the retired Double Ratchet DM path (`v=4`) and the group sender-keys path
> (`v=5`) are present in source and switched off for production sends.
>
> **`osl-ratchet-next` has no production encrypt/decrypt path.** The crate is a
> workspace member (`Cargo.toml:29`) and `crates/ipc/Cargo.toml:73` declares the
> dependency, and its IPC adapter `crates/ipc/src/wire_rn.rs` genuinely uses it.
> The production send dispatcher now calls `wire_rn::select_wire_version` and
> reads `RnSessionStore` solely to refuse a silent downgrade if a peer is already
> pinned to RN (`crates/ipc/src/commands.rs:3284-3320`). Capability
> advertisement is absent, so normal peers are checked as
> `PeerCapabilities::Absent`; because no production path raises RN pins and
> `RN_WIRE_IN_ENABLED` is `false` (`crates/ipc/src/wire_rn.rs:83`), this guard is
> inert unless existing local state already requires RN. `send_rn` and
> `receive_rn` refuse while the gate is off (`crates/ipc/src/wire_rn.rs:843-875`),
> and inbound RN wires are rejected before bootstrap (`crates/ipc/src/commands.rs:5216-5244`).
> Status: `implemented-unwired`. No forward-secrecy, post-compromise, or
> post-quantum-authentication claim may be made on its behalf.
>
> *Anchor drift note:* the root `Cargo.toml:25-28` comment still asserts that
> osl-ratchet-next is "Reachable only from its own tests; no other crate or app
> depends on it." The first half is true in effect; the second half is now false at
> the Cargo level (`crates/ipc/Cargo.toml:73`). That comment is owned by another
> tab and was not edited here.

## Mission

Limit the power of social media companies and government surveillance over
private conversations. The tool is designed for users who want strong
privacy on Discord without switching messaging platforms — domestic abuse
survivors, journalists, activists, LGBTQ+ users in hostile environments,
lawyers, and ordinary people who value private conversation.

## Out of scope

This tool is **not** intended to provide protection against targeted
federal investigation. Users with that threat model should use Signal,
Briar, or Cwtch. Onboarding states this explicitly.

## Capability matrix

| Threat | v1 protection | Notes |
| --- | --- | --- |
| Discord reads message contents (DM) | Strong (confidentiality only) | **Stateless hybrid, not a ratchet.** Every normal DM uses wire `v=3`: a fresh per-message sender ephemeral X25519 plus an ML-KEM-768 encapsulation to the recipient's *long-term* key, HKDF-combined to wrap a fresh AES-256-GCM body key (`crates/ipc/src/wire_v2.rs:685-760`, `crates/crypto/src/pqxdh.rs:141-195`). Before `v=3` send, the dispatcher checks each non-self recipient's sealed RN version pin and refuses instead of downgrading if the peer is pinned to RN while RN wire-in is disabled (`crates/ipc/src/commands.rs:3284-3320`). The retired Double Ratchet DM path is compiled but dead: `crates/ipc/src/commands.rs:3170` sets `let v4_dm_enabled = false;` so the `if` on `:3171` never runs. See "Forward secrecy" below for exactly which compromise this survives. |
| Discord reads message contents (group) | Strong (confidentiality only) | **Sender keys are off.** Groups use the same stateless `v=3` scheme as DMs unless the RN pin guard refuses legacy send for a previously pinned peer. The `v=5` sender-keys router at `crates/ipc/src/commands.rs:3268-3282` is gated on `AppState::sender_keys_enabled`, whose declaration at `crates/ipc/src/state.rs:279-284` documents it as defaulting false in production; no production code enables it (only tests do). `apps/osl-hub/src/core_bridge.rs:269` reports `group_sender_keys_enabled: false` to the UI. |
| Discord reads image / file contents | Planned (source exists, unproven end-to-end) | Streaming XChaCha20-Poly1305 with bucketed padding exists (`crates/ipc/src/attachment_wire.rs:4`), but there is no "message-chain key" to wrap under — no chain exists. Master §10 finding 4 (non-image attachments staged as durable plaintext) is open, so the no-plaintext-at-rest promise is not met. |
| Discord runs CSAM scanning on uploaded images | Defeated | Discord sees random AEAD ciphertext on its CDN |
| Discord traffic-analyzes timing / sizes | Partial | Padding always on; jitter and cover traffic opt-in (v2.1) |
| Future quantum computer + harvested ciphertext | Defeated (confidentiality only) | ML-KEM-768 is genuinely in the live `v=3` path — a 1088-byte encapsulation per recipient slot (`crates/ipc/src/wire_v2.rs:325-332`, `:731`). This protects *confidentiality* against harvest-now-decrypt-later. It does **not** make authentication post-quantum: sender authentication rides on classical X25519/Ed25519. Never say "post-quantum authentication". |
| Server operator subpoenaed (single jurisdiction) | None v1 / Partial v2.2 | Threshold sharing across 5 jurisdictions in v2.2 |
| Server operator compromised across all 5 jurisdictions | Limited v2.2 | "5 servers, 1 operator" model documented honestly |
| User's ISP correlates Discord usage | None v1 alpha / Partial v2.2 | v1 alpha ships **without bundled VPN or Tor** (deferred to v2.2 — dependency conflict). Users run Mullvad's official app or another VPN externally. v2.2 brings bundled WireGuard + Tor key-server routing. See "Network-layer protection (v1 alpha vs v2.2)" below. |
| Phone camera captures screen | None | Document; cannot mitigate |
| Modified client on recipient side | None | Document; cannot mitigate |
| Hardware capture device | None | Document; cannot mitigate |
| Endpoint malware | Partial | TPM seal helps; cannot fully prevent |
| Past group messages outside current rotation window | **None** | Planned. There is no rotation window because sender keys are off (row 2 above). Every group message is an independent stateless `v=3` message, so a recipient's long-term keys decrypt all of them. Even when `v=5` is enabled the implemented triggers are **24 h or a membership change** (`SENDER_KEY_ROTATE_AFTER_SECS = 24 * 60 * 60` at `crates/ipc/src/commands.rs:3348-3349`; `sender_key_needs_rotation` at `:3367-3383`). No 500-message trigger and no suspicious-event trigger exist anywhere in the tree. |
| One-time RAM dump of group sender / recipient | None | Planned. Depends on the rotation window above, which does not exist. |
| Casual hands-on access to unlocked device | **None (unwired)** | Planned. The primitives exist and are test-proven: threshold 10 (`DEFAULT_FAILED_ATTEMPT_THRESHOLD` at `crates/keystore/src/password.rs:62`) returns `VerifyOutcome::DuressByThreshold` (`:283-288`), and `InactivityTimer` defaults to 900 s. But **no production code calls them.** `verify_against_record`, `VerifyOutcome` and `InactivityTimer` are referenced only by the `crates/keystore/src/lib.rs:58-59` re-export and `crates/keystore/tests/password_test.rs`. `crates/keystore/src/duress.rs` likewise has no caller outside `crates/keystore/`. Nothing in `apps/osl-hub` or `crates/ipc` invokes the lockout, the duress flow, or the timer. |
| Forced unlock under coercion | **None (unwired)** | Planned. `crates/keystore/src/duress.rs` implements the strip/wipe sequence and is test-proven (`crates/keystore/tests/duress_test.rs`), but no production caller exists — nothing outside `crates/keystore/` references it. There is no shipping path from typing a duress password to the flow running. When it is wired, the residual-forensics limits below still apply. |
| Forensic disk analysis of stripped device post-duress | None | Stripped state retains OPSEC artifacts (binary code paths, installer cache, FS journaling, SSD wear-leveling, restore points). Documented honestly. |
| MITM via key substitution after reinstall | **Open critical finding** | The safety number binds only Ed25519; X25519 and ML-KEM ride along unbound, so a keyserver can swap a recipient's *encryption* keys without changing the verified identity. Master §2 P0-1 and §10 finding 1. Until the safety number covers the whole bundle (or the bundle is identity-signed and verified on every use), the verification ceremony does not protect the keys that decrypt messages. |
| Discord adds fake members to your channel | Limited v1 / Partial v2 | v1 trusts Discord's member list; cryptographic admin-signed membership manifest in v2. Narrow threat — see "Membership-manifest residual risk" below. |
| Discord fingerprints modded client | Partial v2.6 | UA / locale / timezone / timer / fonts mitigated; canvas/WebGL not mitigated under WebView2 |
| Build pipeline compromise | Partial v2.5 | Reproducible builds + multi-sig + transparency log |
| Cooperating conversation participant | None | Document |
| User identity correlated with Discord account | Partial v2.3 | Anonymous credentials + optional pseudonymous mode |

## Cryptographic guarantees

These describe the shipping `v=3` scheme. Where a stronger property was
previously claimed, it is restated below as `Planned` with the reason.

### What v3 actually provides

- **Confidentiality (DM and group — same scheme)**: for each recipient the
  sender runs a PQXDH-shaped hybrid handshake — a **fresh per-message
  ephemeral X25519** keypair plus an ML-KEM-768 encapsulation to the
  recipient's long-term key — HKDF-combines the result into a wrap key, and
  uses it to wrap one **fresh random AES-256-GCM body key** shared by all
  recipient slots (`crates/ipc/src/wire_v2.rs:685-760`). The construction is
  secure if *either* X25519 or ML-KEM-768 holds. It is **stateless**: no
  session, no chain, no ratchet, nothing to desynchronize.
- **Body cipher is AES-256-GCM**, not XChaCha20-Poly1305. XChaCha20-Poly1305
  is used elsewhere — attachment streaming (`crates/ipc/src/attachment_wire.rs:4`),
  the local message store (`crates/store/src/cipher.rs:9`), and the unwired
  `osl-ratchet-next` — but not for message bodies.
- **Post-quantum confidentiality**: real, and in the live path. ML-KEM-768
  appears as a 1088-byte ciphertext in every recipient slot
  (`crates/ipc/src/wire_v2.rs:325-332`). Harvest-now-decrypt-later against
  message *contents* is defeated. Authentication remains classical.
- **Sender-side forward secrecy**: partial and one-directional. The sender's
  per-message ephemeral secret is discarded, and the ML-KEM shared secret is
  not recoverable from the sender's long-term keys. Compromising the
  **sender's** long-term identity key does not retro-decrypt what they sent.

### What v3 does not provide

- **Forward secrecy against recipient compromise — `Planned`.** The recipient
  contributes only long-term keys: their X25519 identity key plays both the
  `ik` and `spk` roles and there are no one-time prekeys
  (`crates/ipc/src/wire_v2.rs:722-729`, `OPK is always None`). So a recipient's
  long-term X25519 secret plus their ML-KEM-768 decapsulation key reconstruct
  every DH leg and the KEM secret for **every message ever sent to them**.
  Seizing or malware-extracting one device retro-decrypts that device's entire
  ciphertext history. *Reason it is not implemented:* forward secrecy requires
  ratcheting state, and the ratchet was deliberately disabled — see below.
- **Post-compromise security (DM) — `Planned`.** There is no ratchet step, so
  nothing heals a compromised session. `crates/ipc/src/commands.rs:3170` reads
  `let v4_dm_enabled = false;`, which makes the entire `v=4` Double Ratchet
  branch beginning at `:3171` unreachable. The in-source rationale is explicit
  and deliberate: `v=4` was "the sole source of the recurring 'ratchet desync'
  DM failures", so DMs were routed to stateless `v=3` to eliminate the desync
  class. The replacement RN adapter is not a shipping ratchet yet:
  `RN_WIRE_IN_ENABLED` is still `false`, so RN send/receive refuses before any
  state load or crypto operation. A compromised recipient stays compromised
  until they rotate identity keys out of band.
- **Post-compromise security (group) — `Planned`.** Same reason; additionally
  the sender-keys lane is off (`crates/ipc/src/commands.rs:3268-3282`, gated on
  `AppState::sender_keys_enabled`, documented as defaulting false at
  `crates/ipc/src/state.rs:279-284`). The stated reason there is multi-device
  safety: the chain key carries no device id, so one account on two machines
  desynchronizes. Remediation is protocol-level: carry a signed device id and
  key receiver chains by `(account, device)` before treating `v=5` as a product
  guarantee.
- **Bounded group blast radius / rotation window — `Planned`.** Does not exist
  today. When `v=5` is enabled the implemented triggers are 24 h
  (`SENDER_KEY_ROTATE_AFTER_SECS`, `crates/ipc/src/commands.rs:3348-3349`) or a
  membership change (`sender_key_needs_rotation`,
  `crates/ipc/src/commands.rs:3367-3383`). The "≤ 1 h", "500 msgs", and
  "suspicious event" triggers were never implemented.
- **Sender attribution — `open-security-finding`.** The generic receive path
  authenticates an in-band key but attributes the plaintext to a
  caller-supplied identity (master §2 P0-2, §10 finding 2). Until fixed,
  "who sent this" is not a cryptographic answer.

### v4/v5 reconciliation and remediation

Verification units:
`v5_sender_keys_enabled_default_false_rationale_is_documented`,
`threat_model_reconciles_v4_retirement_and_v5_ratchet_limits`.

The retired `v=4` pairwise Double Ratchet path and the disabled `v=5`
sender-key path must be read together. `v=4` is not merely waiting for a UI
switch; it was removed from the shipping send path because real deployments
hit recurring ratchet desynchronization failures. That retirement means `v=5`
cannot inherit a proven pairwise distribution channel from the current
product. Sender-key setup and rotation messages may exist in source, but the
shipping route remains stateless `v=3`.

`sender_keys_enabled` therefore defaults false as a safety property, not as a
feature flag awaiting marketing approval. The current sender-key state is
account-scoped rather than bound to a distinct physical device, so one account
used on two machines can advance or receive chain state in an order the other
machine cannot prove. Enabling it by default would trade the known
stateless-v3 limitation for a harder-to-debug group desynchronization and
misdelivery class.

Remediation before changing any threat-model row from `Planned`:

- Reintroduce a pairwise ratchet only behind a state format with atomic commit,
  skipped-key bounds, reset authority, and cross-device tests that prove no
  stale session silently decrypts or encrypts.
- Bind sender-key chains to an explicit physical-device identity and prove
  multi-device send/receive ordering, rotation, and recovery across two live
  devices for the same account.
- Add sender-key rotation triggers beyond the implemented 24-hour and
  membership-change paths if the product wants to claim a smaller blast radius;
  until then, do not claim one-hour, 500-message, suspicious-event, or
  current-rotation-only limits.
- Re-run the threat model against the actual default configuration after the
  switch is enabled. Source presence alone is not evidence that the property
  ships.

### Revocability ("burn") — what it destroys and what it does not

Burn is **policy and state deletion, not destruction of decryption
capability.** State this plainly wherever burn is described.

- The local store's `wrapped_key` column is never populated. `MessageStore::put`
  (`crates/store/src/lib.rs:194-206`) inserts
  `(discord_message_id, channel_id, sender_discord_id, sender_osl_user_id,
  ciphertext, nonce, decrypted_at, burned)` and omits `wrapped_key` entirely.
  The column exists (`crates/store/src/schema.rs:132`) and every burn path sets
  it to `NULL` (`crates/store/src/lib.rs:298`, `:342`, `:350`, `:560`) — but it
  was already `NULL`. Nothing in the tree writes it non-null.
- What burn does achieve locally is real and worth stating: it zeroblobs the
  stored `ciphertext`/`nonce`, marks the row burned, and drops the message's
  cached attachments, so the **local cached plaintext** of those messages is
  gone. **This sentence was not true until 2026-07-26 and is worth recording
  rather than quietly fixing.** The receive observer re-decrypts a channel's
  history on re-entry and re-`put`s the same ids, so a single channel re-entry
  after a burn wrote the sealed body back to disk and cleared the flag; and
  `mark_burned` short-circuited on `burned = 1`, so rows written by earlier
  builds were flagged but never actually shredded. Both are fixed — `put` now
  carries `WHERE messages.burned = 0` and refuses to write a live body under a
  burned flag, and `mark_burned` shreds unconditionally while stamping
  `burned_at` only when unset, so re-burning cannot make an old destruction look
  recent. Evidence, both directions: re-introducing the early return made the
  test fail with `sealed body survives on disk (29 non-zero bytes)`. See
  `docs/reports/store-lane-2026-07-26.md`.
- What it does **not** achieve: the ciphertext Discord holds on its CDN remains
  decryptable by anyone who still has the recipient key material — which,
  because v3 wraps to long-term recipient keys, is any holder of that
  recipient's identity keys, forever. Burn does not revoke that.
- Peer-side burn is `implemented-unwired` and additionally defective — see
  `docs/qa/two-identity-p2p-verification.md` §6 item 4.
- The cover-text rendering property (burned messages render as their original
  cover text, no `[deleted]` marker) is a presentation choice and is unaffected
  by the above.

*Reason the designed property is not implemented:* server-held per-message
wrapped keys are a keyserver lifecycle feature that was never built into the
send path. Making burn cryptographic requires the send path to wrap to a
per-message key that lives only on the keyserver, plus a proven delete. Until
then, **do not use the words "cryptographic burn" or "permanently
undecryptable" anywhere.**

## Limitations users must understand

1. **Metadata.** Discord still sees who talks to whom, when, and how
   often. This tool cannot hide that. Use Signal/Briar/Cwtch if metadata
   confidentiality is your threat model.
2. **Screenshot resistance, not prevention.** Window capture protection
   (`SetWindowDisplayAffinity` / `WDA_EXCLUDEFROMCAPTURE`) blocks common
   tools (OBS, ShareX, Game Bar, ShadowPlay) but does not stop phone
   cameras, hardware capture, or modified clients.
3. **Burn semantics and exposure window.** *`Planned` — the model below
   describes server-held per-message wrapped keys, which the send path never
   creates (see "Revocability" above). Today burn deletes the sender's local
   cached plaintext and marks state; it does not revoke anyone's ability to
   decrypt the ciphertext Discord still holds.* The intended design was: burn
   revokes future decryption of past content, and after burn:
    - Recipients with wrapped keys still cached locally retain access
      until their cache zeroes (next user interaction OR 5-minute
      timer, whichever fires first).
    - Recipients actively viewing a message at the moment of burn may
      continue to see it for **up to ~5 minutes** until their app
      re-validates with the server.
    - Recipients **offline** at the time of burn who haven't yet
      fetched the wrapped key: **zero exposure**.
    - Content already screenshotted, copied, or captured outside the
      app: indefinitely retained.

   User-facing framing:
   > *"Burn revokes future access to your past messages. Recipients
   > actively viewing messages at the moment of burn may continue to
   > see them for several minutes until their app re-validates with
   > the server. Recipients who took screenshots, or who saw the
   > messages before burn, retain those copies. Burn does not erase
   > what has already been read."*

   Burned messages render as the original stego'd cover text on
   recipient clients (no "[deleted]" placeholder), so an observer
   scrolling history cannot identify which messages were burned.
   This places a quality bar on Mode 1 stego templates — they must
   read as plausible chat, not just pass automated scanners.
4. **Discord ToS.** Using this tool may violate Discord's Terms of
   Service. Discord may ban your account. Use at your own risk.
5. **Stego stealth: per-message fluent, conversation-level not
   coherent.** Stego output is fluent at the per-message level but
   does not form coherent multi-message conversations. A close
   reader of conversation history can identify the encrypted
   messages don't naturally thread together. This is an
   **architectural constraint, not a design choice**: making stego
   context-dependent (so message N's encoding references N-1) would
   require storing messages on our own server, defeating the
   project's thesis of layering privacy over Discord's existing
   transport. Applies to Mode 1, Mode 2, and Mode 3 stego alike.

   Mitigation for users who want narrative cover:

   > *"Mix encrypted (sensitive content) with plaintext (normal
   > chatter) — the overall conversation has real content threaded
   > through, encrypted portions provide private substance, the
   > contrast is plausible (real users don't make every message
   > sensitive)."*

## Password and duress (v1)

**Status: `implemented-unwired` (verified 2026-07-26).** The design below is
built and test-proven in `crates/keystore`, and none of it has a production
call path. `verify_against_record`, `VerifyOutcome::DuressByThreshold`,
`InactivityTimer` and `crates/keystore/src/duress.rs` are referenced only by
the `crates/keystore/src/lib.rs:58-59` re-export and the keystore's own tests.
Neither `apps/osl-hub` nor `crates/ipc` calls them. Do not describe the
10-attempt auto-burn, the 15-minute inactivity timeout, or the duress password
as user-available features.

The app is *designed to* support an optional unlock password and an optional
duress password. Both are UX gates, not part of cryptographic key
derivation. Full spec in
[`design/unlock-and-duress.md`](design/unlock-and-duress.md).

**What the password protects against:**

- Casual hands-on access to a device with the app installed
  (someone pickup-and-tap level access).

**What the password does NOT protect against:**

- Malware on the device that extracts identity keys directly from
  TPM / keychain (the password is not used to derive keys).
- Forensic disk analysis of the device.
- A coercer who knows the duress feature exists and demands
  identification of which password is which (social engineering;
  cryptography cannot fix).

### Forensic-resistance limits (post-duress)

Stripped app is plausibly innocent to **casual inspection**. It is
**NOT** plausibly innocent to forensic disk analysis. Possible
artifacts:

- Binary still contains OPSEC code paths even in stub mode.
- Windows installer cache may retain previous app state.
- File-system journaling may preserve deleted file metadata.
- SSD wear-leveling may preserve actual content of "deleted" files.
- System restore points and shadow copies may snapshot pre-duress
  state.

> *"Duress strip mode protects against casual examination after the
> duress event. It does not protect against forensic disk analysis
> by a sophisticated adversary. If you anticipate forensic
> examination, full physical destruction of the device storage is
> the only reliable answer."*

### Discoverability framing

> *"If a coercer is aware the app supports duress passwords, they may
> demand to know which password is which. The duress feature provides
> cryptographic protection against forced unlock; it does not protect
> against social engineering. Plausible deniability is strongest when
> the coercer doesn't know the feature exists, or when many users
> skip duress setup so 'I don't have one' is credible."*

Practical implication: **do not promote the duress feature
prominently in marketing.** Ubiquity (high adoption among privacy-
conscious users) reduces the plausible-deniability value.

### MITM mitigation after reinstall

When a user reinstalls after duress, identity keys are regenerated.
The key server records a key-rotation event with timestamp
(`last_rotated_at`). Contacts' clients see the rotation flag on
next `GET /v1/pubkeys/:user_id` and prompt the user to verify the
new fingerprint out-of-band before trusting (Signal safety-numbers
pattern). Server cannot enforce verification — recipient-side
decision; the server only surfaces the event. The user-facing copy
lives in [`ONBOARDING.md`](ONBOARDING.md).

## Audit status (v1 alpha vs v1 stable)

**What needs auditing is the scheme that ships.** As of this reconciliation
that is the stateless hybrid `v=3` scheme in `crates/ipc/src/wire_v2.rs`, plus
its attribution defect (master §10 findings 1–2), plus the narrow RN
downgrade-refusal guard in `crates/ipc/src/commands.rs:3284-3320`. The retired
`v=4` Double Ratchet DM path, disabled RN wire-in, and `v=5` sender-keys
construction are audit scope only for a release that turns them on.

The shipping hybrid construction is custom (not built on libsignal). The
disabled sender-keys construction for groups is custom and currently lacks the
device-id remediation needed for one account on two machines.

### v1 alpha ships unaudited

Budget constraints make commissioning a full cryptographic review
before alpha infeasible. **Loud disclosure** is required in
onboarding (see [`ONBOARDING.md`](ONBOARDING.md)) and at the top of
the user-facing readme. Alpha disclosure copy:

> *"This release uses a custom encryption construction that has not
> yet been independently audited. If your threat model is high-
> stakes (legal investigations, targeted surveillance), use Signal,
> Briar, or Cwtch instead. We are working toward a paid third-party
> audit before declaring v1 stable."*

### v1 stable requires audit

Paid third-party cryptographic review of:

- Hybrid PQXDH construction.
- Identity-bundle binding and sender-attribution fixes before any stronger
  authentication claim.
- RN ratchet replacement before `RN_WIRE_IN_ENABLED` can become true.
- Sender-keys construction for groups after signed device ids are carried and
  receiver chains are keyed by `(account, device)`.
- **FS composition** between any enabled pairwise ratchet and sender-key chain
  FS (see "Independent unverified properties" below).

Estimated $40k–$120k for a focused 4-week engagement (Trail of
Bits, NCC Group, Cure53, Quarkslab tier). Sender keys add audit
scope beyond the previous round's estimate; confirm with auditors
before commitment. Fundraising and audit recruitment are tracked as
project-level open items.

### Independent unverified properties (gated behind audit funding)

- **Forward-secrecy composition** between any future enabled pairwise ratchet
  and sender-key chain forward secrecy. v1 makes no such claim today.
  Professional cryptographic verification is gated behind audit funding.
- **Side-channel and constant-time properties** of chain-step and
  message-key derivation paths.
- **Memory-dump bound** for sender keys: stated property is
  "current rotation only" but has not been formally verified.

## Network-layer protection (v1 alpha vs v2.2)

Bundled Mullvad WireGuard (via `boringtun`) and Tor key-server routing
(via `arti-client`) were originally scoped for v1 alpha but have been
**deferred to v2.2**. Reason: those crates pin release-candidate
versions of `x25519-dalek` / `curve25519-dalek` that conflict with
`dryoc`'s curve25519-dalek requirement (and with each other). See
[`../CHANGELOG.md`](../CHANGELOG.md) for the version table.

### v1 alpha network-layer guidance (external VPN)

> *"v1 alpha does not bundle a VPN or Tor. We recommend running
> Mullvad's official app (or another trustworthy VPN) externally on
> your machine while using this app, to keep your real IP address
> hidden from Discord, your ISP, and the key server. The bundled-VPN
> and Tor-routing protections that the design docs describe will
> ship in v2.2."*

Practical impact for alpha users:

- **Discord still sees your real IP** unless you separately run a VPN.
- **The key server** (single-server in alpha; threshold sharing also
  v2.2) sees your real IP unless you separately route via Tor or a
  VPN.
- **The app does not enforce VPN status** in alpha — it cannot detect
  whether your external VPN is up. This is honest disclosure, not a
  silent fallback. If your external VPN drops, your IP is visible to
  Discord, ISP, and key server until you reconnect.
- Whatever cryptographic protections are actually in effect (today: the
  stateless hybrid `v=3` scheme, attachment AEAD, and RN downgrade refusal —
  not the retired v4 Double Ratchet, not enabled RN traffic or sender keys, and
  not cryptographic burn) **are unaffected** by the missing network layer.
  Content confidentiality holds whether or not a VPN is running. The network
  layer only addresses metadata (who connects, from where, when).

### v2.2 brings (per original design)

- Bundled Mullvad WireGuard with kill-switch enforcement.
- arti-Tor client for key-server `.onion` traffic.
- Threshold key sharing across 5 jurisdictions.
- Mullvad-account flow integrated into first-launch.
- Verified Mullvad ToS / API permissions for programmatic config use
  (this prereq moves with the bundled implementation, since v1 alpha
  doesn't programmatically retrieve Mullvad configs).

### Membership-manifest residual risk (sender keys, v1)

> *"Discord could theoretically add fake members to your channel,
> causing your client to encrypt to attacker-controlled keys. This
> is a narrow threat — adversaries capable of manipulating Discord's
> member list typically have many other attack vectors."*

v1 trusts Discord's member list because a cryptographic admin-signed
manifest costs significant UX (channel admins must rotate keys, sign
membership changes, propagate signatures). Manifest deferred to v2;
see roadmap below.

## v1 alpha hard prerequisites

- Verified `WDA_EXCLUDEFROMCAPTURE` propagates from Tauri parent
  HWND to the WebView2 child process on Windows 10 + 11.
- Selector CI green on Discord stable.
- Reproducible build skeleton in place (full multi-sig + transparency
  log can wait until v2.5; the skeleton must ship in alpha so alpha
  binaries are not "trust me" updates).
- **Audit-status disclosure copy in onboarding** (see "Audit status"
  above and `ONBOARDING.md`).
- **External-VPN disclosure copy in onboarding** ("Network-layer
  protection" above; bundled VPN deferred to v2.2).
- Memory monitoring scaffold for future enabled sender-key and RN state.

## v1 stable hard prerequisites

All of the v1 alpha prerequisites, plus:

- Paid third-party cryptographic review per "Audit status" above.
- Vulnerability response runbook (`docs/security/vuln-response.md`)
  written and reviewed.
- Cryptographer-validated test vectors for all custom constructions.
- Constant-time review of all secret-dependent code paths.
- FS-composition verification (formal or expert review sign-off).

## v2 roadmap (privacy-relevant)

- **Server-push burn-event channel**: sub-second burn propagation to
  online recipients, replacing the v1 5-minute polling cycle. Reduces
  burn exposure window for online recipients from ~5 min to
  sub-second.
- **PQ one-time prekeys**: extend ML-KEM coverage from identity-only
  (v1) to per-OPK in the prekey bundle, improving PQ post-compromise
  of the long-term ML-KEM identity key.
- **Independent co-operators** for the 5-jurisdiction threshold
  servers, replacing the v2.2 "5 servers, 1 operator" model.
- **Cryptographic group-membership manifest**: signed by channel
  admin to prevent Discord-side recipient-set manipulation under
  sender keys.
- **Hardware-backed unlock (FIDO2 / YubiKey)** as an alternative to
  PIN unlock; physical key tap required to unlock the keystore.
  Defeats 6-digit-PIN brute-force concerns and shoulder-surfing.
  Slots into v2.4 endpoint-hardening per the original roadmap.

## Open

- **Audit budget.** Estimate $40k–$120k for a focused 4-week
  engagement on the shipping custom hybrid construction plus any release that
  turns on RN or sender keys (Trail of Bits, NCC Group, Cure53, Quarkslab
  tier). Sender keys add audit scope beyond the previous round's estimate;
  confirm with auditors before commitment.
- **Funding model** for ongoing operations across 5 jurisdictions in
  v2.2.
- **Developer-side legal review** of distributing a tool that defeats
  Discord's automated content scanning. Recommended before public
  release.
