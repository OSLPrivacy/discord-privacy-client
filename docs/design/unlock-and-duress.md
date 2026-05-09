# Design: Unlock password + duress password

Status: **Draft. Review gate. v1 feature, not v2.**

## Overview

Two passwords, both optional:

- **Unlock password**: gates app access. Required to launch and use
  the app once configured.
- **Duress password**: when entered, appears to unlock normally but
  silently burns all encrypted data and strips OPSEC features from
  the app, leaving it as a plain Discord webview shell. Designed for
  coercion scenarios.

Setup is offered at first install (skippable) and accessible later
from settings.

**Failed-attempt auto-burn**: after N failed attempts (configurable,
default 10), trigger the same flow as duress.

## Cryptographic role

**Password is a UX gate, not part of identity-key derivation.**

- TPM-sealed identity keys unseal regardless of password input (and
  same for the keychain fallback).
- Password decides which flow runs:
    - Correct unlock password → normal app unlock, full features.
    - Correct duress password → appears to unlock normally,
      immediately triggers silent burn-and-strip in background.
    - Neither match → "incorrect password" error, retry. After the
      threshold of failed attempts, auto-trigger duress flow.

This is a deliberately weaker model than password-derived key
encryption. Documented honestly in
[`../THREAT_MODEL.md`](../THREAT_MODEL.md):

> "Password protects against casual hands-on access. An attacker who
> has compromised the device cryptographically (e.g., malware that
> extracted TPM-unsealed keys) bypasses the password."

Pros:
- Works seamlessly with TPM seal — no per-launch password-derived
  KDF cost.
- Enables duress to apparent-unlock without a per-password key
  derivation that would visibly fail on wrong password.

Cons:
- Malware on a compromised device extracts keys without needing the
  password.
- Forensic disk analysis of a stripped device may recover artifacts
  (see "Forensic-resistance limits" below).

## Password format

- **Minimum 6 digits (numeric).** Not 4 — too easily brute-forced
  and fat-fingered. 6 digits provides ~10⁶ key space, sufficient
  when combined with the failed-attempt auto-burn threshold (default
  10).
- Optionally accept longer alphanumeric passphrases at user choice.
- Both passwords must differ from each other; UI rejects matching at
  setup.
- Confirmation prompt at setup (enter twice).

## Storage

Password hashes stored in the keystore alongside identity keys.

- **Hash function**: Argon2id with parameters tuned to ~250 ms on
  representative hardware (memory: 64 MB, parallelism: 1, iterations:
  tuned by benchmark).
- **TPM-sealed when available**: hash blob sealed under the same TPM
  binding as identity keys.
- **Keychain fallback**: Windows Credential Manager via the `keyring`
  crate when TPM unavailable.
- The hash is **not** a key — it's used only for equality comparison
  on password entry. Forging a hash that matches gains nothing
  cryptographically.

Storing a hash, not the password itself, defends against casual disk
inspection. TPM seal defends against offline brute-force on a
captured disk image. This does **not** defend against an attacker
who has the device and TPM together — see "Cryptographic role"
above.

## When prompted

- On every app launch.
- After 15 minutes of inactivity (re-prompt).
    - Configurable, strong default 15 min.
    - Range: 1 min – 60 min. Below 1 min disabled to avoid lock-loop.
- Inactivity timer source: OS idle (last input device event), not
  app-focus idle, since the user might be focused on another window
  with the app in background. (Open question 7.)

## Setup flow at first install

After Discord login and identity-key generation:

```
Set up unlock password? (Recommended)
Protects your app from casual access by others.

[Set up password]    [Skip for now]
```

If user proceeds:

```
Choose a 6-digit unlock password:
[______]
[______]  (confirm)
[Continue]
```

Then offer duress setup:

```
Set up a duress password? (Optional)

A duress password unlocks the app normally to anyone watching,
but silently:
  • Permanently deletes all your encrypted messages (no undo)
  • Strips privacy features from the app
  • Requires full reinstall to restore privacy features

Use this if you might be forced to unlock under coercion.

[Set up duress password]    [Skip]
```

If user proceeds, show the warning modal first:

```
⚠ Important — please read

Setting up a duress password is a serious decision.

When you enter the duress password:
  • All your encrypted messages are permanently deleted
  • All keys are destroyed — there is no recovery
  • The app's privacy features are stripped — to use them
    again, you must fully uninstall and reinstall
  • The app appears as a normal Discord client to anyone
    watching

This protects you in coercion scenarios but means a single
mistype is permanent. There is no undo.

If you accidentally trigger duress, you will need to:
  1. Uninstall this app
  2. Reinstall it
  3. Re-establish encrypted contact with everyone

Continue setting up duress password?
[Set up]    [Cancel]
```

If "Set up":

```
Choose a 6-digit duress password:
[______]
[______]  (confirm)

⚠ Make this different from your unlock password and easy to
remember in a stressful situation.
⚠ There is no undo. Entering this code permanently destroys
your data.

[Confirm]
```

If user skips at any point, both options remain available later from
settings.

## Settings access

Under "Security" section (or similarly neutral name):

- **Change unlock password** — requires current unlock password.
- **Set up / change duress password** — requires current
  **unlock** password. **Never ask for current duress** — that would
  expose its existence to a watcher.
- **Disable duress password** — requires current unlock password.
- **Inactivity re-prompt timer** — configurable, default 15 min.
- **Failed-attempt auto-burn threshold** — configurable, default 10.

Duress feature in settings labeled neutrally
("Additional unlock options" or "Coercion protection") rather than
something obviously alarming. Brief explainer modal on first visit.

## Duress flow — full specification

When the duress password is entered, OR the failed-attempt threshold
is exceeded:

### Phase 1 — Apparent unlock

App proceeds through standard unlock animation and UI as if unlock
password were correct. User and any watcher sees the app open and
load normally for several seconds. **Phase 2 begins immediately and
runs concurrently with Phase 1; the visible UI does not block on
Phase 2.**

### Phase 2 — Background silent burn (concurrent with Phase 1)

#### Server-side burn

`DELETE /v1/wrapped-keys` with `scope=all`, executed against key
server. Queues with exponential-backoff retry if network unavailable.
**Local strip proceeds regardless of remote burn completion.**

#### Local burn (synchronous, completes before strip)

In order:

1. **TPM-evict all sealed identity-key blobs** (X25519 + ML-KEM-768)
   via `TPM2_EvictControl`.
2. **Delete keychain-fallback identity keys** if present.
3. **Wipe encrypted local cache.**
4. **Wipe anonymous credential token store** (v2.3+).
5. **Wipe prekey state** — own bundles + cached fetched bundles.
6. **Wipe Double Ratchet state** + skipped-message-key cache.
7. **Wipe per-channel sender keys** — sent and received, including
   active `RotationRoot` and chain keys.
8. **Wipe ratchet states for all peers.**
9. **Zeroize all in-memory key material** via `zeroize`.
10. **Wipe stored unlock and duress password hashes.**

### Phase 3 — Strip OPSEC features (NOT uninstall)

Transition app to a "stripped" state:

- Tauri shell remains installed and functional.
- App continues to launch, load `discord.com` webview, function as
  plain Discord client.
- All OPSEC features physically removed from disk:
    - Injection-layer JavaScript scripts deleted.
    - Encryption-module configuration deleted.
    - Settings UI entries for OPSEC features deleted.
    - Window screenshot-protection flag never set.
    - All references to key server removed from local config.

App appears as a generic Tauri-based Discord wrapper, visually
indistinguishable from a normal Discord client.

### Phase 4 — Stripped state behaviour

When the app launches in stripped state:

- No unlock password prompt (password infrastructure removed).
- No encryption toggles in any UI.
- No burn buttons or controls.
- No per-contact protection toggles.
- No settings entries for privacy features.
- No injection layer active — webview is plain Discord.
- No screenshot resistance.
- Visually indistinguishable from any other Tauri-based Discord
  wrapper.

## Implementation requirements for strip mode

- Tauri shell supports two modes at runtime: **full** (with injection
  scripts loaded) and **stub** (no injection).
