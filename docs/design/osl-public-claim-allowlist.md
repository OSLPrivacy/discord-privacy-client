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
| **Permitted wording** | "Discord receives cover text, not your message. It never receives the decryption key." |
| **Status** | **`Beta`** badge, on the same corrected standard as A1. |
| **Evidence** | `crates/ipc/src/wire_v2.rs:685-760`; body sealed under a fresh random AES-256-GCM key, wrapped per recipient |
| **Required alongside** | "Discord still sees who you talk to, when, and how often. We cannot hide that." (metadata exclusion — mandatory on every confidentiality claim) |

### A3 · Local encrypted storage

| | |
|---|---|
| **Permitted wording** | "Decrypted messages cached on your device are encrypted at rest." |
| **Status** | `test-proven-only` → **`Beta`** badge |
| **Evidence** | `crates/store/src/cipher.rs:9` (XChaCha20-Poly1305, random nonce); sealed on insert at `crates/store/src/lib.rs:188-206` |
| **Required alongside** | "This does not cover attachments. Non-image attachments are currently written to disk unencrypted — see master §10 finding 4." Do not make this claim on any page that also markets attachments. |

### A4 · Open source

| | |
|---|---|
| **Permitted wording** | "OSL is fully open source, except one optional AutoScrub module you choose to download separately." |
| **Status** | `verified-live` → **`Available`** badge |
| **Evidence** | Public repository `OSLPrivacy/discord-privacy-client`, Apache-2.0 |
| **Required alongside** | The exception must be stated in the same sentence, per master §7.6 and §7.10 — never as a footnote. |

### A5 · Screenshot resistance

| | |
|---|---|
| **Permitted wording** | "Blocks common screen-capture tools such as OBS, ShareX, Game Bar and Xbox capture." |
| **Status** | `implemented-unwired` → **`Planned`** badge (the API call exists; first-paint ordering is an open critical, master §2 P0-5) |
| **Evidence** | `SetWindowDisplayAffinity` / `WDA_EXCLUDEFROMCAPTURE` at `apps/osl-hub/src/native_discord_overlay.rs:3633-3638` |
| **Required alongside** | "It cannot stop a phone camera, a hardware capture device, or a modified client. On machines where Windows refuses the protection it does nothing — and we show you when that happens." Never write "screenshot-proof" or "prevents screenshots". Master §3b: this is the feature most likely to be over-read; keep the copy narrow. |

### A6 · No stored payment data

| | |
|---|---|
| **Permitted wording** | "$5 for one month. Nothing renews, nothing to cancel, and we never store your payment details." |
| **Status** | `verified-live` → **`Available`** badge, for **these three claims only** |
| **Evidence** | Master §7.14; deployed checkout is a single one-time charge |
| **Required alongside** | Nothing extra. **But:** "your month starts when you enter the code" is **NOT eligible** — see D8. |

### A7 · Nobody can claim your account before you do

| | |
|---|---|
| **Permitted wording** | "Nobody can register your Discord identity on OSL's key server — not even before you do. OSL identities are separate from your Discord account, and the key server refuses Discord identifiers outright." |
| **Status** | `verified-live` → **`Available`** badge |
| **Evidence** | Migration `keyserver-cf/migrations/0029_authoritative_osl_identity.sql` applied to production 2026-07-26 (owner-confirmed against the live migration list). Worker refuses snowflakes at both surfaces: `keyserver-cf/src/endpoints/register.ts:115` and `keyserver-cf/src/endpoints/pubkeys.ts:21` return `400 "Discord identifiers are not OSL identities"`. Quarantine enforced in `keyserver-cf/src/lib/db.ts:377`, `:474` (`AND identity_lookup_enabled = 1`), re-enable path at `:408-411`. All 111 pre-existing identities start disabled (`identity_lookup_enabled INTEGER NOT NULL DEFAULT 0`). |
| **Required alongside** | "Existing accounts created before this change are switched off until their owner's app re-registers them, which happens automatically the next time they open OSL." *Corrected 2026-07-26:* the previous wording ("until their owner re-registers") implied manual work. `ensure_keyserver_registered` runs on launch **and** unlock, not only for new identities (`crates/ipc/src/commands.rs:7580`, called from `src-tauri/src/bootstrap.rs:1296`, `apps/osl-hub/src/password_lifecycle.rs:384`, `apps/osl-hub/src/core_bridge.rs:306`), and `build_register_request` sends the OSL `user_id` with no snowflake (`crates/keystore/src/client.rs:601`), so a quarantined identity re-enables itself. Do not extend this into a general identity-verification claim — full-bundle identity binding is still an open finding (master §2 P0-1), so this closes account *pre-registration*, not key *substitution*. |
| **Open risk, not this lane's** | `Identity` carries a `discord_snowflake` field populated by `osl_register_self_snowflake`. Any peer-lookup path keyed on a snowflake rather than an OSL user id is now **permanently unresolvable** under 0029. Raised by the crypto lane 2026-07-26; needs an explicit check by whoever owns peer lookup. It does not falsify A7, but it could break finding a contact. |

