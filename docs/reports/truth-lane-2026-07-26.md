# Truth lane — website, public claims, project documents (2026-07-26)

**Lane owner:** truth. **Model:** Opus 5, effort high.
**Branch:** `web-pricing-truth-2026-07-26` in `/mnt/c/Users/liamw/projects/oslprivacy-web`.
**Commit:** `72213331beee76da926124214d9e778b4517dc45` ("Bring every website claim inside the
public-claim allowlist"), based on `d77709b` = `origin/main` = **what production serves today**.
**Nothing is deployed.** Promotion to production is the owner's call.

Authority used: master `OSL-MASTER-2026-07-26-r7` (bumped by this lane from r5 through r7),
`docs/design/osl-public-claim-allowlist.md`, `docs/THREAT_MODEL.md`, and current source.

---

> **Read §10 first.** After the first pass, Zhao corrected the frame: the site is **pre-launch
> marketing for v1**, not a live status dashboard, and one factual error of mine (`per-message-sealing`
> labelled `Planned`) was reversed. Sections 2–8 describe the first pass and are kept for the audit
> trail; where §10 differs, §10 wins.

## 0 · The one-paragraph version

The website's pricing is now driven by a single manifest and is provably consistent everywhere. The
bigger finding is that the site was over-promising capability, not price: after checking every
capability badge against the allowlist and against source, **five badges had to come down, and the
site now carries no `Available` capability badge at all.** The most commercially serious of those is
encrypted image sending — the headline benefit Pro is sold on — which is `Planned`, not `Beta`. That
is escalated to Zhao as a decision, not fixed by copy edit. Separately, six verified-false claims are
**live on production right now** and stay live until the owner promotes this branch.

---

## 1 · Live false claims currently on production — raised as an incident

Production serves `origin/main` @ `d77709b` (confirmed: `https://oslprivacy.com` stamps
`assets/js/main.js?v=182c9e8f`, which is the `origin/main` asset fingerprint). Every phrase below is
present in that commit and is therefore public right now:

| Phrase | Page at `d77709b` | Why it is false |
|---|---|---|
| "Erase 3" | `index.html`, `features.html` | Scrub discovers and hands off; it does not erase. |
| "forward secrecy" | `docs/index.html` | The live path is stateless per-message sealing. `crates/ipc/src/commands.rs:2842` hard-disables the ratchet. |
| "use Sender Keys" | `docs/faq.html` | Sender keys are off (`crates/ipc/src/state.rs:280`) and the construction carries no sender signature. |
| "Unlimited messages" | `download.html` | Master §8.5 orders this replaced with Scrub positioning. |
| "Private text + files" | `download.html` | Non-image sending is unavailable; the picker offers PNG/JPEG only (`apps/osl-hub/src/attachment_formats.rs:154`). |
| "the key and readable message disappear" | `features.html` | Expiry is not wired, and there is no cryptographic burn — `crates/store/src/lib.rs:194` never populates `wrapped_key`. |

All six are fixed on this branch. **They remain live until the branch is promoted.** That promotion
is the owner's decision and this lane did not make it.

---

## 2 · Task 1 — pricing, now single-source

Pro is **$5 for one month**, bought as a prepaid activation code; compute credits are separate
one-time purchases (master §7.14). `data/pricing.json` is the single manifest and now drives
**19 markers across all 15 pages** — homepage, download/checkout, FAQ, terms, privacy, receipt and
entitlement copy.

**The real defect found here:** `index.html` carried two `Get Pro · $5 / month` checkout buttons and
had **no renewal, cancellation or activation-code disclosure anywhere on the page**. Read plainly,
that is a subscription — the exact implication master §7.14 forbids ("Copy must never imply a stored
card or a cancellation duty"), and a breach of allowlist A6 rule 3, which requires the limitation to
ship on the same page at comparable prominence, not in a linked FAQ. The crawler had not caught it
because no `required_phrases` entry covered `index.html`.

Fixed: `tiers.pro.no_renewal_note` was added to the manifest, rendered in both the hero and the
`home-buy` section of `index.html`, and pinned by a new `required_phrases` entry so it cannot
regress.

Left alone deliberately: `docs/terms.html`'s `$50` is a liability cap, not a price — the crawler
already whitelists it by proximity to the word "liability".

Prepaid-code honesty: the redemption claim stays unpublished. Master §7.14 and allowlist D8 forbid
"your month starts when you enter the code" until the keyserver actually implements it — today a paid
code grants **lifetime** access and no redemption timestamp exists. The manifest records this in
`model.$redemption_note` as product truth that is explicitly *not publishable yet*.

**Verification:** `pricing-sync --check` → 19 markers, 0 drift, 0 unknown paths.
`check-claims` → 15 files scanned, 0 failed, 0 bare prices, 0 forbidden hits.

---

## 3 · Task 2 — every capability claim brought inside the allowlist

All eight items named in the assignment were already corrected in the previous tab's uncommitted
work; I verified each against source rather than assuming, and all eight are genuinely gone. The
pinned sentence in `docs/how-it-works.html` — "fresh ephemeral key for every message, so each message
stands on its own" — is intact and is now enforced by a `required_phrases` entry, so a future cleanup
cannot delete the one accurate crypto sentence while removing the false ones around it.

What the previous tab had **not** done, and what this lane changed: the capability registry still
granted statuses the allowlist does not earn.

| Capability | Was | Now | Reason |
|---|---|---|---|
| `per-message-sealing` | `Available` | `Planned` | Allowlist A1/A2 are `implemented-unwired` → Planned until a **named release build** proves the promise end to end. Allowlist §E also makes `open-security-finding` outrank everything on the row, and master §9 lists live v3 crypto as exactly that (sender attribution). The scheme *is* live in source; the badge is about proof, not existence. |
| `cover-carrier-text` | `Available` | `Beta` | The claim it carries is row C1, "Encrypted messages send through Discord as ordinary-looking text", which is `runtime-proven` on QA builds only. No allowlist row grants the carrier `Available`. |
| `image-send` | `Beta` | `Planned` | Master §9 lists the whole attachment lane `implemented-unwired` with open findings; master §10 critical class 4 is non-image attachments staged as durable plaintext. Scope (PNG/JPEG only) is not proof of function. |
| `scrub-discovery` | `Beta` | `Planned` | No evidence names an exact build. Master §9 says "active dirty integration work"; the last live report (2026-07-25) was that pressing "do scrub" detected **0 accounts**, because browser import never creates an `AccountRecord`. Allowlist rule 5: evidence that does not name an exact build is `unknown-recheck-required`, which earns no claim. |
| `scrub-guided-deletion` | `Experimental` | `Planned` | Cannot be exercised end to end while discovery produces nothing. |

**Consequence: the site now carries no `Available` capability badge.** Current distribution is
4 `Beta`, 31 `Planned`, 2 `Illustration`, 0 `Available`. That is the honest state of the product and
it is the single most important thing in this report after the Pro finding.

Copy was rewritten to match, not just badges — Scrub sections now read "Planned, and not in the
shipping app yet… the drawing below shows the intended sequence", the Scrub promise moved from "The
scan stays on your device" to "is designed to stay", and the burn FAQ answer was rewritten to
separate local erasure, the cooperative peer request, and what burn cannot do.

The AutoScrub open-source exception is stated consistently with §7.6/§7.10 on both
`docs/how-it-works.html` and `audit.html`, and both are pinned by `required_phrases`.

---

## 4 · Task 3 — banned phrases in project documents

Both were already corrected in the working tree; I verified the corrected text says what burn
actually does.

- `docs/design/open-core-and-local-privacy-boundary.md:82` — now reads that OSL Burn "deletes OSL's
  local and server-side state and sends a cooperative request to the peer's client", and states
  explicitly that burn does **not** destroy the only decryption capability, citing
  `crates/store/src/lib.rs:194-206`.
- `docs/design/pqxdh-double-ratchet.md:242` — now marked `Planned` with the same correction.

A sweep of `docs/**` for "cryptographic burn", "permanently undecryptable", "disappears forever" and
"gone for good" returns only **ban-list entries** (the allowlist, THREAT_MODEL, simple-spec, master
§9, and the two files above telling people not to write it). No violations remain. As instructed,
the `cipher-store-cf` hits were left alone — they are ban lists, not violations.

---

## 5 · Task 4 — status keeper

**No lane reports had landed** in `docs/reports/` at the time of writing (only
`baseline-2026-07-26-current-bytes.md`). Nothing was rubber-stamped because there was nothing to
judge. The pipeline is ready: when lanes 1 and 2 land their "Acceptance rows this earns" sections,
they get applied in one atomic edit per lane.

### Checklist: 85 → **88 / 303**

I found and fixed a **pre-existing arithmetic error**: section H's header said 5 earned while its own
rows summed to 7. Rows are the authority, so the recorded 85 had been *understating* the project by
2. Nothing was built to close that gap.

| Change | Δ | Why |
|---|---:|---|
| Bookkeeping correction, section H | +2 | Header disagreed with its own rows. |
| H1 · one pricing model | +1 | Manifest provably drives all 15 pages: 0 marker drift, 0 conflicting price/renewal/entitlement claims, missing A6 limitation added and pinned. The 4th point is **held** for actual promotion to production and the keyserver redemption change. |
| J7 · public-claim allowlist | +1 | The master §8.6 crawler now exists and is proved by 14/14 known-bad fixtures. |
| H8 · accessibility/motion/responsive matrix | −1 | **Overclaim corrected.** It sat at full marks while its own text named an open defect. 200% zoom and the accessibility half of §8.6 (accessible names, 44×44 tap targets, focus, non-colour-only state) are still unmeasured. |

Verified consistent after editing: row weights 303 = header weights 303, row earned 88 = header
earned 88 = snapshot 88, **0 section mismatches**. Applied as a single atomic write.

No points were withdrawn for the five badge downgrades, because none of those badges had ever earned
a checklist point — they were website overclaims, not recorded progress.

### Other document updates

- **Master bumped r5 → r6** with a §0.5 delta line, and the §9 Website row rewritten from "multiple
  divergent deployments and conflicting claims" to the current evidenced state.
- **Allowlist gained the rows it was missing**, so every badge on the site now maps to a row: Scrub
  discovery, Scrub guided deletion, AutoScrub, link/tracker protection, before-send exposure warning,
  AI carrier text, processing credits, plus an explicit note covering the two `Illustration` items.
  Section F now records that the crawler exists, and a new subsection records the five downgrades so
  nobody re-derives an old value from an older page.

---

## 6 · Task 5 — record, don't fix: root `Cargo.toml`

**Not edited**, as instructed — another lane is building Rust and a root-manifest touch forces a full
rebuild. Verified false: root `Cargo.toml:25-28` says osl-ratchet-next has no dependents, but
`crates/ipc/Cargo.toml:73` declares it. `implemented-unwired` still holds, because nothing uses
`wire_rn` — its only non-test reference is `pub mod wire_rn;` at `crates/ipc/src/lib.rs:76`.

Exact replacement text for the manifest owner. Current lines 25–28:

```toml
    # Isolated, UNWIRED research crate. Reachable only from its own
    # tests; no other crate or app depends on it. See
    # crates/osl-ratchet-next/DESIGN.md — unreviewed, must not carry
    # real traffic before external cryptographic review.
```

Replace with:

```toml
    # Isolated research crate, UNWIRED at the product level. `crates/ipc`
    # declares the dependency (crates/ipc/Cargo.toml:73) and the adapter
    # crates/ipc/src/wire_rn.rs genuinely uses it — but nothing uses
    # wire_rn: its only non-test reference is the module declaration
    # `pub mod wire_rn;` at crates/ipc/src/lib.rs:76. There is still no
    # production call path, so the status remains implemented-unwired. See
    # crates/osl-ratchet-next/DESIGN.md — unreviewed, must not carry
    # real traffic before external cryptographic review.
```

---

## 7 · Self-verification

| Gate | Result |
|---|---|
| `node scripts/check-claims.mjs` | 15 files scanned, **0 failed**; 0 bare prices, 0 forbidden hits, 0 bad badges, 0 missing required sentences |
| `node scripts/check-claims.mjs --self-test` | **14/14 fixtures**, 0 failed — 12 known-bad caught, 2 honest-negation cases correctly not flagged |
| `node scripts/pricing-sync.mjs --check` | 19 markers, **0 drift**, 0 unknown paths |
| `node scripts/screenshot-matrix.mjs` | **252 captures, 0 failed, 0 unmeasurable** |
| Checklist arithmetic | rows 88/303 = headers 88/303 = snapshot; 0 section mismatches |
| Every badge maps to an allowlist row | yes, after the allowlist additions in §5 |

**Screenshot matrix**, HeadlessChrome/149.0.7827.55, build stamp `d77709b3-dirty`,
`docs/evidence/website-matrix/matrix.json`:

- 14 pages × {320, 360, 390, 768, 1024, 1440} px × {js-on, js-off, reduced-motion} = 252.
- **Meaningful content is visible before scroll and without JavaScript on every page.** Worst case
  across all 252 captures is 154 visible characters (`/donate` at 1440 px); the js-off floor is also
  154. The reveal-animation blanking problem in master §8.5 does not reproduce.
- 504 full-page and fold PNGs are on disk at `docs/evidence/website-matrix/` (70 MB). They are
  **gitignored on purpose** — the repository carries `matrix.json`, the machine-readable verdict, not
  70 MB of binaries. `.assetsignore` excludes `data/` and `docs/evidence/` from publication.

**Not covered, and honestly flagged:** 200% zoom and the accessibility checks required by master
§8.6. That is exactly why H8 was corrected down to 2/3.

---

## 8 · Things I could not decide, or deliberately did not

1. **Pro is sold on a `Planned` capability.** Raised to Zhao as `action-needed`. With `image-send`
   at `Planned`, the only currently-truthful thing Pro buys is "early access to new privacy tools as
   they reach the beta channel". I made the copy honest and left the checkout working, because
   whether to keep selling, reprice, pause sales, or ship image-send is a commercial decision that is
   not mine. The copy now says plainly: "Buying today buys early access to those tools as they land,
   not the tools themselves."
2. **`per-message-sealing` → `Planned` is the most debatable call in this report.** The scheme really
   is what the live code does, and labelling it `Planned` risks reading as "the encryption isn't
   there". I followed the allowlist because §E is explicit and because master §0.2 rule 5 says choose
   the safer, easier-to-prove option. I added a clause to the page explaining that it is what the code
   does today and that the badge is about unproven-on-a-release-build plus the open attribution
   finding. If the owner prefers `Beta` here, that is a defensible reading of C1 and a one-line change.
3. **A conflict I did not resolve silently.** The assignment implies image sending works and only
   *file* sending is unavailable; master §9 and §10 say the whole attachment lane is unproven. I chose
   the safe direction (`Planned`) and am flagging the disagreement rather than picking a winner
   quietly, per master §0.2.
4. **Scrub downgrade lands 7 days before the hard demo.** Marking the flagship demo feature `Planned`
   is uncomfortable, and it is reversible the moment the Scrub lane lands an exact-build walkthrough
   in `docs/reports/`. I would rather under-claim now and promote on evidence.
5. **Not deployed, not pushed.** The commit is local on `web-pricing-truth-2026-07-26`. I did not push
   the branch either, because the deploy trigger for this repository is unverified and the safe
   assumption on record is that a push to `main` publishes to production.

---

## 9 · Operational finding for the other lanes — event `scope` must be a registered id

Both of this lane's events were rejected on first submission with `unknown event scope`, because
`scope` was set to a descriptive word (`"website"`). `apply_event` requires `scope` to be an id that
already exists in the bot's state, otherwise it raises and the file is renamed `.rejected` and
silently dropped. Resubmitting the same ids with a valid scope worked; both are now in
`processed_event_ids` with `delivered-` records and an empty `alert_outbox`.

**Valid scopes today** (the full checklist has been imported, so this is richer than the "only
`overall` exists" note that was circulating): `overall`, `workstream.a` … `workstream.j`,
every acceptance row id `a1` … `j7`, plus `workstream.windows` and the per-window ids. Prefer the
most specific one — a website event should use `h1`/`workstream.h`, not `overall`, so it routes to
the right dashboard.

**Two events from the Scrub lane are currently stranded as `.rejected`** in
`/home/liamw/claude-bridge/osl-events/` and were never delivered:
`scrub-f10-demo-target-decision-2026-07-26` and `scrub-imap-qa-certificate-authority-2026-07-26`.
They are not this lane's to re-file — the Scrub lane should resubmit them with the same ids (replay
is idempotent) and a valid scope such as `f10`.

## Acceptance rows this earns

- **H1 +1** (2 → 3 of 4) — one pricing model, provably single-source across all 15 pages.
- **J7 +1** (2 → 3 of 3) — the master §8.6 claim crawler exists and is fixture-proved.
- **H8 −1** (3 → 2 of 3) — correction of a pre-existing overclaim.
- **Section H header +2** — correction of a pre-existing addition error, not new work.

Net: **85 → 88 / 303.**

---

## 10 · Second pass — owner reframe: pre-launch marketing for v1

Zhao's correction, applied in full. The infrastructure from the first pass stayed; the presentation
model changed, and one of my calls was simply wrong.

### 10.1 The error I made

I labelled `per-message-sealing` **Planned**. That was wrong. I had treated master §9's
`implemented-unwired` as if it described the cryptography, when it describes *a call path*. The code
does do this — `crates/ipc/src/wire_v2.rs:685-760` seals every DM to the recipient's published X25519
and ML-KEM-768 keys with a fresh per-message ephemeral — and live encrypted sends have landed real
cover text in a real Discord conversation. Restored to **`Beta`**: proven on QA builds, not on a
named release build, with sender attribution still an open finding.

**The standing test is now "does the code do this", not "has a two-identity harness watched it."**
Re-checked under it, the other four hold for code reasons, not missing-harness reasons:
`cover-carrier-text` `Beta` (never had a row granting `Available`); `image-send` `Planned` (master §9
attachment lane `implemented-unwired`, §10 critical 4 durable-plaintext staging — and Zhao's own
instruction not to sell it present-tense agrees); both Scrub rows `Planned` **on the matrix only**.

### 10.2 What the site looks like now

- **Full v1 product is shown.** Encrypted messaging, attachments, view-once, expiry, burn, Scrub and
  the exposure warning are presented as what OSL is, with real explanations and the animations
  intact. The per-card `Planned` stamp is gone — that stamp on every card is what made a product
  under construction read as vaporware.
- **Honesty concentrated in two places.**
  1. **`/docs/status` — "What works today."** The master §8.4 versioned matrix, dated
     `2026-07-26`: a per-capability table covering all 21 registry entries and a per-connector table
     with protected send, protected receive, attachments, Scrub, verification date, status and
     provider-policy risk. It is **generated from `data/pricing.json`**, so it cannot drift, and
     `build-status --check` fails if the committed page is stale. Every feature section links to it
     with one plain line, "See what works today."
  2. **The point of sale.** Pro is now an explicit early-access purchase: *"Pro is an early-access
     purchase. You are paying to support a product that is still being built, and to get new privacy
     tools first. You are not buying the finished v1 feature set."* Then two lists — what Pro gives
     you today, and what is not in the app and is not what you are paying for. Image sending sits in
     the second list.
- **One global early-access banner** on every page instead of thirty disclaimers.
- **Scrub marketing was not watered down**, per instruction. Its matrix row carries the truth.

### 10.3 Checkout decision, implemented

Keep selling, as stated early access, at $5/month. It fits the existing layout — no layout change
was needed. The purchase summary is wrapped in `<!--osl:checkout-summary-->` markers and the crawler
enforces that only capabilities flagged `sellable` (today: protected text, per-message sealing, cover
carrier) may be referenced inside it. Selling image sending present-tense is now a build failure, not
a matter of editorial care.

### 10.4 The crawler learned the distinction

> A marketing page may describe a v1 capability in forward-looking language. The support matrix and
> the checkout summary may only state what current evidence supports. A present-tense capability
> claim outside those two surfaces still fails the gate.

Mechanically enforced, with six new fixtures:

| Rule | Failure mode it catches |
|---|---|
| Forward-looking framing | a marketing section names a capability that is not `Available`/`Beta` with no forward-looking marker |
| Matrix link | a marketing capability page does not link to `/docs/status` |
| Matrix completeness | the matrix silently drops a capability, or carries a row with no status |
| Checkout allowlist | a non-`sellable` capability appears inside a checkout summary |
| Badge drift | a badge disagrees with the manifest, on any surface |
| Section D | a forbidden phrase appears **anywhere** — absolute, framing-independent |

The section D ban stayed absolute rather than becoming surface-dependent. The `ratchet` capability
was **renamed** to "Ratcheting (protecting old messages)" because its own public name contained a
banned phrase; renaming was preferable to carving an exception into the ban.

### 10.5 Gates after the reframe

| Gate | Result |
|---|---|
| `check-claims` | 16 files, **0 failed** |
| `check-claims --self-test` | **23/23** fixtures (17 known-bad caught, 6 honest-copy cases not flagged) |
| `pricing-sync --check` | 18 markers, **0 drift** |
| `build-status --check` | matrix matches the manifest |
| `screenshot-matrix` | **270 captures, 0 failed, 0 unmeasurable** (15 pages × 6 widths × JS-on/off/reduced) |
| checklist arithmetic | rows 89/303 = headers = snapshot, 0 mismatches |

### 10.6 Documents updated

Master **r6 → r7** with a §0.5 delta and a rewritten §9 website row. The allowlist gained an
"Owner reframe" section recording the new presentation model and the corrected standard, A1/A2 moved
to `Beta`, and section B's blanket "never in the present tense" rule was narrowed to the matrix and
checkout. Checklist **88 → 89** (H5 +1 for the versioned support matrix).

### 10.7 Least sure about

The forward-looking marker list is a keyword check, not comprehension. A section that says
"Arriving at v1" and then describes the capability in confident present tense will pass the gate,
because the marker is present. It reliably catches a card with *no* hedge at all — which is the
failure mode that produced this whole exercise — but it cannot judge whether the hedge is prominent
enough to actually register with a reader. The matrix and the checkout are properly enforced; the
marketing pages are enforced only against the crude version of the mistake.

---

## 11 · Crypto lane adjudication (report landed 17:15)

`docs/reports/crypto-lane-2026-07-26.md`. Their section is titled "Checklist rows — nothing
claimed", and their restraint on the two rows they did discuss is correct:

- **D6 bilateral burn** — the control lane is wired rather than inert, but there is no runtime or
  two-identity evidence that a burn request crosses between two devices, and the keyserver
  revocation lane's deploy state is still contested in this repository. Correctly not claimed.
- **C5 attribution** — fixed in code and unit-proven at the producer, but the renderer half is
  proven only structurally and nothing has been observed on screen. Correctly not claimed.

I agree with both. Claiming either would be the "code written = point earned" failure.

### Where I overrode them — A8, +1, which they did not claim

They left **A8 · Secret zeroization and metadata minimization** at 2/4 without discussing it. I
awarded +1 after verifying the evidence in source myself:

- `crates/keystore/src/storage.rs:75` derives `Zeroize, ZeroizeOnDrop` on the secret-carrying
  struct; `zeroizing_an_inner_identity_clears_every_secret_field` (`:298`) asserts every secret
  field clears, recovery entropy included; `secret_carriers_wipe_themselves_on_drop` (`:344`) pins
  the derive.
- Identifier redaction is present in `crates/ipc` (`cipher_store_client.rs:811-812`, `:847`,
  `wire_v2.rs:1098`).

**Why a unit test is sufficient here and is not the shallow-test failure:** memory wiping on drop
cannot be observed end to end. A test that constructs the value, drops it and asserts the secret
fields are cleared *is* the appropriate evidence for the property. The rule exists to stop tests
standing in for user-visible behaviour that could have been demonstrated; that is not this case.

Not 4/4: master §10 medium finding 9 also covers plaintext identifiers **at rest**, and no
comprehensive at-rest audit was delivered.

Verified by source inspection, **not** by running cargo — this lane does not contend for the cargo
lock while another lane is building Rust. I am relying on the crypto lane's reported
`cargo test -p keystore` result (176 passed / 0 failed / 1 ignored, beating a 170/1 baseline) for
the fact that these tests pass.

### A correction to my own allowlist that their report forced

Their 0029 analysis does **not** contradict A7 — their heading says "not yet live" but the body
verifies *source*, whereas A7 rests on the owner's confirmation against the live migration list.
Those are different evidence, not a conflict.

It did expose an inaccuracy in my own wording. A7's required limitation read "switched off until
their owner re-registers", which implies manual work. It is automatic:
`ensure_keyserver_registered` runs on launch **and** unlock (`crates/ipc/src/commands.rs:7580`,
called from `bootstrap.rs:1296`, `password_lifecycle.rs:384`, `core_bridge.rs:306`), and the
register request carries the OSL `user_id` with no snowflake, so a quarantined identity re-enables
itself. Corrected in the allowlist. This is an overstatement in the *cautious* direction, which is
still an inaccuracy.

**Logged against A7, not owned by me:** `Identity` carries a `discord_snowflake` field, and any
peer-lookup path keyed on a snowflake rather than an OSL user id is permanently unresolvable under
0029. Needs an explicit check by whoever owns peer lookup. It does not falsify A7, but it could
break finding a contact.

### Checklist after this pass

**90 / 303.** Rows 90/303 = headers = snapshot, 0 section mismatches. Point alerts confirmed
delivered for 88 and 89 (`delivered-point.*.88`, `.89` in the bot state), so the one-alert-per-point
rule is working end to end.

---

## 12 · Scrub lane F1 adjudication, and the accessibility gate

### F1 · Local all-browser/all-profile detection — **+1 approved** (4 → 5 of 6)

Claimed by the Scrub lane; judged, not rubber-stamped. The row's stated remaining scope was
"remaining browsers, UI, revocation, exact release proof". This closes the **UI and revocation**
halves, and closes them at runtime rather than by compilation.

What convinced me, in order of weight:

1. **A negative control was performed.** Forcing `allows()` to always grant makes the test fail on
   exactly the default-deny assertion. That is the difference between a proof and a green light, and
   almost nothing else in this project has one.
2. **The root cause is real and explains a standing blocker.** `set_browser_profile_consent` and
   `list_browser_profile_choices` were registered in **no** Tauri command, so detection returned zero
   in any shipped build — which is the "0 accounts detected" symptom that has been sitting on the
   Scrub demo.
3. **Scoping was proven, not asserted** — every observation came from the granted seeded profile and
   the owner's real `Default` profile was never read.

Verified independently rather than taken on trust: both commands are in `generate_handler!`
(`apps/osl-hub/src/main.rs:4192-4193`), with matching ACL in `capabilities/hub.json`
(`allow-list-browser-profile-choices`, `allow-set-browser-profile-consent`) and
`permissions/hub.toml:474` — **no mismatch in either direction**, which matters because this repo has
previously shipped an ACL entry for a command that was never registered. The runtime test
`seeded_profile_is_read_only_after_consent` exists at `apps/osl-hub/src/native_apps.rs:6960`.

One correction to their report: it names the second command `list_browser_profile_candidates`; the
actual name is `list_browser_profile_choices`. A naming slip, not an overclaim.

Their deliberate non-claims (F1 → 6, F2, F4, F10) are correct. **No website claim changes:** this
proves per-profile consent and scoped reads, not end-to-end account discovery, so `scrub-discovery`
stays `Planned` on the matrix. That is also what the owner asked for — no whipsawing.

### Accessibility and 200% zoom — the gap H8 was docked for is now closed

`scripts/check-a11y.mjs`, 15 pages × 4 widths × {100%, 200%} = 120 combinations:

| Check | Result |
|---|---|
| Images missing `alt` | **0** |
| Controls with no accessible name | **0** |
| Horizontal overflow, any width or zoom | **0** |
| Tap targets under 44px | 3 distinct (33–41px wide × 44px tall) |

Two real bugs found and fixed: `/donate` scrolled sideways 47px at 768px because the `.mission-*`
illustration has **no CSS rules anywhere in the stylesheet**; `/audit` buttons were 40.8px because
`assets/css/audit.css` loads after `style.css` with an id-bearing selector.

I also had to fix the audit itself: it used `getBoundingClientRect`, which returns the *transformed*
box, so the reveal animation made compliant 44px buttons measure 41px. It now uses the layout box.
**H8 restored 2 → 3.**

Residual, recorded rather than hidden: three nav/footer links are 33–41px wide at 44px tall — WCAG
2.5.8 AA (24×24) yes, the 44×44 in §8.6 no. Padding short link text out to 44px wide would read
worse than the finding. Non-colour-only state and illustration text-equivalence remain unasserted.

### Delegation note

The `/audit` 40.8px hunt went to Codex (`codex exec -m gpt-5.5`, backgrounded, read-only prompt with
explicit may-not-touch files). It found what I had missed — audit.html loads a **second** stylesheet —
but **its proposed fix was wrong**: it suggested `.audit-file-actions .button.button` at specificity
(0,3,0), which cannot beat the real rule `#main .audit-file .button` at (1,2,0). Verifying in both
directions caught that. Fixed at source in `audit.css` instead.

### Checklist

**92 / 303.** Rows = headers = snapshot, 0 section mismatches. Deltas this stretch: A8 +1 (crypto
lane under-claim), F1 +1 (Scrub lane claim, approved), H8 +1 (restored).

---

## 13 · Release lane adjudication — 4 of the 6 proposed points awarded

Proposed: I3 → 2, I4 → 2, I6 → 2, I2 → 1 (**+6**). Awarded **+4**. Every number below was verified
by the writer against GitHub and the branch, not taken from the lane's summary.

| Row | Proposed | Awarded | Why |
|---|---:|---:|---|
| **I2** · authoritative integration line | +1 | **+1** | Approved |
| **I3** · green Rust/TS/selector/security CI | +2 | **+1** | **Cut.** Rust is red |
| **I4** · signed candidate, VM promotion, rollback | +2 | **+2** | Approved — strongest evidence in the batch |
| **I6** · governance / branch protection / PR cleanup / releases | +1 (to full) | **+0** | **Declined.** Two of four items untouched |

### I4 — approved, and the standard others should copy

The promotion gate is proven by **refusal**, not by a happy path.
`scripts/release/prove-promotion-gate.sh` contains **20 `expect_reject` cases**, all confirmed
present. Among them: *"attestation describes a different binary"* (substituted binary),
`mutate 'd["operator"] = "   "'` → *"no accountable operator"* (blank operator), *"CAPTCHA was
automated around"*, *"same golden snapshot restored twice"*, *"clean restore attested as a string,
not a boolean"*, *"no installer at all"*. A gate proven to refuse every tested way of promoting an
untested or substituted build is a real safety guarantee, which is exactly what the earning rule
asks for.

Held at 2 of 4: **"signed candidate" is not proven** — `required_signatures` is `false` on the
protected branch and no signing was demonstrated on a real candidate — and the reproducible build
has not been shown to actually reproduce.

### I3 — cut from +2 to +1, because the pipeline exists but does not pass

`gh run list` shows `Rust Test` = **`failure` on the four most recent runs** (2026-07-27 00:11,
00:20, 00:28, 00:40). All four ran on `release-lane-2026-07-26`, **not** on `main`; on `main` the
only workflow with recent runs is Selector CI.

TypeScript, selectors, the release audit and the new desktop-binary job are genuinely green, and
that is worth a point. But the row's headline promise is *green* CI **including Rust**. Scoring 2 of
3 would read as "nearly satisfied" while the heaviest gate fails every run. The three Rust defects
are not owned by that lane, so this is a scoring cut, not a criticism of their work.

### I6 — declined, because two of the four named items are untouched

Branch protection is genuinely on, verified via
`gh api repos/OSLPrivacy/discord-privacy-client/branches/main/protection`: force pushes and
deletions disabled, status checks required. That is real, and it is what earns I2 its point.

But this row also names **PR cleanup** and **releases**. `gh pr list --state open` returns **6 open
PRs** (#1–#6, several long-stale) and `gh release list` returns **nothing at all — zero releases**.
Full marks with half the row untouched is the rubber-stamp this role exists to prevent.

**Recorded against the protected line, not a deduction:** the required contexts are only
`["TypeScript gate", "audit"]` — **the Rust gate is not required** — and `enforce_admins` is
`false`. So the authoritative integration line can still accept a merge while Rust is red, and an
admin can bypass it entirely. Worth closing before the line is trusted.

### Recorded, not awarded

Logged in the checklist so they cannot be forgotten or later re-counted as new work: the
control-inbox **retry bound** (the lane must be a retryable queue, not a dropped notice), the
**Windows duress defect** (no production caller, known TPM-less failure mode — A7 already sits at 0
for this), **cryptographic burn not implemented** (owner decision: not now), and **Received/Opened
receipts collapsing** into one `acknowledgmentCount`.

### The 44×44 residual — fixed, not amended

I chose to meet the rule rather than change it. Weakening an accessibility standard for a cosmetic
reason is the wrong direction for a product whose users include people with disabilities, and the
extra width lands as spacing between links, which helps touch anyway. `check-a11y` across 120
combinations is now clean on all four measures: **0** images missing alt, **0** controls without an
accessible name, **0** tap targets under 44px, **0** horizontal overflow — verified to introduce no
overflow at 320px or at 200% zoom.

Still unasserted, recorded rather than hidden: non-colour-only state communication, and whether
every illustration has an equivalent text description (proving no image lacks `alt` is weaker).

### Checklist

**96 / 303.** Rows = headers = snapshot, 0 section mismatches.
