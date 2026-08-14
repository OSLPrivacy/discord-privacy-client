# OSL security review pack

This is the short claim boundary for a security reviewer. It is not an outside
review: no reviewer is appointed, no scope is commissioned, and no review has
been completed. It covers the public capability registry in
`data/pricing.json` and the public surfaces in
`data/public-surface-manifest.json` as checked on 2026-08-10.

## The claim OSL can support

Current source contains a stateless direct-message encryption construction
that seals each message body with a fresh AES-256-GCM key and wraps that key to
each recipient using X25519 plus ML-KEM-768. The source proof is
`crates/ipc/src/wire_v2.rs:685-760` and
`crates/crypto/src/pqxdh.rs:141-195`. This is a source-level confidentiality
claim, not a claim that a downloadable release or a supported service adapter
works: the site has no verified release, and
`docs/status/support-matrix.json` currently marks every protected adapter
unavailable for a public support claim.

If that exact path completes using the authentic intended peer keys, and the
connected service only passively relays the carrier, it is meant to protect the
selected message body from that service reading it and from a passive recorder
later using a quantum computer to decrypt harvested traffic. It does not hide
the account, participants, destination, timing, frequency, size bucket, or the
fact that an encrypted block was sent.
Authentication remains classical, and current source has open key-substitution
and sender-attribution findings. A reviewer must therefore treat the provider,
the key service, the connected-service account, recipients, other software on
either endpoint, and anyone who can observe traffic as separate adversaries.

## Every public capability name

The table names all 22 entries in the website capability registry. `Beta` here
means source or historical QA evidence only; it does not mean a current release
is proved. `Planned` is an explicit non-claim of present protection.
`Illustration` means a drawing or comparison, not a measurement.

| Registry id | Website capability name | Website status | Review verdict and proof boundary |
| --- | --- | --- | --- |
| `per-message-sealing` | Per-message hybrid sealing | Beta | Source-proved construction at `crates/ipc/src/wire_v2.rs:685-760` and `crates/crypto/src/pqxdh.rs:141-195`; no current-release or adapter claim. |
| `cover-carrier-text` | Cover text carrier | Planned | Not protected now: natural-language cover mode is disabled; `crates/ipc/src/commands.rs:2640-2660` coerces it to a visible encrypted capsule. |
| `protected-text` | Protected text send and read | Beta | Not accepted for the current release: the registry cites historical Discord QA, while `docs/status/support-matrix.json` refuses every current protected-adapter support claim. |
| `image-send` | Encrypted image sending | Planned | Not protected now; no named release proves end-to-end image transport. |
| `file-send` | Non-image file sending | Planned | Not protected now; the current picker is limited to PNG and JPEG and the non-image plaintext finding remains open. |
| `group-protection` | Group chat and server channel protection | Planned | Not protected now; the shipping product constructs direct-message scopes only. |
| `ratchet` | Old-message protection | Planned | Not protected now; the live path is stateless and the ratchet runtime remains disabled. |
| `attachment-privacy-guard` | Attachment Privacy Guard | Planned | Not protected now; the metadata-stripping seam has no production caller. |
| `exposure-warning` | Before-send warning | Planned | Not protected now; the detection code is not called by the shipping app. |
| `expiry` | Timed expiry | Planned | Not protected now; timed deletion is unwired and there is no current cryptographic erasure path. |
| `view-once` | View once | Planned | Not protected now; production wiring and two-party consent proof are incomplete. |
| `burn` | Burn | Planned | Not protected now as a peer action; it must never be read as un-send, provider deletion, screenshot recall, or destruction of every decryption capability. |
| `link-protection` | Link protection | Planned | Not protected now; the registry records no implementation. |
| `scrub-discovery` | Scrub discovery | Planned | Not protected now; provider identity, inventory, media, and completeness are not qualified end to end. |
| `scrub-guided-deletion` | Guided deletion handoff | Planned | Not protected now; discovery is unqualified and the destructive handoff is not connected end to end. |
| `autoscrub` | AutoScrub | Planned | Not protected now; only switched-off interface scaffolding exists. |
| `ai-carrier-text` | AI-generated carrier text | Planned | Not protected now; cloud generation is not end-to-end private and is not offered. |
| `processing-credits` | Processing credits | Planned | Not a present capability and not on sale. |
| `pro-expiry-enforcement` | Automatic Pro expiry | Planned | Not protected now; automatic one-month expiry is not implemented and checkout is paused. |
| `exposure-comparison` | Country exposure comparison | Illustration | A sourced comparison, not a device scan or live protection score. |
| `website-scrub` | Website Scrub | Illustration | A username-only illustration; the website does not inspect browser or local-account data. |
| `product-animations` | Product animations | Illustration | Drawings of intended behavior, not recordings or proof of a build. |