This is the first row in this file to reach `verified-live` on the strength of a production
deployment. It closes master §2 P0-3 and §10 critical class 3.

### A8 · Local-first processing

| | |
|---|---|
| **Permitted wording** | "Encryption, keys and your plaintext stay on your machine." |
| **Status** | `implemented-unwired` → **`Planned`** badge |
| **Evidence** | Master §6 product contract; no server-side plaintext path in `crates/ipc` |
| **Required alongside** | If the page also mentions Pro cloud carrier generation or cloud AutoScrub, it must say those send selected data to a server and are not end-to-end private (master §7.1, §7.6). |

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
| Bilateral burn | `implemented-unwired`, plus an open defect | `apps/osl-hub/src/broker.rs:2635-2640` deletes the revocation notice unapplied | "Planned." See `docs/qa/two-identity-p2p-verification.md` §6 item 4. |
| View-once, timed deletion, attachments | `implemented-unwired`; attachments additionally **could not execute in production** | master §9 rows. **2026-07-26:** attachment upload was broken in production and had been before that date — the body was piped through a `TransformStream` and R2 requires a known length, so every upload failed. It passed tests only because the R2 test double accepts any stream. Server side now fixed and probed on a real workerd, not deployed. | "Planned." Master §8.2 forbids marketing these as available before the exact release build has evidence. **Treat any "attachment sent successfully" reported before 2026-07-26 as unproven** — including any earlier award or badge that rested on it. Re-derive from evidence, never from a prior award. |
| Signal, WhatsApp, Telegram, Outlook support | `designed-only` / `externally-blocked` | master §7.8, §9 | Must carry `Coming soon`, `Experimental`, or `Externally blocked`. Master §7.8: a logo does not imply support unless the support matrix says so. |
| Scrub discovery ("find the accounts you left behind") | `unknown-recheck-required` → published as `Planned` | Master §9 "active dirty integration work"; no evidence names an exact build; the last live report (2026-07-25) was that pressing "do scrub" detected **0 accounts**, because browser import never creates an `AccountRecord` or hands anything to Scrub | "Planned. Not in the shipping app yet." Describe the scan as *designed to* stay on the device, not as something it does today. Never publish a finding count, real or illustrative, as if it came from a real scan. |
| Scrub guided deletion handoff | `implemented-unwired` → `Planned` | Depends on Scrub discovery, which produces no accounts on any named build, so the handoff cannot be exercised end to end | "Planned." Must always say OSL does **not** erase anything for you — it discovers and hands off, and the user confirms every deletion on the service's own page. Master §7.5: requested deletion is never shown as verified deletion. |
| AutoScrub | `implemented-unwired` → `Planned` | Master §9: active implementation, optional-module packaging unresolved | "Planned." Must state the closed-source exception in the same sentence (master §7.6, §7.10), that it is **not installed by default**, and that cloud AutoScrub is not end-to-end private. |
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
| Encrypted send reaches Discord | "Encrypted messages send through Discord as ordinary-looking text." | `runtime-proven` → **`Beta`** | Dated QA screenshot evidence, master §9 "Discord protected send" | "Verified on QA builds, not yet on the release build." |
| Decrypted overlay ("the eye") | "OSL paints the decrypted text over the Discord rows it belongs to." | `runtime-proven` → **`Beta`** | `osl-rehydrate-geometry-diagnosis.md`: `placedRowCount` 0→3, `rel_l` −8→0, executable SHA-256 `6b6a36945b42…` | "Measured on a QA build. Placement is proven; the full visual result, peer-authored rows, scrolling, resizing and DPI changes are not." |
| Delivery confirmation | "OSL tells you whether Discord accepted the message — sent, not sent, or uncertain." | `designed-only` | Master §7.3 tri-state contract | Do not claim until the tri-state proof exists on an exact build. `delivery_uncertain` must never be described as failure. |

---

## D · NOT ELIGIBLE — these phrases may not appear anywhere

Website, in-app copy, README, store listing, social posts, screenshots, alt text.
Each is listed with why, so nobody re-derives it and reintroduces the phrase.

