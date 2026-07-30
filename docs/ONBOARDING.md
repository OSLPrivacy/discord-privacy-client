# Onboarding

Single source of truth for user-facing copy shown during the first-
launch flow and key-related events. Implementations in `webview/`
should pull from here.

## First-launch sequence

1. **Platform check** — exit if not Windows 10+.
2. **External-VPN reminder** (v1 alpha; bundled Mullvad WireGuard
   setup returns in v2.2). See "Network protection" below.
3. **Discord login.**
4. **App identity setup** — generate hybrid keypair (X25519 + ML-KEM-
   768), seal to TPM, register with key server.
5. **Unlock password setup** (recommended, skippable). See "Password
   setup" below.
6. **Burn password setup** (optional, skippable). See "Burn password
   explanation" below.
7. **Onboarding explainer modal** — what the tool does and does not
   protect against, account-ban risk, screenshot resistance vs
   prevention, view-once limitations.

## Threat-model framing (shown in onboarding modal)

> *"This tool provides strong end-to-end encryption with post-quantum
> security, revocable access, and screen-capture resistance for
> Discord text messages, images, and files. It substantially raises
> the cost of surveillance and casual capture, and protects content
> against most realistic threats — including future quantum computers
> attempting to decrypt today's captured ciphertext. It does not hide
> that you are communicating, when, or with whom — Discord still sees
> that. For threat models where metadata leakage matters (legal
> investigations, targeted surveillance), use a tool designed for
> that threat model, like Signal, Briar, or Cwtch."*

> *"Screenshot resistance blocks common screen-capture tools on
> Windows but does not stop phone cameras, hardware capture devices,
> or modified versions of this app. Burn revokes future decryption
> of past content but does not erase content from devices where it
> has already been viewed."*

> *"Using this tool may violate Discord's Terms of Service. Your
> Discord account could be banned. Use at your own risk."*

See [`THREAT_MODEL.md`](THREAT_MODEL.md) for the full capability
matrix.

## Audit status (shown prominently in onboarding modal)

> *"This release uses a custom encryption construction that has not
> yet been independently audited. If your threat model is high-
> stakes (legal investigations, targeted surveillance), use Signal,
> Briar, or Cwtch instead. We are working toward a paid third-party
> audit before declaring v1 stable."*

Background: the construction was designed against published
specifications (Signal PQXDH, FIPS 203 ML-KEM-768, RFC 9106 Argon2id,
RFC 8439 ChaCha20-Poly1305) and built on audited primitive libraries
(`dryoc`, RustCrypto `ml-kem`, `hkdf`, `sha2`). What has **not**
happened in alpha is third-party review of how those primitives are
composed in our specific construction. That review is the gate for
v1 stable.

Practical guidance to surface at install time:

- **Most users**: alpha is suitable for everyday privacy from casual
  surveillance, Discord's automated content scanning, and
  screenshot-recorder tools.
- **High-stakes users** (journalists with sensitive sources, abuse
  survivors with active threats, lawyers with privileged comms,
  political dissidents): use Signal, Briar, or Cwtch — purpose-built
  for those threat models, with mature audited implementations.

See [`THREAT_MODEL.md`](THREAT_MODEL.md) audit-status section for
the full v1 alpha vs v1 stable prerequisite breakdown.

## Encrypted-message stealth (note shown in onboarding)

> *"Encrypted messages individually look like normal text, but a
> careful reader of your full conversation history may notice the
> encrypted portions don't form a coherent narrative on their own.
> For maximum stealth, mix encrypted messages with regular plaintext
> chat — sensitive content encrypted, normal small talk in the
> clear."*

This is an architectural constraint, not a design choice we can
change: making stego context-dependent across messages would require
storing messages on our own server, defeating the project's thesis
of layering privacy over Discord's social graph. See
[`THREAT_MODEL.md`](THREAT_MODEL.md) "Limitations users must
understand" item 5 for the threat-model entry.

## Encrypted messages and channel members (visibility note)

> *"Encrypted messages appear as plausible-looking text to anyone who
> can see your Discord channels — including users without the app.
> The encryption hides the **content** of your conversation, not the
> **existence** of messages. New channel members who later install
> the app see the same cover text everyone else saw."*

Subtler implication: if a hostile party joins a channel later and
installs the app, they can read the cover text just like everyone
who was always in the channel. They cannot decrypt the underlying
ciphertext (no shared secret with you), but they can read what looks
like normal chat. For users worried about retroactive exposure of
**cover text**: that exposure was already there when the message
was sent. For users who expected "encryption hides our conversation
entirely from anyone else": that is **not** what this tool provides.
Use Signal/Briar/Cwtch if hiding the existence of conversations is
your threat model.

## Network protection (v1 alpha: external; v2.2: bundled)

### v1 alpha — External-VPN reminder (step 2 of first-launch)

```
Before launching the app, please connect to a VPN.
This app uses end-to-end encryption to protect your message
contents, but does not yet hide your IP address from Discord
or our key server. Using an external VPN (Mullvad recommended,
but any reputable VPN works) is strongly suggested for v1 alpha.

[ ] I have a VPN connected and ready
[ ] Continue without VPN (not recommended)

[Continue]
```

Below the checkboxes, in smaller type:

> *"We can't verify your VPN status from inside the app. Please
> confirm yourself."*

