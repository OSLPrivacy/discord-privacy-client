# Threat Model

Status: Living document. Updated as features ship.

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
| Discord reads message contents (DM) | Strong | PQXDH + Double Ratchet, hybrid X25519 + ML-KEM-768 |
| Discord reads message contents (group) | Strong (bounded blast radius) | Sender keys with rotation ≤ 1 h / 500 msgs / membership change / suspicious event |
| Discord reads image / file contents | Strong | Streaming AEAD with per-attachment key wrapped under message-chain key, padded |
| Discord runs CSAM scanning on uploaded images | Defeated | Discord sees random AEAD ciphertext on its CDN |
| Discord traffic-analyzes timing / sizes | Partial | Padding always on; jitter and cover traffic opt-in (v2.1) |
| Future quantum computer + harvested ciphertext | Defeated | ML-KEM-768 hybrid component |
| Server operator subpoenaed (single jurisdiction) | None v1 / Partial v2.2 | Threshold sharing across 5 jurisdictions in v2.2 |
| Server operator compromised across all 5 jurisdictions | Limited v2.2 | "5 servers, 1 operator" model documented honestly |
| User's ISP correlates Discord usage | None v1 alpha / Partial v2.2 | v1 alpha ships **without bundled VPN or Tor** (deferred to v2.2 — dependency conflict). Users run Mullvad's official app or another VPN externally. v2.2 brings bundled WireGuard + Tor key-server routing. See "Network-layer protection (v1 alpha vs v2.2)" below. |
| Phone camera captures screen | None | Document; cannot mitigate |
| Modified client on recipient side | None | Document; cannot mitigate |
| Hardware capture device | None | Document; cannot mitigate |
| Endpoint malware | Partial | TPM seal helps; cannot fully prevent |
| Past group messages outside current rotation window | Strong | Sender-keys forward ratchet; rotation heals compromise within ≤ 1 h or 500 msgs |
| One-time RAM dump of group sender / recipient | Limited | Reveals only messages within the current rotation window; past rotations unrecoverable |
| Casual hands-on access to unlocked device | Strong | Optional unlock password (default 6+ digits, 15-min inactivity timeout, 10 failed attempts → auto-burn) |
| Forced unlock under coercion | Partial | Optional duress password apparent-unlocks then silently burns + strips features. Plausibly innocent to casual inspection, NOT to forensic disk analysis. |
| Forensic disk analysis of stripped device post-duress | None | Stripped state retains OPSEC artifacts (binary code paths, installer cache, FS journaling, SSD wear-leveling, restore points). Documented honestly. |
| MITM via key substitution after reinstall | Recipient-controlled | Server surfaces `last_rotated_at` events; recipient-side fingerprint verification (Signal safety-numbers pattern) |
| Discord adds fake members to your channel | Limited v1 / Partial v2 | v1 trusts Discord's member list; cryptographic admin-signed membership manifest in v2. Narrow threat — see "Membership-manifest residual risk" below. |
| Discord fingerprints modded client | Partial v2.6 | UA / locale / timezone / timer / fonts mitigated; canvas/WebGL not mitigated under WebView2 |
| Build pipeline compromise | Partial v2.5 | Reproducible builds + multi-sig + transparency log |
| Cooperating conversation participant | None | Document |
| User identity correlated with Discord account | Partial v2.3 | Anonymous credentials + optional pseudonymous mode |

## Cryptographic guarantees

- **Confidentiality (DM)**: hybrid PQXDH handshake (X25519 + ML-KEM-768)
  followed by a Double Ratchet for forward secrecy and post-compromise
  security. Per-message symmetric keys via XChaCha20-Poly1305.
- **Confidentiality (group)**: sender keys with mandatory rotation.
  Group encryption uses sender keys with rotation triggered hourly,
  by message count, on membership change, or on suspicious events.
  Within each rotation, a forward-secure ratchet ensures past message
  keys cannot be re-derived from current state. A successful attack
  on past group messages requires either persistent endpoint
  compromise (which defeats all encryption schemes), or a one-time
  RAM dump combined with rapid action before the next rotation.
  Past messages outside the current rotation window remain
  unrecoverable even with full state compromise.