| Forbidden phrase | Why it is forbidden |
|---|---|
| **"Better than Signal"** | Explicitly barred by master §7.11: no public "Signal protocol", "better than Signal", forward-secrecy, post-compromise or post-quantum-authentication claim may outrun the audited live path. Nothing has been audited, and Signal has forward secrecy and post-compromise security that OSL currently does not. The claim is false today, not merely unproven. Master §8.4 also requires OSL's disadvantages to appear with the same prominence as competitors'. |
| **"Post-quantum authentication"** | Factually wrong. ML-KEM-768 provides *confidentiality* only (`crates/ipc/src/wire_v2.rs:731`). Sender authentication is classical X25519/Ed25519. Additionally, attribution is an open critical finding (master §10 finding 2) — the receive path authenticates one key and attributes the plaintext to a caller-supplied identity. So authentication is neither post-quantum nor currently sound. |
| **"Cryptographic burn"** | Burn is policy and state deletion, not destruction of decryption capability. `MessageStore::put` (`crates/store/src/lib.rs:194-206`) never writes the `wrapped_key` column, so the burn paths that set `wrapped_key = NULL` (`:298`, `:342`, `:350`, `:560`) null a column that was already null. Burn zeroblobs the *local* cached ciphertext — real, and sayable — but the ciphertext on Discord's CDN stays decryptable by any holder of the recipient's long-term keys. |
| **"Destroys keys, not messages"** | Exactly inverted. There is no key to destroy: `MessageStore::put` (`crates/store/src/lib.rs:194-206`) never populates `wrapped_key`, so every burn path that sets `wrapped_key = NULL` nulls a column that was already null. Burn destroys *messages* — OSL's local copies of them — and destroys no keys at all. This phrasing was on `README.md:78`, the repository's public front page, until 2026-07-26. |
| **"Permanent ciphertext" / "permanent gibberish" / "mathematically opaque"** | Asserts the carrier becomes undecryptable after a burn. It does not. v3 seals to the recipient's *long-term* keys (`crates/ipc/src/wire_v2.rs:722-729`, no one-time prekeys), so anyone holding that key material can still read the ciphertext the connected service retains — burning your own copy changes nothing about theirs. Comes from design documents describing the per-message wrapped-key model, which is **designed and deliberately not built** (owner decision 2026-07-26). |
| **"Disappears forever" / "permanently undecryptable" / "gone for good"** | Same root cause as above, plus: v3 wraps to *long-term* recipient keys with no one-time prekeys (`crates/ipc/src/wire_v2.rs:722-729`), so a recipient's keys decrypt their entire history indefinitely. Also unfixable in principle for screenshots and copies — master §8.4 requires Burn copy to separate local erasure, cooperative peer request, host-platform deletion attempt, and unpreventable copies. |
| **"Works on Gmail / Discord"** as a general capability claim | Discord is the reference adapter and is `runtime-proven` on QA builds only — see section C. Gmail has no adapter at all; Outlook is scoped as OSL Mail, a separate staged program (master §7.8, §9). Naming a service you have not qualified is exactly the "logo implies support" failure master §8.6 prohibits. Per-service claims must come from the versioned support matrix (master §8.4), not from this general form. |
| **"Provider-tested" / "verified by Discord" / "works with Discord's approval"** | No provider has tested, reviewed, or approved anything. The opposite is closer to true: master §5 and THREAT_MODEL "Discord ToS" note that using OSL may violate Discord's Terms of Service and may get the account banned. Implying provider sanction is both false and harmful to users making a risk decision. |
| **"Audited" / "reviewed" / "independently verified"** | No audit has been commissioned. THREAT_MODEL "Audit status" requires the *opposite* disclosure in onboarding: the construction is custom and unaudited. The only completed review is an internal source audit that found 5 critical and 2 high findings (`docs/security/osl-audit-2026-07-26-codex.md`). |
| **"Military-grade" / "unbreakable" / "NSA-proof"** | Meaningless or false. THREAT_MODEL "Out of scope": OSL is explicitly *not* intended to resist targeted federal investigation, and points such users to Signal, Briar or Cwtch. |
| **"Screenshot-proof" / "prevents screenshots"** | Overstates A5. Capture protection is a platform affordance that can silently fail and cannot stop a camera. |
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

## F · Maintenance

Update this file in the same task as any change to security truth or feature status
(master §20.2). When a row changes, also update master §9, the internal checklist, and
any page currently using the old wording — a row that no longer earns its badge is a
live incorrect claim on the site, not a documentation backlog item.

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
  puts the attachment lane at `implemented-unwired` and §10 critical class 4 has non-image
  attachments staged as durable plaintext. Zhao's own instruction in the same correction — "do not
  silently sell image sending as a present-tense feature" — agrees.
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