## Other website statements

The static pages make these process and commercial representations, not extra
security capabilities:

- **How it works:** protected messaging is separate from account scanning;
  scanning is limited to explicitly selected accounts or material; files are
  attachments rather than a device-wide sweep; service actions use a polite
  pace; and scan or deletion risk is confirmed before action.
- **Download:** no verified Windows release exists yet; a future release is
  intended to be an unsigned NSIS installer accompanied by a SHA-256 list and
  Minisign signature. Those are future packaging statements, not proof that an
  installer exists now.
- **FAQ:** provider suspension or ban is possible; OSL does not make harmful
  messages safe; AutoScrub is unavailable; Burn excludes external sessions,
  service history, and other retained copies; and Pro's current/future feature
  list is early-access copy rather than proof of each named feature.
- **Pricing and terms:** Pro is intended to cost 5 dollars for one month from
  code entry with no renewal or OSL card storage, but automatic one-month
  expiry is not implemented and checkout is paused. Deletion is separately
  scoped, previewed, confirmed, verified, and reported without equating a
  request with removal.
- **Donate:** donations are represented as funding builds, audits, and
  operations; they do not unlock Pro and a donation receipt is not an
  activation code.
- **Compare:** OSL is a hub over existing accounts, not equivalent to or a
  replacement for a purpose-built private messenger; the page is not a device
  scan and does not show a live protection score.

These are what the website says. This pack does not convert them into
exact-build guarantees where the build evidence does not.

Both public prototypes are simulations. Their banners say that they use
simulated data and perform no real account access, encryption, scanning,
sending, or deletion. Controls and feature names inside those prototypes are
illustrations, not additional current-product claims. In particular, the Hub
Home prototype displays “Your text protection is ready,” nine launch
companions, three sensitive-history findings, a local old-address preview,
42 checks, 11 cleaned links, zero verified removals, and a server-plaintext
retention promise. The existing website audit classifies every one of those
except the prototype's no-network-request statement as unsupported. This pack
does not adopt any of them. The Chats Lab's E2EE messages, calls, Circles,
view-once, expiry, attachment hosting, and other simulated controls are also
future-product illustrations, not build proof.

## Openly not protected

OSL does not protect against:

- a **screen photo** taken with a phone or camera, hardware capture, or a
  recipient making a screenshot, copy, export, recording, or report;
- **malware**, a modified or compromised endpoint, an unlocked device, or an
  attacker who obtains the recipient's long-term secret keys;
- **provider-account risk**: the connected service can observe metadata,
  enforce its rules, change its interface, suspend or ban the account, and
  retain provider copies, exports, or backups;
- a malicious or cooperating recipient who reads, retains, forwards, or
  republishes plaintext;
- targeted investigation, traffic analysis, identity correlation, account
  correlation, destination discovery, timing, frequency, or size leakage;
- key-server substitution of encryption keys or reliable sender attribution
  until the documented open findings are closed and independently reviewed;
- deletion of recipient copies, provider history, exports, backups, or opened
  copies; Burn and expiry are not an un-send promise; or
- present protection for groups, files, images, view once, expiry, Burn,
  Scrub, AutoScrub, AI carrier text, or any connected-service adapter that lacks
  current exact-build proof.

For a user who needs protection from endpoint compromise, a determined
provider, or targeted investigation, the honest answer is to use a
purpose-built private messenger and a separately secured device instead of
relying on OSL.