- Duress flow deletes injection scripts and configuration files that
  the **full** mode requires, causing future launches to fall through
  to **stub** mode.
- Both modes' code paths exist in the binary; **duress doesn't modify
  the binary itself**, only the data files and configuration that
  gate which mode runs.
- All file deletions during duress use **secure-delete** (overwrite
  then unlink) on a best-effort basis. Modern SSDs make this less
  reliable; document honestly.
- TPM eviction is an explicit TPM API call, not a file delete.
- **Strip operations journaled** to a small state file. If the app
  is killed mid-strip, relaunch detects incomplete strip and
  resumes/completes before allowing the UI to load.

## Restoration after duress

Restoration requires **full reinstall**. User must:

1. Uninstall app via Windows Settings → Apps.
2. Re-download installer from official source.
3. Reinstall — generates fresh identity keys, registers new public
   keys with key server.
4. Re-establish encrypted contact with all contacts (handled via the
   key-rotation flow below and in
   [`key-server-api.md`](key-server-api.md)).

## Key rotation on reinstall (MITM mitigation)

When a user reinstalls after duress and registers fresh keys:

1. Fresh identity keypair generated (X25519 + ML-KEM-768).
2. Server registration creates a new entry for `user_id` with new
   public keys. Server requires Discord OAuth proof of `user_id`
   ownership.
3. Key server flags this `user_id` as having undergone a
   key-rotation event with timestamp (`last_rotated_at`).
4. Contacts' clients on next `GET /v1/pubkeys/:user_id` see the new
   keys and the bumped `last_rotated_at`, treat this as a rotation
   event, and prompt:

```
⚠ Liam re-registered new keys on [date].
Their previous keys are no longer valid.

[Trust new keys]    [Verify fingerprint first]
```

5. "Verify fingerprint" surfaces the new key fingerprint and asks
   the user to confirm via an out-of-band channel before trusting.
6. Standard MITM-prevention pattern (equivalent to Signal's safety
   numbers).
7. **Server cannot enforce verification** — recipient-side decision;
   the server only surfaces the rotation event.

Document in user-facing onboarding (`docs/ONBOARDING.md`):

> *"If a contact reinstalls, verify their new keys before trusting —
> this protects against an attacker substituting their own keys."*

## Edge cases

| Case | Behaviour |
| --- | --- |
| User forgets duress password and types it accidentally | Burn happens, no undo. Setup warning is the only mitigation. |
| User mistypes duress under normal circumstances | Same. Real risk; warning is the mitigation. |
| Burn-in-progress when network drops | Local burn completes synchronously, server burn queues with retry. App enters stripped state regardless of remote burn completion. |
| Duress entered while in shared Discord call | Burn proceeds; Discord call continues per Discord's normal behaviour. Strip takes effect on next reload. |
| Multiple wrong password attempts | After N failed attempts (default 10), trigger auto-burn-and-strip. Same effect as duress password. |
| App relaunched mid-strip | Strip operations journaled; relaunch detects incomplete strip and resumes before allowing UI to load. |
| OS-level cloud backup of app data directory | Out of scope; app cannot prevent OS-level backups. Onboarding docs warn the user. |

## Forensic-resistance limits — document honestly

Stripped app is **plausibly innocent to casual inspection** (coercer
opens app, sees plain Discord client, finds no OPSEC features in UI
or settings).

Stripped app is **NOT plausibly innocent to forensic disk analysis**.
Possible artifacts:

- Binary still contains OPSEC code paths even in stub mode.
- Windows installer cache may retain previous app state.
- File-system journaling may preserve deleted file metadata.
- SSD wear-leveling may preserve actual content of "deleted" files.
- System restore points and shadow copies may snapshot pre-duress
  state.

Honest framing in onboarding:

> *"Duress strip mode protects against casual examination after the
> duress event. It does not protect against forensic disk analysis
> by a sophisticated adversary. If you anticipate forensic
> examination, full physical destruction of the device storage is
> the only reliable answer."*

## Discoverability framing

> *"If a coercer is aware the app supports duress passwords, they may
> demand to know which password is which. The duress feature provides
> cryptographic protection against forced unlock; it does not protect
> against social engineering. Plausible deniability is strongest when
> the coercer doesn't know the feature exists, or when many users
> skip duress setup so 'I don't have one' is credible."*