- **Forward secrecy**: ratchet step on every DM; one-way HKDF chain
  step on every group message; compromise of long-term keys does not
  retro-decrypt past content.
- **Post-compromise security (DM)**: receiving a new ratchet message
  after compromise heals the session.
- **Post-compromise security (group)**: bounded-window — replaced by
  mandatory rotation. Healing happens at the next rotation (≤ 1 h,
  ≤ 500 msgs, on membership change, on suspicious event).
- **Post-quantum**: ML-KEM-768 in the hybrid combiner — defeats
  harvest-now-decrypt-later. Construction is secure if either X25519
  or ML-KEM-768 holds; both must break.
- **Revocability ("burn")**: per-message wrapped keys live on the key
  server(s). Burn deletes them. Once deleted, ciphertext on Discord's
  CDN is permanently undecryptable, including against future quantum
  attacks. Burned messages render as their original stego'd cover
  text on recipient clients (no "[deleted]" marker), so an observer
  cannot distinguish burned messages from messages that were always
  cover text.

## Limitations users must understand

1. **Metadata.** Discord still sees who talks to whom, when, and how
   often. This tool cannot hide that. Use Signal/Briar/Cwtch if metadata
   confidentiality is your threat model.
2. **Screenshot resistance, not prevention.** Window capture protection
   (`SetWindowDisplayAffinity` / `WDA_EXCLUDEFROMCAPTURE`) blocks common
   tools (OBS, ShareX, Game Bar, ShadowPlay) but does not stop phone
   cameras, hardware capture, or modified clients.
3. **Burn semantics and exposure window.** Burn revokes future
   decryption of past content. After burn:
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

The app supports an optional unlock password and an optional duress
password. Both are UX gates, not part of cryptographic key
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

The hybrid PQXDH + Double Ratchet construction is custom (not built
on libsignal). The sender-keys construction for groups is custom and
deviates from libsignal's standard pattern (TPM-sealed rotation
root, suspicious-event auto-rotation, recipient-initiated rotation).

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
- Double Ratchet integration (DM).
- Prekey lifecycle.
- Sender-keys construction for groups (TPM-sealed rotation root,
  suspicious-event auto-rotation, recipient-initiated rotation,
  encrypted headers, AD encoding, attachment-key wrapping).
- **FS composition** between pairwise-ratchet FS and sender-key
  chain FS (see "Independent unverified properties" below).

Estimated $40k–$120k for a focused 4-week engagement (Trail of
Bits, NCC Group, Cure53, Quarkslab tier). Sender keys add audit
scope beyond the previous round's estimate; confirm with auditors
before commitment. Fundraising and audit recruitment are tracked as
project-level open items.

### Independent unverified properties (gated behind audit funding)

- **Forward-secrecy composition** between pairwise PQXDH-Double-
  Ratchet forward secrecy and sender-key chain forward secrecy.
  v1 assumes soundness based on intuitive reasoning. Professional
  cryptographic verification is gated behind audit funding.
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
- The cryptographic protections (PQXDH + Double Ratchet, sender keys,
  AEAD, padding, burn semantics) **are unaffected** by the missing
  network layer — content confidentiality and integrity hold whether
  or not a VPN is running. The network layer only addresses metadata
  (who connects, from where, when).

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
- Memory monitoring scaffold (sender-keys cache trip wire, ratchet
  state).

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
  engagement on a custom hybrid PQXDH + Double Ratchet + sender-keys
  construction (Trail of Bits, NCC Group, Cure53, Quarkslab tier).
  Sender keys add audit scope beyond the previous round's estimate;
  confirm with auditors before commitment.
- **Funding model** for ongoing operations across 5 jurisdictions in
  v2.2.
- **Developer-side legal review** of distributing a tool that defeats
  Discord's automated content scanning. Recommended before public
  release.