The "I have a VPN connected and ready" checkbox is a **self-
attestation**, not a verification. v1 alpha cannot reliably detect
external VPN status, and the app does not pretend otherwise. If a
user checks the box without an active VPN, the app proceeds — the
disclosure is the protection, not enforcement.

The "Continue without VPN" path is allowed but discouraged. Selecting
it does not block onboarding; it does not show a separate warning
modal in v1 alpha (the page-level copy already states the risk).

The "Continue" button is enabled when **either** checkbox is
selected; a user must make an explicit choice (no default
auto-advance).

See [`THREAT_MODEL.md`](THREAT_MODEL.md) "Network-layer protection
(v1 alpha vs v2.2)" for the threat-model framing.

### v2.2 — Bundled Mullvad WireGuard setup (returns when bundled WireGuard ships)

> **Status: v2.2 plan. Not active in v1 alpha.** This section
> documents the original first-launch VPN flow that returns once
> `boringtun` and a self-contained crypto stack are reintroduced
> (see [`../CHANGELOG.md`](../CHANGELOG.md) and
> [`../crates/transport/src/lib.rs`](../crates/transport/src/lib.rs)).

Three clearly-labelled options on the setup screen:

```
Choose how this app routes its network traffic:

(•) Use Mullvad VPN (recommended)
    Enter your Mullvad account number; the app fetches a
    WireGuard config from Mullvad and brings up a tunnel
    scoped to this app's traffic. Discord won't load until
    the VPN is verified up.

( ) Use my own VPN or proxy
    Make sure your VPN is connected before using this app.
    We won't manage it for you, but we recommend it stays on
    while you use this app.

( ) No VPN (not recommended)
    Your real IP address will be visible to Discord, to your
    ISP, and to your key server. Without a VPN, your network
    identity is linked to your encrypted activity. We strongly
    recommend using Mullvad or another VPN.

[Continue]
```

If "No VPN" is selected, a confirmation modal is required:

```
⚠ Continue without VPN?

Your real IP address will be visible to Discord, to your ISP,
and to your key server. Without a VPN, your network identity
is linked to your encrypted activity.

[I understand, continue without VPN]    [Go back]
```

Persistent VPN status indicator in the app's top bar (v2.2):

- **Green** — Mullvad active and verified.
- **Yellow** — User's own VPN selected (we can't verify, just
  trusting).
- **Red** — No VPN. Clickable to revisit setup.

Kill-switch behaviour (v2.2):

- **Mullvad mode**: if WireGuard tunnel drops, immediately block app
  network requests, show "VPN disconnected" overlay until reconnected.
- **User's own VPN**: no enforcement (app cannot detect drops);
  periodic reminder if hours pass without an active session.
- **No VPN mode**: no enforcement; status indicator stays red.

## Password setup

See [`design/unlock-and-duress.md`](design/unlock-and-duress.md) for
the earlier duress design. An isolated keystore engine models that
wipe-and-strip sequence, but the desktop app does not construct or
call it; several steps also require caller-supplied handlers. The
current webview instead offers the narrower Burn password behavior
documented below.

### Unlock password (recommended, optional)

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

Minimum 6 digits. Optionally accept longer alphanumeric passphrases.

## Burn password explanation

```
Burn password

Erases OSL data from this device when entered at sign in.

Current password: [______]
New burn password: [______]
Confirm: [______]

[Set password]    [Not now]
```

The reachable setup screen validates that the new Burn password
differs from the main password. It does not show the legacy duress
warning or promise a covert apparent unlock.

### What Burn does and does not protect against

> *"If you might be forced to unlock OSL under coercion, you can set
> up a Burn password. Entering it at OSL sign in starts cleanup of
> OSL-owned local data and returns the app to first-run setup."*

> *"Burn does not delete original Discord data, Discord history,
> recipient copies, or operating-system backups. Remote identity
> unregister is best effort and is reported separately from local
> cleanup."*

At sign in, a verified Burn password closes OSL-owned service
windows, runs the fixed-root local cleanup, returns to first-run
setup, and reports whether local cleanup completed. The earlier
apparent-unlock and feature-strip sequence remains a draft design.
The current desktop app does not call that `DuressEngine`.

## Post-reinstall key verification

When a contact reinstalls, their identity keys change. The key server
records a key-rotation event. Your client surfaces it on next sync:

```
⚠ Liam re-registered new keys on [date].
Their previous keys are no longer valid.

[Trust new keys]    [Verify fingerprint first]
```

> *"If a contact reinstalls, verify their new keys before trusting —
> this protects against an attacker substituting their own keys."*

To verify a fingerprint:

1. Tap **"Verify fingerprint first"**.
2. Read the displayed fingerprint to your contact via an out-of-
   band channel (in person, secure phone call).
3. Your contact reads their fingerprint as displayed on their
   device.
4. The two must match.

This is equivalent to Signal's safety numbers.

## Cloud backup warning

> *"If your operating system backs up app data to the cloud, those
> backups may persist after local Burn cleanup. Disable cloud backup
> of this app's data directory if this is a concern. On Windows,
> check File History settings and OneDrive folder backup settings
> before enabling a Burn password."*

## License and ToS

This app is licensed under Apache-2.0. Source code is available;
reproducible builds are documented per release.

Using this app may violate Discord's Terms of Service. The user
assumes that risk.
