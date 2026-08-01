# OSL public claim allowlist

> **This file is the only source the website and app copy may draw from for what OSL
> does.** A capability cannot become `Available` through a copy edit. It becomes
> `Available` when its evidence row here changes — and the evidence row changes only
> when the cited `file:line` (or a named build's runtime artefact) says so.
>
> Created 2026-07-26. Implements master §24 item 5 ("Public-claim allowlist"), and is
> the claim-eligibility half of master §20.2 and §8.2.
> Authority: [`osl-master-decision-2026-07-26.md`](osl-master-decision-2026-07-26.md).
> Status vocabulary: master §0.3. Evidence detail: [`../THREAT_MODEL.md`](../THREAT_MODEL.md).
> Claim-gate source SHA-256: `251c1268e6ee907e77a20d40e22f48a7af6659905b2cbb192a6589f0841e3631`

## How to use it

1. **Find the row.** If there is no row, there is no claim. Do not extrapolate from a
   neighbouring row.
2. **Use the permitted wording verbatim,** or a strictly weaker paraphrase. Strictly
   weaker means: fewer absolutes, narrower scope, more hedging. Never the reverse.
3. **Ship the limitation with the claim** — same page, same section, comparable
   prominence. A limitation in a linked FAQ does not count.
4. **The status label sets the website badge** (master §8.2: `Available`, `Beta`,
   `Experimental`, `Planned`, `Illustration`). The mapping is in the last section.
5. **If evidence is stale, conflicting, or does not name an exact build, the answer is
   `unknown-recheck-required`** — which earns no public claim at all. Not a softened
   claim. No claim.

Verified against worktree `osl-eye-and-features-2026-07-26`, HEAD
`fc6b9830c679032692e812c90639a3838b4b26fd`, dirty. Four other tabs are editing this
tree; re-verify anchors before relying on a row.

---

## A · Claims that are currently eligible

### A1 · Post-quantum message confidentiality

| | |
|---|---|
| **Permitted wording** | "Message contents are encrypted with a hybrid scheme combining X25519 and ML-KEM-768. Breaking it requires breaking both." |
| **Status** | **`Beta`** badge. *Corrected 2026-07-26:* the standard is "does the code do this", and it does — the scheme is live and encrypted sends have landed real cover text in a real Discord conversation. `implemented-unwired` describes a call path, not the cryptography. Not `Available`, because it is proven on QA builds rather than a named release build. |
| **Evidence** | `crates/ipc/src/wire_v2.rs:685-760` (`encrypt_v3`); ML-KEM-768 ciphertext in every recipient slot, `:325-332`, `:731`; hybrid combiner `crates/crypto/src/pqxdh.rs:141-195` |
| **Required alongside** | "This protects contents against future quantum decryption of recorded traffic. It does not make identity verification post-quantum — that is still classical." |

### A2 · Discord cannot read message contents

| | |
|---|---|
| **Permitted wording** | "Discord receives an encrypted block, not your message. It never receives the decryption key." — **CORRECTED 2026-07-27.** The old wording ("receives cover text") is **NOT ELIGIBLE**: `crates/ipc/src/commands.rs:2640-2660` (9-MODE1-RETIRE) disables Mode 1 template stego in V2 as unviable under the PQ-hybrid ~1190-byte wrap leg and silently coerces legacy configs to Mode 0, which emits a visible `DPC0::<base64>` capsule. Confidentiality holds; the *stealth* claim does not. |
| **Status** | **`Beta`** badge, on the same corrected standard as A1. |
| **Evidence** | `crates/ipc/src/wire_v2.rs:685-760`; body sealed under a fresh random AES-256-GCM key, wrapped per recipient |
| **Required alongside** | "Discord still sees who you talk to, when, and how often. We cannot hide that." (metadata exclusion — mandatory on every confidentiality claim) |

### A3 · Local encrypted storage

| | |
|---|---|
| **Permitted wording** | "Decrypted messages cached on your device are encrypted at rest." |
| **Status** | `test-proven-only` → **`Beta`** badge |
| **Evidence** | *Re-anchored 2026-07-27, old citations had drifted.* AEAD: `crates/crypto/src/aead.rs:31`, `:68` (XChaCha20-Poly1305). Fresh nonce + seal: `crates/store/src/cipher.rs:108`. Sealed before write in `MessageStore::put`: `crates/store/src/lib.rs:283-288`. |
| **Required alongside** | "This does not cover attachments. Non-image attachments are currently written to disk unencrypted — see master §10 finding 4." Do not make this claim on any page that also markets attachments. |

### A4 · Open source

| | |
|---|---|
| **Permitted wording** | "Everything OSL ships today is open source. One optional AutoScrub module will be closed source and separately downloaded when it exists." — **tense-corrected 2026-07-27.** The old wording implied the exception already exists; it does not. `open-core-and-large-privacy-boundary` notes private code still has to move to a separate repository *before public release*, and the checklist treats separate installation/consent as unearned. Present-tensing an unbuilt boundary is the same error as a Planned feature in a feature list. |
| **Status** | `verified-live` → **`Available`** badge |
| **Evidence** | Public repository `OSLPrivacy/discord-privacy-client`, Apache-2.0 |
| **Required alongside** | The exception must be stated in the same sentence, per master §7.6 and §7.10 — never as a footnote. |

### A5 · Screenshot resistance

| | |
|---|---|
| **Permitted wording** | "Blocks common screen-capture tools such as OBS, ShareX, Game Bar and Xbox capture." |
| **Status** | `implemented-unwired` → **`Planned`** badge (the API call exists; first-paint ordering is an open critical, master §2 P0-5) |
| **Evidence** | *Re-anchored 2026-07-27, and stronger than previously recorded.* The call is `crates/runtime/src/screenshot.rs:83`, and the result is **read back and required to match exactly** at `:91`/`:97` — it does not assume success. The overlay compositor independently re-reads via `GetWindowDisplayAffinity` before treating exclusion as proven (`apps/osl-hub/src/native_discord_overlay.rs:4875`), and non-QA builds select `ScreenshotProtection::On` at `:176`. Stays **Planned** only because first-paint ordering is an open critical (master §2 P0-5). |
| **Required alongside** | "It cannot stop a phone camera, a hardware capture device, or a modified client. On machines where Windows refuses the protection it does nothing — and we show you when that happens." Never write "screenshot-proof" or "prevents screenshots". Master §3b: this is the feature most likely to be over-read; keep the copy narrow. |

### A6 · No stored payment data

| | |
|---|---|
| **Permitted wording** | "$5 for one month. Nothing renews, nothing to cancel, and we never store your payment details." |
| **Status** | `verified-live` → **`Available`** badge, for **these three claims only** |
| **Evidence** | Master §7.14; `keyserver-cf/src/lib/stripe.ts:57` uses `mode=payment`, not a subscription. **Limit of repository evidence (recorded 2026-07-27):** the `$5` figure lives behind an external Stripe price ID and cannot be verified from this checkout — it is owner-attested, not source-verifiable. The "nothing renews" and "nothing stored" halves ARE source-supported. |
| **Required alongside** | Nothing extra. **But:** "your month starts when you enter the code" is **NOT eligible** — see D8. |

### A7 · The key server refuses Discord account numbers

| | |
|---|---|
| **Permitted wording** | "OSL identities are separate from your Discord account, and the key server refuses Discord's numeric account IDs outright." — **NARROWED 2026-07-27.** The old wording ("Nobody can register your Discord identity… not even before you do") is **NOT ELIGIBLE**: it is true only for snowflake-shaped IDs. `register.ts:110` validates `isProtocolId` (bounded UTF-8, no control chars) and `:114` rejects only `isDiscordSnowflake` = `/^[0-9]{17,20}$/` (`validation.ts:18`, `:29`). **Any opaque identifier — including a Discord username or alias — is still first-come-first-served**, and most users read "my Discord identity" as their username. |
| **Status** | `verified-live` → **`Available`** badge |
| **Evidence** | Migration `keyserver-cf/migrations/0029_authoritative_osl_identity.sql` applied to production 2026-07-26 (owner-confirmed against the live migration list). Worker refuses snowflakes at both surfaces: `keyserver-cf/src/endpoints/register.ts:114` and `keyserver-cf/src/endpoints/pubkeys.ts:38` return `400 "Discord identifiers are not OSL identities"` (*re-anchored 2026-07-27*). Quarantine enforced in `keyserver-cf/src/lib/db.ts:366`, `:474`, re-enable guarded at `:407` (`AND identity_lookup_enabled = 1`), re-enable path at `:408-411`. All 111 pre-existing identities start disabled (`identity_lookup_enabled INTEGER NOT NULL DEFAULT 0`). |
| **Required alongside** | "Existing accounts created before this change are switched off until their owner's app re-registers them, which happens automatically the next time they open OSL." *Corrected 2026-07-26:* the previous wording ("until their owner re-registers") implied manual work. `ensure_keyserver_registered` runs on launch **and** unlock, not only for new identities (`crates/ipc/src/commands.rs:7580`, called from `src-tauri/src/bootstrap.rs:1296`, `apps/osl-hub/src/password_lifecycle.rs:384`, `apps/osl-hub/src/core_bridge.rs:306`), and `build_register_request` sends the OSL `user_id` with no snowflake (`crates/keystore/src/client.rs:601`), so a quarantined identity re-enables itself. Do not extend this into a general identity-verification claim — full-bundle identity binding is still an open finding (master §2 P0-1), so this closes account *pre-registration*, not key *substitution*. |
| **Open risk, not this lane's** | `Identity` carries a `discord_snowflake` field populated by `osl_register_self_snowflake`. Any peer-lookup path keyed on a snowflake rather than an OSL user id is now **permanently unresolvable** under 0029. Raised by the crypto lane 2026-07-26; needs an explicit check by whoever owns peer lookup. It does not falsify A7, but it could break finding a contact. |

This is the first row in this file to reach `verified-live` on the strength of a production
deployment. It closes master §2 P0-3 and §10 critical class 3.

### A8 · Local-first processing

| | |
|---|---|
| **Permitted wording** | "Protected-message encryption and decryption run locally. OSL sends ciphertext and public identity keys to its services — never your plaintext or your private keys." — **CORRECTED 2026-07-27.** The old wording was literally false: the client uploads its X25519, Ed25519 and ML-KEM **public** keys on registration (`crates/keystore/src/client.rs:599`), which is necessary and normal, but "keys stay on your machine" does not survive it. |
| **Status** | `implemented-unwired` → **`Planned`** badge |
| **Evidence** | Master §6 product contract; no server-side plaintext path in `crates/ipc` |
| **Required alongside** | If the page also mentions Pro cloud carrier generation or cloud AutoScrub, it must say those send selected data to a server and are not end-to-end private (master §7.1, §7.6). |

### A9 · OSL refuses to send unless the exact carrier is in the composer

| | |
|---|---|
| **Permitted wording** | "Before pressing Send, OSL checks that the composer contains exactly the complete encrypted carrier, in the window it means to send to. If that check fails it withholds Send and keeps your draft." |
| **Status** | `implemented-unwired` → **`Planned`** badge (source is real; no named release build proves it end to end) |
| **Evidence** | `apps/osl-hub/src/native_discord_adapter.rs:16333` calls `await_exact_carrier_in_composer(target, process_is_trusted, scope_binding, &expected, carrier, &focused, …)` — the expected carrier, the focus state and a trusted-process check all gate the send. The overlay clears the draft only after the adapter marks the carrier sent. |
| **Required alongside** | "This proves what was in the composer immediately before Send. It does not prove the service accepted the message afterwards — see C3, where the uncertain outcome is currently shown as failure." |

**Added 2026-07-27** from an adversarial audit that looked for claims OSL had *earned but never made*.
This one is worth having: it is the honest counterweight to C3, and it is the kind of guarantee a
cautious user actually wants — the product refuses to act rather than acting on an assumption.

### A10 · A served key bundle is verified against the peer, not trusted from the server

| | |
|---|---|
| **Permitted wording** | "Before accepting a key bundle handed to it by the key server, OSL verifies the peer's own signature over that bundle." |
| **Status** | `implemented-unwired` → **`Planned`** badge |
| **Evidence** | `crates/keystore/src/client.rs:213`, whose own comment states the principle exactly: *"the keyserver is a carrier for the bundle, not its integrity authority."* |
| **Required alongside** | **Do not read this as identity binding.** It stops the key server altering keys *inside* a bundle. It does **not** stop a substituted bundle under a different identity key, because the safety number binds only Ed25519 (THREAT_MODEL, MITM row) — full-bundle identity binding is still open (master §2 P0-1). Claiming this closes key substitution would be the exact overclaim this file exists to prevent. |

### Candidates NOT added — cited evidence did not check out

Two further suggestions were rejected on verification, recorded so nobody re-derives them:

- **Encrypted attachment cache.** Cited `crates/store/src/lib.rs:489` as sealing attachment metadata
  and body separately; that line is burn-ordering logic, not sealing. **Unverified — no row.**
- **Encrypted local message metadata / blind indexes.** Plausible and likely true after schema v4, but
  it belongs to the store lane's current work and needs its own verification against the v4 migration
  before it earns a row. **Unknown, which is a better answer than a guess.**

---

## B · Claims eligible only with an explicit "not yet" framing

**Superseded in part by the owner reframe below (2026-07-26).** These may now be described on a
*marketing* page as part of the v1 product, in forward-looking language, with a link to the support
matrix. What they still may **not** do: appear in a checkout purchase summary, appear on the support
matrix as anything other than their real status, or appear anywhere as a bald present-tense claim
that the shipping app already does them. The mandatory framing column below is the wording to use
when the distinction matters.

| Claim | Status | Evidence | Mandatory framing |
|---|---|---|---|
| Forward secrecy | `implemented-unwired` | `osl-ratchet-next` built; `crates/ipc/src/wire_rn.rs` has no consumer — only the `crates/ipc/src/lib.rs:76` module declaration | "Planned. Not in the current release." |
| Post-compromise security | `implemented-unwired` | same; the Double Ratchet DM path is dead at `crates/ipc/src/commands.rs:2842` | "Planned. Not in the current release." |
| Group sender keys / bounded blast radius | `implemented-unwired` | gated off at `crates/ipc/src/commands.rs:2938`; default documented false at `crates/ipc/src/state.rs:280` | "Planned. Group messages currently use the same scheme as direct messages." |
| Unlock password, 15-minute auto-lock, 10-attempt auto-burn, duress password | `implemented-unwired` | `crates/keystore/src/password.rs:62`, `:283-288`; `crates/keystore/src/duress.rs` — zero production callers, only the `crates/keystore/src/lib.rs:58-59` re-export and keystore tests | "Planned." Do not list under device security. Master §7.14-adjacent copy must not imply the app locks itself. |
| Bilateral burn | `implemented-unwired`, plus an open defect | `apps/osl-hub/src/broker.rs` retains authenticated revocation notices and returns `EnforcementUnavailable` rather than falsely acknowledging an unenforced burn; `apps/osl-hub/src/security.rs::admit_peer_content_seq` remains deliberately unwired pending T1/T2's authenticated content-envelope protocol change. | "Planned. A peer burn request is not proof that another device deleted its copy." |
| View-once, timed deletion, attachments | `implemented-unwired`; attachment transport and recovery remain unproved on a named release build | master §9 rows. Historical source-unbound Worker `0a17547d` proved only a known-length part upload returned 201. Exact recovery `3938a73` is `runtime-proven` locally; exact release contract `1e9e635` is `test-proven-only`. Migration `0010` is unapplied and its matching Worker is inactive, so production wrong-size/abandoned recovery and quota release are `unknown`. | "Planned." Master §8.2 forbids marketing these as available before the exact release build has evidence. The intended property—that Discord receives ciphertext/decoy material rather than the protected attachment—is unproved on a release build. Treat earlier attachment-success reports as unproven unless they bind the exact app and Worker source, migration state, and runtime evidence. |
| Signal, WhatsApp, Telegram, Outlook support | `designed-only` / `externally-blocked` | master §7.8, §9 | Must carry `Coming soon`, `Experimental`, or `Externally blocked`. Master §7.8: a logo does not imply support unless the support matrix says so. Outlook is scoped as OSL Mail, a separate staged program, not Outlook chat support. |
| Scrub discovery ("find the accounts you left behind") | `unknown-recheck-required` → published as `Planned` | Master §9 "active dirty integration work"; no evidence names an exact build; the last live report (2026-07-25) was that pressing "do scrub" detected **0 accounts**, because browser import never creates an `AccountRecord` or hands anything to Scrub | "Planned. Not in the shipping app yet." Describe the scan as *designed to* stay on the device, not as something it does today. Never publish a finding count, real or illustrative, as if it came from a real scan. |
| Scrub guided deletion handoff | `implemented-unwired` → `Planned` | **Weaker than previously recorded (2026-07-27 audit).** Not merely blocked on discovery: the Discord guided-deletion backend is itself unwired on its input half — `apps/osl-hub/src/native_discord_adapter.rs:6662` leaves focus, menu opening, delete selection, activation, confirmation and verification unperformed, and holds every candidate. | "Planned." The user-confirms-on-the-service-page rule is a **design rule, not an earned capability** — do not phrase it as something OSL does today. Must always say OSL does **not** erase anything for you. Master §7.5: requested deletion is never shown as verified deletion. |
| AutoScrub | `designed-only` → `Planned` | **Downgraded from implemented-unwired 2026-07-27.** The repository contains design and **disabled UI scaffolding**, not an execution backend or a packaged module: the UI itself says "Coming soon" / "Unavailable in this build" (`apps/osl-hub-ui/src/main.ts:3139`), and no separately downloadable module exists. | "Planned." Must state the closed-source exception in the same sentence (master §7.6, §7.10), that it is **not installed by default**, and that cloud AutoScrub is not end-to-end private. |
| Link/tracker protection | `designed-only` → `Planned` | No implementation found in the tree: there is no link or tracker crate and no tracking-parameter handling | "Planned." This is the weakest row in the file — it is a design intention with no code behind it. Do not imply any link is being cleaned today. |
| Before-send exposure warning | `implemented-unwired` → `Planned` | `crates/exposure-warning` has **no dependent at all**; root `Cargo.toml:12-16` documents it as unwired | "Planned." Master §7.12 requires neutral consequence language, never moral or political judgement. Do not list under device security. |
| AI-generated carrier text | `designed-only` → `Planned` | Master §7.1 opt-in feature, gated behind processing credits that cannot be bought | "Planned." Cloud generation may **never** be called end-to-end encrypted (master §7.1 forbids it directly — the service sees the context). Local processing is the privacy-preferred option. |
| Processing credits | `designed-only` → `Planned` | No purchase or balance endpoint exists on the deployed keyserver | "Planned, and not on sale." Master §7.14: may be explained but not sold until the ledger provably excludes message text, conversation names, carrier text and recipient identity. |

**Illustration rows.** Two things on the site are drawings, not measurements, and carry the
`Illustration` badge: the homepage country-keyed exposure comparison (a retrospective comparison of
published agency and company practice — master §8.4 forbids any *live* protection score on the
website) and every animated product scene on the homepage, features and download pages (a drawing
of intended behaviour, not a recording of the app). Both need equivalent explanatory text and an
accessible name per master §8.6.

---

## C · The eye and the send path — narrow rows, easy to overclaim

| Claim | Permitted wording | Status | Evidence | Required alongside |
|---|---|---|---|---|
| Encrypted send reaches Discord | "Encrypted messages send through Discord." **"as ordinary-looking text" removed 2026-07-27** — Mode 1 is disabled, so what is sent is a visible `DPC0::` capsule. | `runtime-proven` → **`Beta`** | Dated QA screenshot evidence, master §9 "Discord protected send" | "Verified on QA builds, not yet on the release build." |
| Decrypted overlay ("the eye") | "On the cited QA build, OSL decrypted three Discord rows and computed valid overlay placement rectangles for all three." — **CORRECTED 2026-07-27.** "Paints" is not eligible: `placedRowCount` is computed at `apps/osl-hub/src/main.rs:3223` by filtering rows that have both plaintext and geometry, **before the renderer has visibly painted anything**. Geometry computed is not pixels drawn; a screenshot or renderer-output artefact is still required. | `runtime-proven` → **`Beta`** | `osl-rehydrate-geometry-diagnosis.md`: `placedRowCount` 0→3, `rel_l` −8→0, executable SHA-256 `6b6a36945b42…` | "Measured on a QA build. Placement is proven; the full visual result, peer-authored rows, scrolling, resizing and DPI changes are not." |
| Delivery confirmation | "OSL tells you whether Discord accepted the message — sent, not sent, or uncertain." | `designed-only` | Master §7.3 tri-state contract | Do not claim until the tri-state proof exists on an exact build. `delivery_uncertain` must never be described as failure. **SAFETY FINDING 2026-07-27, not just a claim limit:** the shipping renderer does not model "uncertain" **at all** — `grep uncertain apps/osl-hub-ui/src/overlay.ts` returns nothing. An ambiguous Enter is reported failure-shaped (`overlay.ts:1603`, "Discord did not receive this message") and the draft is deliberately preserved so the user can retry. The code comment concedes the command "can return Ok(...) even when Discord never got it". So the exact case the contract calls uncertain is presented as failure **and** the user is invited to resend — which sends a private message twice if Discord did accept the first. Master §7.3 and the standing rule that `delivery_uncertain` is never auto-retried are both violated in behaviour. Owner: the hub/overlay lane; this lane does not edit `overlay.ts`. |

---

## D · NOT ELIGIBLE — these phrases may not appear anywhere

Website, in-app copy, README, store listing, social posts, screenshots, alt text.
Each is listed with why, so nobody re-derives it and reintroduces the phrase.

<!-- forbidden_support_phrases -->

| Forbidden phrase | Why it is forbidden |
|---|---|
| **"Better than Signal"** | Explicitly barred by master §7.11: no public "Signal protocol", "better than Signal", forward-secrecy, post-compromise or post-quantum-authentication claim may outrun the audited live path. Nothing has been audited, and Signal has forward secrecy and post-compromise security that OSL currently does not. The claim is false today, not merely unproven. Master §8.4 also requires OSL's disadvantages to appear with the same prominence as competitors'. |
| **"Post-quantum authentication"** | Factually wrong. ML-KEM-768 provides *confidentiality* only (`crates/ipc/src/wire_v2.rs:731`). Sender authentication is classical X25519/Ed25519. Additionally, attribution is an open critical finding (master §10 finding 2) — the receive path authenticates one key and attributes the plaintext to a caller-supplied identity. So authentication is neither post-quantum nor currently sound. |
| **"Cryptographic burn"** | Burn is policy and state deletion, not destruction of decryption capability. `MessageStore::put` (`crates/store/src/lib.rs:194-206`) never writes the `wrapped_key` column, so the burn paths that set `wrapped_key = NULL` (`:298`, `:342`, `:350`, `:560`) null a column that was already null. Burn zeroblobs the *local* cached ciphertext — real, and sayable — but the ciphertext on Discord's CDN stays decryptable by any holder of the recipient's long-term keys. |
| **"Destroys keys, not messages"** | Exactly inverted. There is no key to destroy: `MessageStore::put` (`crates/store/src/lib.rs:194-206`) never populates `wrapped_key`, so every burn path that sets `wrapped_key = NULL` nulls a column that was already null. Burn destroys *messages* — OSL's local copies of them — and destroys no keys at all. This phrasing was on `README.md:78`, the repository's public front page, until 2026-07-26. |
| **"Permanent ciphertext" / "permanent gibberish" / "mathematically opaque"** | Asserts the carrier becomes undecryptable after a burn. It does not. v3 seals to the recipient's *long-term* keys (`crates/ipc/src/wire_v2.rs:722-729`, no one-time prekeys), so anyone holding that key material can still read the ciphertext the connected service retains — burning your own copy changes nothing about theirs. Comes from design documents describing the per-message wrapped-key model, which is **designed and deliberately not built** (owner decision 2026-07-26). |
| **"Disappears forever" / "permanently undecryptable" / "gone for good"** | Same root cause as above, plus: v3 wraps to *long-term* recipient keys with no one-time prekeys (`crates/ipc/src/wire_v2.rs:722-729`), so a recipient's keys decrypt their entire history indefinitely. Also unfixable in principle for screenshots and copies — master §8.4 requires Burn copy to separate local erasure, cooperative peer request, host-platform deletion attempt, and unpreventable copies. |
| **"Burn makes messages unrecoverable" / "Burn removes recipient copies"** | Burn may remove local OSL state and may send a cooperative request. It cannot prove recipient-device deletion, screenshot removal, platform backup deletion, or that retained carrier ciphertext is unreadable to any holder of the long-term recipient keys. |
| **"Burn deletes provider messages" / "Burn removes provider messages" / "Burn deletes Discord messages" / "Burn removes Discord messages" / "Burn unsends messages"** | Burn deletes OSL's local state and may request cooperative cleanup where consent, binding, authority and verification exist. It does not prove provider deletion, does not un-send native-service messages and must not display a cleanup request as verified deletion. |
| **"Works on Gmail / Discord"** as a general capability claim | Discord is the reference adapter and is `runtime-proven` on QA builds only — see section C. Gmail has no adapter at all; Outlook is scoped as OSL Mail, a separate staged program (master §7.8, §9). Naming a service you have not qualified is exactly the "logo implies support" failure master §8.6 prohibits. Per-service claims must come from the versioned support matrix (master §8.4), not from this general form. |
| **"Works on Signal" / "Works on WhatsApp" / "Works on Telegram" / "Works on Outlook" / "Signal support" / "WhatsApp support" / "Telegram support" / "Outlook support" / "OSL Mail support"** | These are support claims without the exact status, scope, and limitation. Signal and WhatsApp need separate exact-build proof, Telegram is externally blocked until the signed-client row probe clears, and Outlook is only a future OSL Mail program unless a separate mailbox-and-recipient evidence row promotes it. |
| **"Supports Gmail" / "Supports Discord" / "Supports Signal" / "Supports WhatsApp" / "Supports Telegram" / "Supports Outlook"** | A generic `supports` phrase hides the capability boundary: protected send, protected receive, attachments, deletion, browser/email import, and account lifecycle each need their own row. Discord has only narrow QA-build evidence; the others are not generally available. |
| **"Available on Gmail" / "Available on Discord" / "Available on Signal" / "Available on WhatsApp" / "Available on Telegram" / "Available on Outlook"** | `Available` is a badge reserved for `verified-live` rows. None of these service-level claims has that status in the support matrix. |
| **"Signal support is available" / "WhatsApp support is available" / "OSL supports Signal" / "OSL supports WhatsApp" / "works on Signal" / "works on WhatsApp"** | Signal and WhatsApp have signed profile evidence rows, but the public support matrix still marks both `coming_soon` with `claim_allowed: false`. Public app-support wording must follow the exact versioned matrix row, not the existence of a profile, logo, prototype or test fixture. |
| **"Provider-tested" / "verified by Discord" / "works with Discord's approval"** | No provider has tested, reviewed, or approved anything. The opposite is closer to true: master §5 and THREAT_MODEL "Discord ToS" note that using OSL may violate Discord's Terms of Service and may get the account banned. Implying provider sanction is both false and harmful to users making a risk decision. |
| **"Audited" / "independently verified" / broad security "reviewed" claims** | No third-party cryptographic audit has been commissioned. THREAT_MODEL "Audit status" requires the *opposite* disclosure in onboarding: the construction is custom and unaudited. The narrow b90 exception is eligible only as: "A narrow SESSION_RESET ratchet remediation was independently reviewed and signed off." It must be paired with: "This was source review of one remediation, not a third-party cryptographic audit of OSL." Evidence: `docs/reports/ratchet-lane-2026-07-26.md` section "Reviewer Sign-Off: b15-b20 Findings Closed", `apps/osl-hub/tests/ratchet_lane_signoff_b36.rs`, and `crates/ipc/src/commands.rs` test `remediate_independent_review_findings`. Do not turn that into a provider, outside-firm, independent-verification, penetration-test, or product-wide audit claim. |
| **"Military-grade" / "unbreakable" / "NSA-proof"** | Meaningless or false. THREAT_MODEL "Out of scope": OSL is explicitly *not* intended to resist targeted federal investigation, and points such users to Signal, Briar or Cwtch. |
| **"Screenshot-proof" / "prevents screenshots"** | Overstates A5. Capture protection is a platform affordance that can silently fail and cannot stop a camera. |
| **"Discord attachment scanning defeated"** / **"defeats Discord attachment scanning"** / **"Discord cannot scan attachments"** / **"Discord sees only decoys"** / **"Discord's attachment scanner is defeated by OSL"** / **"OSL bypasses Discord's attachment inspection"** / **"Discord receives harmless cover files instead of the attachment"** / **"Uploaded files are opaque to Discord's scanners"** | These assert the intended ciphertext/decoy transport property as a shipping fact. Attachment transport is still `Planned`; no named release build proves the app-to-Discord path, and migration `0010` plus its matching recovery Worker are inactive. A historical source-unbound Worker returning 201 for one known-length part upload does not establish what Discord received or what the release app can send. Equivalent claims that Discord inspection is *defeated, solved, bypassed, neutralized, blocked, evaded, thwarted, circumvented, prevented,* or *rendered ineffective* are also ineligible, as are claims that Discord receives only a *cover, placeholder, stand-in, dummy, surrogate, fake,* or *decoy*, or that the real upload is *opaque, unreadable,* or reveals nothing to its scanner. |
| **"End-to-end encrypted" applied to Pro cloud carrier generation or cloud AutoScrub** | Master §7.1 forbids it directly: do not describe it as end-to-end encrypted if the service can see the context. |
| **"Your month starts when you enter the code"** | Master §7.14: the deployed implementation grants **lifetime** access on `checkout.session.completed`, and no redemption timestamp exists anywhere. Eligible only after that is fixed. The price, "nothing renews", and "nothing stored" *are* eligible today (A6). |
| **"Anti-spyware" / "malware detection"** | Master §8.4: remains a research concept. Call it a privacy-posture or risk monitor, and keep the compromised-device exclusion. |
| **Any live "protection score" on the website** | Master §8.4: the website score is a retrospective default-settings comparison, not a live device scan. Only the installed app may show a timestamped live score, and it must list every point of failure. |

---

## E · Status → website badge mapping

| Master §0.3 status | Website badge (§8.2) | May appear in a feature list? |
|---|---|---|
| `verified-live` | `Available` | Yes |
| `runtime-proven` | `Beta` | Yes, with the scope limit stated |
| `test-proven-only` | `Beta` | Yes, with the scope limit stated |
| `implemented-unwired` | `Planned` | No — roadmap only |
| `designed-only` | `Planned` | No — roadmap only |
| `externally-blocked` | `Externally blocked` | No |
| `open-security-finding` | **no badge, no claim** | No |
| `unknown-recheck-required` | **no badge, no claim** | No |
| `superseded` | — | Never |

Two rules that follow from the table and are easy to get wrong:

- **`open-security-finding` outranks everything else on the same row.** A feature can be
  fully built, runtime-proven, and still earn no claim because a finding blocks it.
  Identity/trust and live v3 crypto are both in this state (master §9).
- **A claim assembled from two eligible rows is not automatically eligible.** "Encrypted
  and it disappears" is not A2 + burn; it is a new claim, and it is false.

## F0 · The claim gates cannot catch unreachable code — a class, not a bug

**Recorded 2026-07-27**, after the crypto lane's sweep found **17 subsystems with zero production
callers** (`docs/reports/crypto-lane-2026-07-26.md`), among them encrypted Notes, a chunked asset
vault, LAN collaboration, a plugin sandbox, the duress wipe engine, signed burn alerts, keyserver
wrapped-key deletion and timed-message expiry.

**`check-app-claims.mjs` is a STRING gate and cannot catch this class by construction.** It answers
one question: does this text contain a §D phrase? `osl_notes` has UI command strings that are
*entirely truthful sentences* with no backend command registered anywhere. The gate reads them, finds
nothing forbidden, and passes — correctly, on its own terms. The sentence is not a lie about what the
code does; it is a true sentence about code that is not wired, and no property of the string
distinguishes the two.

Bolting a narrow reachability check onto the string gate would cover almost nothing — registry ids
are not command names, and most UI strings carry no capability marker — while making the gate *look*
complete. **That is the false-confidence failure the gates exist to prevent**, so it was not done.

**Where the class can be caught:** a reachability sweep over `generate_handler!` and the call graph,
which is a different gate with a different input, and which the crypto lane has now built once by
hand. It belongs with whoever owns app Rust.

**Independent truth-lane cross-check of the four hand-verified findings (2026-07-27).**

| Finding | Reachability evidence | Human claim surface | Triage |
|---|---|---|---|
| `post_wrapped_key` | Definition at `crates/keystore/src/client.rs:805`; current `git grep` finds callers only in keystore tests, including the explicitly named live smoke test—not in production code. | Internal keyserver/design material describes the model and the threat model explicitly says the send path never built it. No user-facing sentence says production messages upload wrapped keys. | **INTERNAL ONLY** |
| `fetch_wrapped_key` | Definition at `crates/keystore/src/client.rs:782`; current `git grep` likewise finds callers only in keystore tests. | Same. No user-facing sentence says production receive fetches a wrapped key. | **INTERNAL ONLY** |
| `BurnAlertPayload` / sign / verify | Definitions at `crates/keystore/src/burn_alert.rs:34,75,83`; outside that file the only source hit is the re-export at `crates/keystore/src/lib.rs:44`. | Commit `dae12da` removed README's present-tense “signed burn notice is also sent” promise. `README.md:85-87` now says the peer-notification path is not end-to-end proved, is not available as a working peer action today, and must not be relied on to remove another member's copy. The newer, separate authenticated `0x0A` revocation path exists in `broker.rs`; it still does not call this signature layer. | **UNCLAIMED after README correction** |
| `osl_notes` / `osl_assets` / `osl_lan` wiring | None of the modules is declared in `apps/osl-hub/src/lib.rs:1-80`; none of their command names appears in `generate_handler!` at `apps/osl-hub/src/main.rs:7207-7336`; their TypeScript wrappers have no production UI consumer. | The app is honest: `main.ts:2555` sets Notes `available: false` and `:5117` says it is planned. Website terms had said Pro expiry leaves “your … notes” unaffected; that present-tense leak is now removed from both the manifest disclosure and `docs/terms.html`. | **INTERNAL ONLY after website correction** |

The website baseline is the negative control, not supporting proof:
`node scripts/pricing-sync.mjs --check`, `node scripts/build-status.mjs --check`, and
`node scripts/check-claims.mjs` all exited 0 (16 pages, 0 failed) while the Notes and peer-Burn
leaks were present. The gates behaved according to their declared string/status scope; they did
not and cannot establish reachability.

**Scope of this adjudication:** it covers only the four entries the crypto lane had already
hand-verified, now re-checked against current source and the current website worktree. The other
thirteen remain unverified counts and must be re-grepped before anyone acts on them.

## F1 · Structural gap — two lists that must agree, with nothing enforcing it

**Recorded 2026-07-27.** Capability statuses exist in **two places**: the rows in this file, and
`capability_registry` in `data/pricing.json` in the website repository. `check-claims` verifies that
every page badge matches **the manifest**. Nothing verifies that the manifest matches **this file**.

So the manifest could drift from the allowlist and *both* gates would still pass — a page badge would
agree with a registry entry that no longer agrees with the claim that authorises it. This is exactly
the duplication that was designed out for §D banned phrases, where the app gate **parses this
document** rather than keeping a copy. Statuses were not given the same treatment because the two
files live in different repositories and a cross-repository path dependency is its own fragility.

**Checked by hand 2026-07-27 and they currently agree:** manifest has `per-message-sealing` and
`protected-text` at `Beta`, everything else `Planned` or `Illustration`; this file has A1/A2/A3 at
`Beta`, A4/A6/A7 at `Available` for claims with no registry capability behind them, and the rest
`Planned`. **A hand check is not a mechanism.** Until one exists, treat a status change here as
requiring a matching manifest edit in the same task, and vice versa.

## F · Maintenance

Update this file in the same task as any change to security truth or feature status
(master §20.2). When a row changes, also update master §9, the internal checklist, and
any page currently using the old wording — a row that no longer earns its badge is a
live incorrect claim on the site, not a documentation backlog item.

### Latent claim surface in the app — NOT live, but one wire-up away (recorded 2026-07-27)

`apps/osl-hub/src/core_bridge.rs` builds a feature list whose labels include **"Group and server
 encryption"**, **"Encrypted images and attachments"** and **"Ciphertext-only relay"**. Group
protection is switched off, and attachment ciphertext/decoy transport remains unproved on a named
release build, so as user-facing capability lines those would be false.

**They are not currently false, because nothing shows them.** Traced before judging severity:
`list_core_features` is defined and registered in `generate_handler!`, but **no UI code invokes it**;
`parseCoreFeatures` is referenced only by `core.test.ts`; and the one path that would populate the
list, `loadCoreIntegrationFromNative`, invokes only `get_core_readiness` and hardcodes
`features: []`. This is the caller-before-callee check: a capability that looks live is dead, and a
capability that looks dead can be live.

**Why it still matters.** The moment anyone wires that command to a view, the app ships capability
lines for cryptography that is not running — with no gate to stop it, because the app-copy gate reads
*string literals*, and these are already string literals that simply never render. Two conditions
before it is ever wired: the labels must carry §8.2 status vocabulary, and `bridge_state` must stop
being the honesty channel — its values (`source-linked`, `guarded`, `refactor-required`,
`shell-adapter-required`) are engineering states that tell a user nothing about whether the feature
protects them. Owner: whoever owns `core_bridge.rs`; not this lane, which does not edit app Rust.

### In-app copy gate — CLOSED 2026-07-26 (was the gap recorded earlier the same day)

Section D says it governs "Website, in-app copy, README, store listing, social posts, screenshots,
alt text". Only **one** of those is mechanically enforced. `scripts/check-claims.mjs` runs against
the website repository; **nothing checks the strings a user actually reads inside the app** —
`apps/osl-hub-ui/src/**` and user-facing text in `apps/osl-hub/src/**`. README is now correct but is
likewise unguarded, as tonight demonstrated: it carried an inverted burn claim on the repository's
front page while THREAT_MODEL already said those words were unearned.

**Now closed.** `scripts/check-app-claims.mjs` in the app repository scans every string and template
literal under `apps/osl-hub-ui/src/**` plus `README.md`, and is wired into the `TypeScript Test`
workflow so it cannot rot. First clean run: **28 banned phrases parsed, 7,345 strings scanned,
0 violations.**

It **parses section D of this file directly** rather than keeping its own list. That is the point:
add a row to §D and the app gate tightens automatically, with no second list to drift. It carries
non-empty floors (>=8 phrases, >=300 strings, README non-empty) so it cannot pass by measuring
nothing, proven by starving it.

Two precision rules were required to make it usable, both learned from its first run:

- **Negation awareness.** README legitimately says burn is *not* cryptographic erasure; a gate that
  flags an honest denial is worse than no gate, because it gets switched off.
- **Context gating for ordinary English.** Section D bans "audited"/"reviewed" as *security* claims,
  but the first run flagged "Selected apps reviewed" and "Every batch is reviewed and confirmed" —
  the user reviewing a batch. Those single words now fire only near security context (osl,
  encryption, protocol, independently, third-party). Multi-word section D phrases stay absolute:
  "cryptographic burn" is never innocent. The b90 public-review unlock is deliberately narrower:
  the app gate admits only the exact `SESSION_RESET` remediation sentence when the exact
  one-remediation, not-third-party-audit limitation is in the same scanned fragment.

**The crawler now exists.** `scripts/check-claims.mjs` in the website repository asserts that no
page contains a section D phrase, that no bare price appears outside the pricing manifest, that
every capability badge matches the manifest's registry, and that the mandatory limitation sentences
are present on the pages that need them. It carries a known-bad fixture suite (`--self-test`,
14 fixtures) so it cannot decay into an all-green source-shape test, and `scripts/pricing-sync.mjs
--check` proves no page has drifted from the manifest. Residual gap: both are pre-deploy commands,
not a CI gate, so nothing yet *blocks* a deploy that skips them.

### Owner reframe 2026-07-26 — the site is pre-launch marketing, not a status dashboard

Zhao's correction, and it overrides the presentation model this file previously implied
(authority order §0.1 item 1). **The website is pre-launch marketing for v1.** A site with no
`Available` badge anywhere reads as a dead product, which is its own form of dishonesty: OSL is a
product being built toward a launch, and the site should look like the thing being launched.

What changed:

- **Marketing pages show the full v1 product** — encrypted messaging, attachments, view-once,
  expiry, burn, Scrub, the exposure warning — with real explanations and the animations. Individual
  feature cards no longer carry a `Planned` stamp; that stamp on every card is what made the site
  read as vaporware.
- **Honesty is concentrated in two unmissable places** instead of thirty disclaimers:
  1. **The dated support matrix**, `/docs/status` — the master §8.4 versioned matrix. Per capability
     and per connector: protected send, protected receive, attachments, Scrub, verification date,
     provider-policy risk, status. Every feature section links to it in one plain line. This page is
     *generated from* `data/pricing.json`, so it cannot drift from the registry.
  2. **The point of sale.** What a person is charged for must be unambiguous about what they get
     today versus at v1. This is a consumer-protection line, not a marketing preference, and it is
     the one surface where softening is not allowed.
- **A global early-access banner** carries the frame for the site as a whole.

**The status vocabulary in §E still governs the matrix and the checkout.** It no longer governs
whether a marketing page may describe a v1 capability. The enforceable rule, now implemented in
`scripts/check-claims.mjs`:

> A marketing page may describe a v1 capability in forward-looking language. The support matrix and
> the checkout summary may only state what current evidence supports. A present-tense capability
> claim outside those two surfaces still fails the gate.

Mechanically: a marketing section naming a capability that is not `Available`/`Beta` must carry a
forward-looking marker; every marketing capability page must link to the matrix; the matrix must
account for **every** registry capability with a matching badge; and a checkout summary may only
reference capabilities flagged `sellable`. Section D remains **absolute on every surface** — a
forbidden phrase is forbidden regardless of framing. (The `ratchet` capability was renamed to
"Ratcheting (protecting old messages)" rather than exempted, because its old public name contained
a section D phrase.)

### Corrected 2026-07-26 — `per-message-sealing` is Beta, not Planned

This file previously drove that badge to `Planned`. **That was wrong, and the error is instructive.**
`implemented-unwired` describes *a call path*, not whether the cryptography works. The code does do
this — `crates/ipc/src/wire_v2.rs:685-760` seals every DM to the recipient's published X25519 and
ML-KEM-768 keys with a fresh per-message ephemeral — and live encrypted sends have landed real cover
text in a real Discord conversation. Labelling a shipped capability `Planned` understates it, which
is its own kind of dishonesty.

**The standard is "does the code do this", not "has a two-identity harness watched it."** `Beta`
rather than `Available` remains correct: it is proven on QA builds rather than a named release
build, and sender attribution is still an open finding.

Re-checked under that standard, the other four downgrades hold, for reasons that are about the code
and not about missing harness time:

- `cover-carrier-text` → `Beta` (not a downgrade in substance; it was `Available` with no row
  granting it).
- `image-send` → `Planned` — the picker offers PNG/JPEG and the streaming AEAD exists, but master §9
  puts the attachment lane at `implemented-unwired`. Historical Worker `0a17547d` is source-unbound,
  recovery `3938a73` is local runtime evidence only, release contract `1e9e635` is test-only, and
  no named release build proves ciphertext/decoy delivery to Discord. Zhao's instruction in the
  same correction — "do not silently sell image sending as a present-tense feature" — agrees.
- `scrub-discovery` / `scrub-guided-deletion` → `Planned` **on the matrix only**. The marketing page
  is deliberately *not* watered down; Scrub is presented as a full v1 capability, because that lane
  may land exact-build evidence within the week and the site must not whipsaw.

### Downgrades applied 2026-07-26 (truth lane)

Four capability badges remain lowered after source verification (`per-message-sealing` was restored
to `Beta` — see above). Recorded here so nobody re-derives the old value from an older page:

| Capability | Was | Now | Why |
|---|---|---|---|
| Cover text carrier | `Available` | `Beta` | The claim it carries is row C1, which is `runtime-proven` on QA builds only. No row grants it `Available`. |
| Encrypted image sending | `Beta` | `Planned` | Master §9 attachments are `implemented-unwired` with open findings; master §10 critical class 4. Scope (PNG/JPEG only) is not proof. **This is the headline Pro benefit.** |
| Scrub discovery | `Beta` | `Planned` | No exact-build evidence; 0 accounts detected on the last live run. |
| Scrub guided deletion | `Experimental` | `Planned` | Cannot be exercised end to end while discovery produces nothing. |

After these changes the website carries **no `Available` capability badge at all**. That is the
honest current state, and it is the single most important thing for the owner to see.