This argues for **not promoting duress prominently in marketing** —
ubiquity (high adoption among privacy-conscious users) reduces the
plausible-deniability value.

## Resolved decisions

1. **Argon2id parameter tuning**: target ~250 ms on representative
   hardware (mid-tier 2020-era Windows 10 laptop, 4-core i5).
   Memory cost floor: 64 MB regardless of timing tradeoff (this is
   the GPU-resistance property — do not lower). Confirm sub-1 s
   login is the hard ceiling on weak hardware; if a representative
   weak laptop exceeds 1 s with these parameters, lower iterations
   before lowering memory.
2. **Strip-state detection by a knowing attacker**: confirmed,
   document and do not over-claim. Stripped state is plausible
   against casual inspection only — not against a coercer who knows
   the product (executable name, installer signature, Tauri shell
   still identify the app).
3. **Concurrent duress-then-recovery**: confirmed — **no abort**.
   Phase 2 burn is synchronous and irreversible. Adding an undo
   would defeat the duress code's safety: a coercer who sees a
   panic-cancel knows what just happened.
4. **Multi-device context**: out of scope for v1 (single-device
   per identity). v2+ multi-device must propagate duress burns to
   all devices.
5. **Stripped-mode user-visible indicator**: confirmed — **no
   indicator**. Stub mode must look like a normal Discord client to
   the user too, to preserve plausible deniability under
   observation. Onboarding mitigates the "why doesn't my privacy
   app work anymore?" UX risk by including the note: *"If your app
   stops showing privacy features, you may have entered your duress
   password. Reinstall to restore features."* See
   [`../ONBOARDING.md`](../ONBOARDING.md).
6. **Server-side rate-limiting of re-registrations**: **3 per
   24-hour rolling window, then exponential backoff** (4th attempt
   1 h delay, 5th attempt 4 h delay, 6th and subsequent 24 h delay
   each). Handles legitimate repeated-duress scenarios while
   preventing automation abuse. The actual defence against
   malicious re-registration is contact-side fingerprint
   verification — rate limits are belt-and-suspenders. Discord
   OAuth proof of `user_id` ownership is also required at
   registration time. Full OAuth spec in
   [`auth-flow.md`](auth-flow.md).
7. **Inactivity timer source**: confirmed — **OS idle** (last input
   device event), not app-focus idle. Window-focus idle would
   unlock the app while the user is working in another window with
   the app in the background.
8. **Strip-resource verification at launch**: confirmed — the app
   must verify all full-mode files are absent before falling
   through to stub, and the journal must be checked for incomplete
   strip operations. Crash-mid-strip → relaunch detects incomplete
   strip and resumes / completes before allowing UI to load.

## Remaining open items

- Argon2id parameters confirmed against measured weak-hardware
  performance (open during alpha-build benchmarking).
- Localhost OAuth listener security review (CSRF, port binding,
  TLS-not-applicable rationale) — handed off to
  [`auth-flow.md`](auth-flow.md).

## v2 roadmap

- **Hardware-backed unlock (FIDO2 / YubiKey)** as an alternative to
  PIN. Physical key tap required to unlock keystore. Defeats
  6-digit-PIN brute-force concerns and shoulder-surfing risks.
  Spec'd separately under v2.4 endpoint hardening per the original
  roadmap.

## Review gate

- [ ] Cryptographer review of:
    - Argon2id parameters.
    - Password-storage construction (TPM seal of hash, fallback to
      keychain).
    - Duress wipe completeness — verify no key material survives
      Phase 2.
    - Re-registration flow and key-rotation event semantics.
- [ ] UX review of setup flow, settings labelling, and warning
      copy.
- [ ] Forensic team review (or red-team review) of stripped state
      against artifact retention.
- [ ] Strip-journal crash-safety review.
- [ ] Threat model entry confirmed (`../THREAT_MODEL.md`).

## References

- Argon2id: RFC 9106.
- Signal safety numbers (MITM-mitigation pattern reference).
