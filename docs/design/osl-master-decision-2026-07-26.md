# OSL master control specification — 2026-07-26

> **READ THIS FIRST.** This is the authoritative, compact control document for OSL product
> intent, current truth, agent coordination, testing, integration, and conflict resolution.
> An agent should be able to start or resume work by reading this file and then following the
> linked subsystem document for its task. Do not reconstruct product intent from old chats.
>
> **Control revision:** `OSL-MASTER-2026-07-26-r11`. A model/account reads this document completely
> the first time only. On later work it checks this revision and its saved memory card, then reads
> only changed sections and the linked subsystem/report. Every semantic edit must increment the
> revision and add a one-line delta to section 0.5.

This began as the adjudication of Plan A vs Plan B and now incorporates the owner's later product
decisions. It is authoritative where it conflicts with either plan, an older design document, a
code comment, memory, or chat excerpt.

## 0 · How to use and maintain this document

### 0.1 Authority order

Use the first applicable source:

1. The owner's current direct instruction.
2. A later dated owner decision recorded in this document.
3. This document's current specification.
4. A linked subsystem specification or accepted decision record.
5. Current source and exact-build runtime evidence for implementation status.
6. Task reports and dated handoffs.
7. Curated memory.
8. Old chats, old plans, comments, README claims, and marketing.

Product intent and implementation status are different. Current source/runtime evidence can prove
that a feature is incomplete, but it cannot silently rewrite the intended product.

### 0.2 Conflict rule

When two sources conflict:

1. Name both claims and cite their exact source.
2. Apply the authority order above. A later owner decision supersedes an older one.
3. Tell the owner in chat what conflicted, which claim wins, and why.
4. Update this document and every affected acceptance test, task, dependency, status, and public
   claim. Mark the losing text `superseded`; do not leave two apparently-current truths.
5. If both choices satisfy the specification and the distinction is not material, choose
   automatically using this order: safer and more private; easier to prove; simpler; more
   reversible; fewer permissions/data; better compatibility. Record the choice in one sentence
   and continue without bothering the owner.
6. Do not autonomously choose between materially different privacy, security, payment, wire,
   persistence, destructive-action, or irreversible-migration behaviors. Recommend the closest
   option and obtain the missing owner decision.

### 0.3 Status vocabulary

Use only these labels:

- `verified-live`: proved on the exact stated build in the real required environment.
- `runtime-proven`: the relevant runtime boundary was exercised, but the whole user promise was not.
- `test-proven-only`: automated tests pass; real runtime/product proof is still missing.
- `implemented-unwired`: source exists but no proven production call path reaches it.
- `designed-only`: specification or scaffold only.
- `externally-blocked`: required behavior depends on an unavailable third-party surface.
- `open-security-finding`: implementation exists but a known security finding blocks the claim.
- `unknown-recheck-required`: evidence conflicts, is stale, or does not identify the exact build.
- `superseded`: historical only.

Never turn “code exists” or “tests pass” into “works.”

### 0.4 Compactness rule

Capture a fact once and link to it elsewhere.

- This master file contains the product contract, status row, owner decision, acceptance summary,
  dependency, and link.
- A subsystem document contains architecture, detailed task graph, interfaces, test matrix, and a
  short `Resume here` block.
- A task report contains the exact diff, commands, build identity, evidence, failures, and next
  handoff.
- Memory contains only a short durable fact that prevents rediscovery. It must link back here or to
  the detailed report.
- Agents must not paste their full exploration into this master file. Update only the delta.

### 0.5 Revision digest

- `r11` — **correction to the r10 gate advice, which was itself half of the truth.** r8/r10 said the
  honest Rust gate is `--features core,discord-qa-shell`. That gate turns a **security check off**
  while turning coverage on: `header_proof_is_enforced()` is literally
  `!cfg!(feature = "discord-qa-shell")` (`apps/osl-hub/src/native_discord_adapter.rs:4529-4531`,
  verified). **Neither gate alone is sufficient** — plain `--features core` hides the
  `qa_selftest_request` module, and `core,discord-qa-shell` relaxes header-proof enforcement. A test
  passing under the QA feature has **not** proven header proof is enforced. **Always quote the gate
  beside the number; "741 passed" means nothing on its own.** Truth-lane assessment: **no current
  allowlist row is load-bearing on header-proof enforcement** — sender attribution is already an
  open finding (§10 finding 2) so no claim rests on it, and A1/A2 are confidentiality rather than
  header authentication. Same family as the rest of tonight: a build configuration that makes a
  check disappear looks identical to one where the check passes.
- `r10` — **correction to the r8 Rust-gate note, which was true but could be read as more
  reassuring than it is.** Two distinct facts, verified in source: (1) `qa_selftest_request.rs` and
  its tests are gated `all(core, discord-qa-shell)`, so plain `--features core` omits them — that is
  the 731-test gate; but (2) the **dispatch itself** lives in `apps/osl-hub/src/main.rs:5009`, and
  `main.rs` is a bin with `required-features = ["desktop"]` (`apps/osl-hub/Cargo.toml:54-57`), so
  `--features core` does not compile `main.rs` **at all**. Consequence: *no* Linux lib-test run
  exercises that dispatch, including the corrected 731 gate — **only the Windows desktop build
  does.** Do not read "run the 731 gate" as "the irreversible-send dispatch is covered".
  Related live trap: the QA trigger dispatches on the first non-whitespace byte being `{`, so a
  UTF-8 BOM — which PowerShell's default `Set-Content` emits — fails that test and falls through to
  the **legacy SEND verb, which has an irreversible side effect**. Source:
  `docs/qa/two-identity-p2p-verification.md`, and the generalisable rule recorded there — **when a
  capability looks missing, check the caller before the callee.**
- `r9` — burn language corrected across the repository, which had been stating two positions at
  once: `THREAT_MODEL.md` said the words were unearned while 26 older statements still claimed
  cryptographic erasure. **`README.md` — the public front page — is the important one**: it said
  "Burn is local cryptographic erasure. It destroys keys, not messages", which is exactly inverted,
  since `wrapped_key` is never populated so there is no key to destroy. Corrected to what burn does:
  local shredding plus server-side deletion plus a cooperative peer request, with no destruction of
  the only decryption capability. `burn-contract.md`, `osl-hub-feature-parity.md` and §8.4 here also
  corrected; the per-message wrapped-key design docs are **labelled unbuilt rather than rewritten**,
  because that model is deliberately deferred. Two phrases added to allowlist §D — "destroys keys,
  not messages" and "permanent ciphertext" — and to the website crawler, so neither can creep back
  through a copy edit. `THREAT_MODEL.md` local-destruction claim now records that it was **false
  until 2026-07-26** (re-`put` resurrection and a `mark_burned` short-circuit, both fixed by the
  store lane with a both-directions control).
- `r8` — keyserver escalation **closed**. Migration `0027` is deployed and there is now a named
  Worker version, which is the exact condition the §9 row and the coordination trap were blocked on:
  keyserver `3f92f0f5`, cipher-store `0a17547d`, `0029` applied. The `NOT DEPLOYED` header in the
  0027 migration file is stale — ignore it; the keyserver lane owns correcting it. `0028` is applied
  but dark behind a default-off flag. Also recorded: attachment part upload returned **500 in
  production for an unbounded period** and now returns 201 under `0a17547d`, so any pre-2026-07-26
  attachment success is unproven; and the honest Rust gate is
  `--features core,discord-qa-shell` (731 tests) because plain `--features core` silently omits the
  module deciding whether a trigger becomes a status read or the irreversible send. Coordination
  layer: `docs/reports/coordinator-state-2026-07-26.md`.
- `r7` — **owner reframe: the website is pre-launch marketing for v1, not a live status dashboard.**
  A site with no `Available` badge reads as a dead product, which is its own dishonesty. Feature
  cards no longer carry per-card `Planned` stamps; the full v1 product is shown with real
  explanations. Honesty moves to two concentrated, machine-enforced surfaces: the generated dated
  support matrix at `/docs/status` (§8.4) and the point of sale, where Pro is sold as an explicit
  early-access purchase naming what works today versus what arrives at v1. `check-claims` learned
  the distinction — marketing may use forward-looking language, matrix and checkout may only state
  current evidence, and a present-tense capability claim outside those two surfaces still fails.
  Section D forbidden phrases stay absolute on every surface. **`per-message-sealing` corrected back
  to `Beta`:** `implemented-unwired` describes a call path, not whether the code works, and live
  sends have landed real covers. The other four downgrades hold on the "does the code do this"
  standard; Scrub marketing is deliberately not watered down. Checklist **89/303** (H5 +1, the
  versioned support matrix).
- `r6` — truth lane pass on the website and the claim pipeline. `data/pricing.json` now provably
  drives every pricing surface; the missing no-renewal limitation was added to `index.html`, which
  had shown two `$5 / month` checkout buttons with no renewal disclosure at all. Five capability
  badges downgraded after source verification (per-message sealing → `Planned`, image sending →
  `Planned`, cover carrier → `Beta`, Scrub discovery and guided deletion → `Planned`), leaving **no
  `Available` capability badge on the site**. `osl-public-claim-allowlist.md` gained rows for Scrub,
  AutoScrub, link protection, exposure warning, AI carrier text, processing credits and the
  Illustration items, so every badge now maps to a row. The §8.6 claim crawler exists and is proved
  by known-bad fixtures. §9 website row updated. Checklist corrected to **88/303**: section H's
  header had understated its own rows by 2, H8 was an overclaim (−1), H1 and J7 earned +1 each.
- `r5` — reconciled `docs/THREAT_MODEL.md` against current source (five claimed security
  properties restated to what stateless `v=3` provides, rest marked `Planned`); resolved three
  live conflicts under §0.2 (eye `BLOCKED` → `runtime-proven`; keyserver 0027 →
  `unknown-recheck-required`, escalated; bilateral-burn description corrected — the drain now
  deletes the revocation notice unapplied); added
  `docs/design/osl-public-claim-allowlist.md` (closes §24 item 5); §9 rows updated for
  identity/duress, live v3 crypto, ratchet-next, eye, lifecycle, and keyserver.
- `r4` — locked Pro billing as a prepaid one-month activation code whose period starts at
  redemption, never at purchase; no payment data stored; compute credits are separate one-time
  purchases. New section 7.14; 8.2 pricing bullet resolved. See
  `docs/decisions/pro-prepaid-monthly-codes-2026-07-26.md`.
- `r3` — Telegram `/osl` test-proven foundation implemented and loaded; activation/live evidence
  remains an owner command.
- `r2` — added the deadline register, versioned AI memory contract, high-only reasoning preference,
  subjective-design authority, and new-model review protocol.
- `r1` — consolidated Plan A/B, later owner decisions, workflow, testing, Telegram, and derived
  project views into the first master control specification.

## 1 · Adjudication: Plan B is the spine, Plan A supplies the cut list

**Decision: implement `osl-plan-b-safety-first.md` as the sequence, and use
`osl-plan-a-ship-first.md` for two specific things only** — its support-set analysis and its list of
literally-unreachable requirements.

**Why B wins.** Every headline claim this product makes is a *security* claim. The audit
(`docs/security/osl-audit-2026-07-26-codex.md`, 13 findings: 5 critical, 2 high, 6 medium) found
that several of those claims are **not currently true** — including two wrong-recipient classes and
one plaintext-at-rest class. For a privacy tool, shipping quickly on a claim that turns out to be
false is the one failure that cannot be walked back: the product's entire value is the user's belief
that it does what it says. A late feature is a disappointment; a false security claim is a betrayal.
So correctness sequences ahead of scope.

**What A contributes, and it is genuinely valuable.** A's analysis of which requirements are
*literally unreachable* prevents burning weeks on them, and its support-set reasoning is the right
tool for deciding what a v1 may honestly claim. Specifically, carry forward: 0-frame *engage* is not
reachable as specified (capture resistance must be proven before the composer is visible, so the
honest target is low tens of ms, not zero — 0-frame *follow* and *scroll* are reachable), and the
discipline of stating plainly in-product what a version does not yet do.

**Where B overreaches:** B treats "unit tests exist" as insufficient and demands measurement for
everything. Correct in spirit, but applied uniformly it stalls. Apply it strictly to anything
carrying a security claim, and accept tests as sufficient for pure logic with no runtime surface.

## 2 · Revised priority order

**P0 — the five criticals, before any new feature work.** Nothing else ships first. In order:

1. **Keyserver key substitution** — a keyserver can swap a recipient's encryption keys without
   changing the TOFU identity. The safety number binds only Ed25519; X25519/ML-KEM ride along.
   This defeats the ceremony we are simultaneously making real, so it must be fixed in the same
   breath: the safety number must cover the *whole* key bundle, or the bundle must be signed by the
   identity key and verified on every use.
2. **v3 receive misattribution** — the generic receive path authenticates an in-band key but
   attributes the plaintext to a *caller-supplied* identity. Authentication and attribution must
   name the same key, or "who sent this" is a guess.
3. **Open self-signed registration** — an attacker can pre-register another user's Discord
   snowflake and become the key everyone fetches. First-write-wins on an unproven identifier is not
   a binding. Requires a keyserver change; see the audit for the endpoint.
4. **Non-image attachments decrypted to durable plaintext files** — direct violation of the
   standing no-plaintext-at-rest rule, and the exact thing the product promises does not happen.
5. **Recovery secrets render after capture protection fails** — TOCTOU. Protection must be *proven
   applied* before a single secret pixel is drawn, the same ordering constraint that makes 0-frame
   engage unreachable.

**P1** — the two highs, then friending P0-1/P0-2 (already in flight), then the ratchet wire-in.
The eye is now proven (`rehydrate_rows_placed count=3`), so gate 1 of the ratchet wire-in is clear.

**P2** — feature work below, and the remaining apps.

### 2.1 Deadline register and deadline behavior

Deadlines are part of scope authority, not casual chat. Record every future deadline here and in
the compact memory/deadline card required by section 11.1.

| Deadline | Class | Deliverable | Minimum honest acceptance | Status |
|---|---|---|---|---|
| **2026-08-02** | Hard external demo | A working, video-demonstrable Scrub vertical slice Zhao can show his father | Exact Windows build; simple detection/accounts → scan → review flow; safe verified action on disposable/test data or clearly labelled non-destructive demo; visible progress/stop; evidence-backed video; no false production claim | active |
| **2026-08-02** | Parallel target; safety cannot be relaxed | Stronger encryption exercised with controlled test identities | Gated two-identity handshake/send/receive, attribution, offline/restart/drain evidence; uncontrolled user traffic stays feature-gated until independent review | active |

Deadline rules:

1. Store date/time zone, hard-versus-target class, deliverable, acceptance, dependencies, owner,
   source instruction, and last-reviewed date. Never save only “next weekend.”
2. Recalculate the critical path, weighted progress, velocity, confidence, and ETA when a deadline
   or scope changes.
3. `/osl` warns when projected completion crosses a deadline, not only after it is missed. Include
   the smallest honest scope cut and what remains after the demo.
4. Prefer a complete demonstrable vertical slice to many half-built surfaces. Never hide a safety
   failure, fake a destructive result, or route uncontrolled users onto unaudited crypto for speed.
5. Keep Scrub and encryption parallel where dependencies/files allow. If the encryption target
   threatens the hard demo, preserve its gated test work and finish the Scrub demo.
6. Resolve or supersede a deadline explicitly; never silently delete it after the date.

## 3 · Screenshot protection — two-party consent (new requirement)

### 3a · View-once requires the viewer to consent to protection

A view-once image is posted as a **button covering the flag-image**. Clicking it is the consent
gesture: the viewer agrees to have screenshot protection enabled **temporarily**, for the duration
of the view. No consent, no decrypt — fail closed.

- The consent must be **informed**: the viewer is told protection will be switched on for their
  window while the image is shown, and switched off after.
- Protection must be **proven applied before the first pixel**, per P0-5. A viewer who consents but
  whose platform refuses protection must be told and shown nothing — never shown the image anyway.
- Restore the prior protection state afterwards, and restore it on crash. A temporary state that
  latches on is a bug; a temporary state that latches *off* is a security bug.

### 3b · Chat-wide screenshot protection flips only on matching two-party consent

Per-conversation, opt-in, with a symmetric flip rule the owner specified:

> Each party independently sets their own opt-in/opt-out. **When both parties' settings match each
> other and differ from the current state, the state flips.** Default on first whitelisting a
> person: **off**.

Design consequences that must be got right:

- **This is a state machine over two independent booleans plus a current state**, not a request/
  approve handshake. Model it exactly that way; a handshake would let one party's stale request
  flip the state later, which is the bug this design avoids.
- **Both directions need consent.** Turning protection *off* requires agreement just as turning it
  on does, otherwise one party can unilaterally strip the other's protection.
- **Never claim more than it does.** Screenshot protection is a platform affordance; it does not
  stop a phone camera, a modified client, or a machine where it silently fails. The UI must say
  what it actually does. Given the product also offers a *stronger* framing elsewhere, this feature
  is the one most likely to be over-read by users — keep the copy narrow.
- Report the *effective* state, not the requested one. If protection is enabled but the platform
  refused it, both parties must see that.

## 4 · Flag-images and flag-files (new requirement)

Cover artefacts for non-text payloads, matching the existing flagtext concept.

- **Flag-images and flag-files must exist.** If the attachment lane has no cover artefact today,
  create them — the same role text covers play for messages.
- **Size parity:** a flag-image must occupy the same rendered size in the Discord row as the image
  it stands in for, so painted content overtakes it exactly. This is the image analogue of the
  existing rule that flagtext must carry the same number of lines as the plaintext.
- The view-once button in 3a sits on top of the flag-image / flag-file.
- Audit finding P0-4 above is in this lane; fix it as part of this work, not after.

## 5 · Carried forward from the addendum

`osl-requirements-addendum-2026-07-26.md` items A1–A6 stand unchanged: view-once text is wiring not
building, read receipts need opt-in/mutual policy, the warn dialog offers a timer ladder with the
L3 tier distinction enforced, view-once images get a dedicated timed viewer, no cross-restart
plaintext drafts, and `#[serde(default)]` on every new preferences field.

## 6 · One-sentence product idea

OSL is a Windows-first, local-first privacy control center that adds trustworthy identity,
encryption, safe presentation, lifecycle control, exposure warnings, and cleanup to accounts and
apps people already use, while also providing first-party private chat, files, notes, and mail
where dependence on a host platform is undesirable.

OSL owns plaintext, keys, trust, policy, receipts, protected rendering, and destructive-action
proof. A connected host app retains its native interface, network, and ordinary functionality and
sees only the structure-compatible carrier where the platform permits that design.

The product is not protection against a fully compromised device, a camera, a modified recipient,
or deliberate blocking by the host platform. It must never market a limitation as a guarantee.

## 7 · Locked owner product decisions

These decisions are later than conflicting older documents and therefore win.

### 7.1 Text, image, video, and file carriers

Every protected payload has a harmless carrier that matches the replaced content's visible host
structure as closely and measurably as the host permits:

- Text carrier: same rendered line count and row shape as the protected text.
- Image carrier: same rendered dimensions, aspect, row footprint, and plausible visual “look.”
- Video carrier: same accepted file/container type, dimensions, duration/thumbnail footprint where
  possible, and row footprint.
- File carrier: same expected file type, visible filename/media presentation, and row footprint.
- “Approximately similar” is not completion. Each adapter must measure the native result and state
  any host transformation that makes exact parity impossible.

Free text carriers use the bounded word-bank generator.

Pro may use context-aware AI-generated carrier prose that considers a bounded recent conversation
window and attempts to form a coherent conversation. It may appear one-sided when the other party
does not use Pro. This is an **opt-in feature only**:

- The enable screen states that recent conversation context will be processed.
- The user chooses local processing or OSL cloud processing.
- Local processing is the privacy-preferred option and consumes no cloud processing credits.
- Cloud processing requires separate informed consent and a persistent visible indication. It
  warns that it is less private than local processing because selected context must reach a server.
- Cloud context is minimized, encrypted in transit, excluded from training by default, retained
  only for the bounded processing operation, and deleted after completion. Do not describe this as
  end-to-end encrypted if the service can see the context.
- Pro includes a small allowance of processing credits. When exhausted, the user may add supported
  cryptocurrency or use Stripe to purchase more processing credits for the OSL account.
- Payment and credit ledgers must not contain message text, conversation names, carrier text, or
  recipient identity. Where practical, redemption uses unlinkable short-lived credit tokens rather
  than attaching every generation request to the billing identity.
- Exhausted credits or unavailable cloud AI fall back safely to local AI when available or the free
  word bank. Encryption must never become unavailable because an AI service or payment is down.

The carrier is not the cryptographic security boundary. Knowing or detecting that text is a carrier
must not weaken confidentiality or authenticity.

### 7.2 Three configured send-authority modes

Setup explains and lets the user select one of three service-specific modes. Preferences are
versioned and safe-defaulted.

1. **Enter → Clipboard — default.** Enter in OSL encrypts and copies the carrier to the clipboard.
   The OSL composer disables until the user deliberately re-enables it. The user manually focuses
   the native composer, pastes, and presses Enter. OSL must not claim the host accepted the message
   unless it subsequently observes bounded sent proof.
2. **Double Enter.** The first Enter encrypts, temporarily dismisses/surrenders the OSL composer,
   and places the carrier into the native composer. The second Enter is the user's final send
   authority.
3. **Single Enter.** Enter encrypts, keeps the OSL composer visually stable, places the carrier, and
   sends automatically after exact target, placement, and focus proof.

Pro can additionally select **typing simulation**, in which the native carrier is paced into the
host composer as the owner types in OSL. It must never leak protected plaintext, switch
conversation, or allow a partial/unproven carrier to send.

No mode may silently fall back to another mode or to plaintext. The UI states the selected mode.

### 7.3 Production delivery proof

Production performs a bounded sent-message proof. The public result is tri-state:

- `sent`: host acceptance was positively proved.
- `not_sent`: OSL positively proved that no send occurred.
- `delivery_uncertain`: Enter/send authority may have been exercised but acceptance could not be
  proved.

`delivery_uncertain` must never be automatically retried. The UI says to check the conversation
before retrying. This prevents duplicate sends after a successful host send whose proof failed.

### 7.4 Lock, composer, eye, and cyan ring

- Lock controls protected input/send routing.
- Eye controls decrypted display only.
- When Lock is off, the OSL protected composer fully disappears and the ordinary host composer is
  available.
- When Lock is on, OSL's protected composer covers the host composer and remains stable until the
  selected send mode deliberately surrenders it.
- The small cyan ring around the native composer footprint means **encryption/protected input is
  active**. Its absence means the user is using the host's ordinary plaintext composer.
- The ring must be visible in every supported theme, DPI, zoom, and forced-colors mode without
  becoming a large mismatched highlight slab.
- Agents and harnesses must never inject protected-test input unless the cyan-ring safety gate is
  positively measured first.
- Eye off shows the untouched carrier/native rows. Eye on paints only authenticated, correctly
  bound plaintext over current native row geometry. Ambiguity leaves the carrier visible.

Each app has a first-launch, click-through onboarding tour explaining Lock, Eye, cyan ring, send
mode, app-specific limitations, and how to return to the tour later.

### 7.5 Embedded and native-session Scrub execution

Scrub is local account/content discovery plus safe deletion of the owner's own content. It is not
account deletion and is not a broker-removal service unless a separately named feature says so.

- The desktop app may run one or more isolated embedded WebViews.
- During automated work the Scrub UI provides a view-only monitor of each surface. The owner cannot
  click into the controlled page and corrupt the run, but can always pause, stop, or revoke it.
- With explicit consent, Scrub can reuse a selected signed-in native-app session when a verified
  adapter exists.
- If the user has additional accounts signed into browsers, the equivalent service may also run in
  isolated WebViews.
- Concurrency is resource- and dependency-aware. Multiple independent WebViews/apps may run when
  system load is safe and the owner is not using those exact surfaces.
- Presence is transport-scoped. Background protocols such as IMAP can continue while the owner
  uses Discord or other apps. UI automation pauses only when the owner contends for that specific
  hosted surface.
- Every destructive flow remains `Scan → Preview → Confirm/authorized plan → Execute → Verify →
  Receipt`. Requested deletion is never displayed as verified deletion.

Browser import consent is separate from deletion consent. Initial import is per selected browser;
the result reports which profiles were actually read and permits profile-level revocation.
Firefox username decryption is a separate opt-in, username-only operation; password material is
structurally out of reach. The website/phone never receives browser-profile data.

### 7.6 Free Scrub, Pro AutoScrub, and optional proprietary module

- Free Scrub and its contracts remain open source and require attended review/confirmation.
- Pro AutoScrub may reuse an owner-approved plan and operate unattended within strict native
  authority, bounds, pacing, stop conditions, visible global status, and an always-available stop.
- Only the minimum AutoScrub implementation that would make trivial cloning possible may remain
  proprietary. This exception does not extend to cryptography, trust, deletion receipts, data
  formats, policy contracts, or the free implementation.
- The proprietary AutoScrub module is **not installed by default**. A Pro user explicitly chooses
  Settings → Pro → Download AutoScrub, receives a plain closed-source warning, and consents before
  download/installation.
- A reproducible fully open-source build must remain usable without the module.
- The module must not contact OSL servers except for a separately consented operation that is
  described in-product. Network tests must prove the default boundary.
- Optional cloud-hosted AutoScrub requires separate consent and states plainly that OSL temporarily
  receives sensitive credentials/data and that the operation is not fully end-to-end private.
  Secrets/data are minimized, isolated, access-audited, expired, wiped at completion, and followed
  by a deletion receipt and a recommendation to revoke temporary credentials.

### 7.7 Website/phone Scrub boundary

The small website/phone version is a username-only public exposure sweep:

- It accepts a username supplied by the user.
- It sends no browser data, cookies, native-app data, passwords, or local account inventory.
- A result is shown only when a service-specific discriminator verifies it. Login walls, bot
  blocks, ambiguous status codes, and Instagram-like unverifiable responses count as `unchecked`,
  never `found`.
- The receipt says how many sources were checked, found, absent, blocked, and unverifiable.
- Its purpose is an honest small demonstration that leads to the desktop app for deep local
  discovery. It must not fake a dramatic count.

### 7.8 Major-release app gate

Every app OSL currently presents as available/supported must satisfy the common adapter acceptance
contract before a major release. An app explicitly labeled `Coming soon`, `Experimental`, or
`Externally blocked` is not offered and does not satisfy marketing as supported.

This is a literal release gate, not permission to pretend an inaccessible surface works. Discord is
the reference adapter. Signal and WhatsApp need their own runtime proofs. Stable Telegram remains
externally blocked if it cannot expose reliable accessible message rows. Outlook is scoped as OSL
Mail rather than falsely treated as an ordinary chat overlay.

### 7.9 Notes and Creative

Notes/Creative is a background passion-project priority and may show honest `Coming soon` labels in
the first release. Minimum first-release functionality:

- local encrypted notes with links/basic Obsidian-like organization;
- encrypted file storage and sharing;
- safe import/export with clear metadata consequences;
- a small number of stable low-level editing features.

Larger office, media, 3D, prototyping, plugin, and collaboration goals are staged milestones, not a
single release gate.

### 7.10 Open-source position

OSL is fully open source except for the narrow optional AutoScrub module described in 7.6. Security
claims, crypto, wire formats, trust, receipts, local storage boundaries, free Scrub, and the
default client remain inspectable. Website and app copy must state the exception consistently.

### 7.11 Stronger encryption

Integrate the stronger ratchet immediately and exercise it with controlled test identities.
Uncontrolled user traffic remains behind a feature gate until independent cryptographic review.

Minimum gates before uncontrolled traffic:

- wire-type registry has no collision;
- authenticated identity binds the full encryption bundle;
- sender attribution cannot differ from the authenticated sender;
- monotone capability/downgrade pin;
- sealed persistence and crash/reorder/replay/skipped-key tests;
- controlled two-identity handshake, send, receive, restart, offline queue, and recovery proof;
- external review findings resolved or explicitly accepted by the owner.

No public “Signal protocol,” “better than Signal,” forward-secrecy, post-compromise, or
post-quantum-authentication claim may outrun the audited live path.

### 7.12 Exposure warning

OSL may warn before potentially sensitive content is sent to an unencrypted host composer. The copy
is neutral consequence language—“This message was detected as potentially sensitive”—not a moral
or political judgment.

- Categories include slurs/targeted hatred, racist or Nazi imagery/jokes, explicit sexual content,
  graphic violence, credentials, business secrets, and other high-consequence disclosures.
- Ordinary political discussion is not itself a warning category.
- Deterministic local detection normalizes Unicode confusables, separators, and leetspeak before
  exact matching; collision/noise tests are mandatory.
- Context such as quotation/reporting may lower confidence, never silently erase a high-risk match.
- The unencrypted-composer warning is on by default. Protected-composer warning is independently
  configurable and off by default.
- Pro may opt into a local AI classifier. Cloud classification follows the consent, privacy, and
  credit rules in 7.1.
- Prefer continuous local warning before Enter. Any low-level keyboard hook capable of swallowing
  Enter must be separately disclosed, narrowly active only for the target composer, and proven not
  to buffer or inspect unrelated keys.

### 7.13 Native app session lifecycle

OSL must not read/copy Discord tokens, cookies, LevelDB, or a live profile to create a second
session. Supported designs are:

- with consent, close and relaunch the owner's ordinary Discord under OSL control using the
  supported user-data path, then close/restore it according to the selected lifecycle; or
- an OSL-owned isolated WebView/native profile with a one-time supported login.

No “same account, second window, zero login” implementation may be achieved by token extraction.

### 7.14 Pro billing — prepaid one-month activation codes

Zhao's 2026-07-26 decision. Detail and the required keyserver change are in
`docs/decisions/pro-prepaid-monthly-codes-2026-07-26.md`; the website's machine-readable source is
`data/pricing.json` in the website repository.

- **Pro is $5 for one month.** The website sells exactly one thing: a one-month activation code.
- **Payment information is never stored, including for repeat purchases.** No stored card, no Stripe
  customer profile, no recurring mandate, no cancellation obligation, no OSL account required.
- **This is not a subscription.** Nothing auto-renews. To continue, the user buys another code.
  Copy must never imply a stored card or a cancellation duty.
- **The month starts when the code is entered into the OSL client**, never at purchase. Two separate
  clocks: *shelf life* (how long an unredeemed code may still be entered) and *entitlement period*
  (the 30 days of Pro). A code left unused for months must still deliver a full 30 days.
- **Redemption must be an explicit call, not a side effect of validation.** The client validates
  repeatedly; burning a month on a background health check is an unrecoverable user-facing bug.
- **Compute-based extras are separate one-time purchases** (processing credits, master §7.1). They
  never renew. They may be explained as `Planned` but not sold until a purchase/balance endpoint
  exists and the ledger provably excludes message text, conversation names, carrier text, and
  recipient identity.
- **Expiry never breaks encryption.** At expiry the app returns to Free; protected text must continue
  to send and decrypt.
- **A redemption record is `(licence hash, timestamp)` only** — no account, email, device
  fingerprint, IP, install ID, or joined payment identifier.
- Existing codes are owner-generated test artefacts; nobody paid. They convert to monthly and may be
  wiped, after checking which are in use on Zhao's own machines.

Current implementation truth (verified 2026-07-26): the deployed checkout is already a single
one-time charge, which is correct, but a paid code grants **lifetime** access
(`subscription-state.ts`: `checkout.session.completed → lifetime ACTIVE`) and comp codes have their
period fixed at mint. No redemption timestamp exists anywhere. Until that is fixed the website may
publish the price, the "nothing renews" and "nothing stored" claims, but **not** "your month starts
when you enter the code".

## 8 · Website workstream and release checklist

Zhao is the owner. Jester is the head developer. Their July 23–26 messages are design and priority
context, not implementation evidence. “Done” still requires current source and deployed proof.

### 8.1 Canonical website state

There are currently multiple competing website realities:

- local checkout `/mnt/c/Users/liamw/projects/oslprivacy-web` at an older May revision;
- public GitHub `main`;
- a newer preview deployment;
- the public production deployment.

Before feature work, declare one canonical source branch/commit for production and record the exact
commit in every preview/deploy report. Never copy changes between these states by eye.

### 8.2 Launch-blocking website truth fixes

- **Resolved 2026-07-26 (see 7.14):** the commercial model is a prepaid one-month $5 activation code
  plus separate one-time compute credits. `data/pricing.json` in the website repository is the single
  pricing manifest and drives homepage, download/checkout, FAQ, terms, privacy, receipt, and app
  entitlement copy. The older `$5 once`, `$50/year`, and any subscription/renewal wording are
  `superseded` and are rejected by the claim crawler.
- Reconcile recovery language: distinguish “OSL cannot server-reset your password” from recovery
  material the user saved locally.
- Mark every capability `Available`, `Beta`, `Experimental`, `Planned`, or `Illustration`.
- Do not market attachments, view-once, expiry, burn, Scrub, or app support as available before the
  exact release build has the required evidence in this document.
- State the open-source AutoScrub exception consistently with 7.6 and 7.10.
- Update crypto-payment privacy copy if Bitcoin/Monero checkout is already presented.

### 8.3 Small website/phone Scrub checklist

- [ ] Compact phone-first username input and one clear `Check exposure` action.
- [ ] Username-only boundary stated before submission; no browser/app import on the website.
- [ ] Service-specific verified results only; ambiguous/block/login-wall responses are `unchecked`.
- [ ] Coverage receipt: checked, found, absent, blocked, unverifiable, last-reviewed date.
- [ ] Each result identifies the source and why it was accepted as a match.
- [ ] Free workflow explains manual review/removal guidance.
- [ ] Pro AutoScrub explains supported recurring checks/actions without promising universal deletion.
- [ ] Desktop call-to-action explains that deep local account discovery happens only in installed OSL.
- [ ] DeleteMe comparison/research covers scope, price, ongoing monitoring, removals, data handling,
      proof of completion, and limitations without copying marketing claims as fact.
- [ ] Phone widths 320/360/390 px: no clipping, horizontal scrolling, tiny illegible type, or hidden
      result controls.

### 8.4 Product explanations and comparison pages

- [ ] Define PWS/Privacy Warning System once. PWS acts before disclosure; Burn acts after disclosure.
- [ ] Add a Burn explanation and animation that separates local state deletion (real, and not
      cryptographic erasure), authenticated
      cooperative peer request, host-platform deletion attempt, and unpreventable copies/screenshots.
- [ ] Add a versioned support matrix with connector, protected send, protected receive, attachments,
      Scrub, verification date, provider-policy risk, and status.
- [ ] Add an honestly sourced major messenger/email comparison: content visibility, metadata,
      account/phone requirement, recipients/timing, backups, identifiers, defaults, weaknesses, and
      what OSL changes or cannot change.
- [ ] Identify OSL disadvantages with the same prominence as competitors' disadvantages.
- [ ] The website score is a retrospective/default-settings comparison, not a live device scan.
- [ ] The app may separately provide a timestamped live protection score based on currently observed
      settings/evidence and list every point of failure.
- [ ] “Anti-spyware” remains a research concept. Until OSL can define and independently test malware
      detection/remediation, call it a privacy-posture/risk monitor and retain the compromised-device
      threat-model exclusion.
- [ ] Cite primary sources and last-reviewed dates for every messenger/email/security comparison.

### 8.5 Visual feedback from owner/head-developer thread

The photos are not preserved in this document, so the linked implementation task must attach them or
translate each into a screenshot/evidence reference before editing.

- [ ] Fix visibly cut-off content.
- [ ] Replace “unlimited messages” positioning with Scrub; Pro receives AutoScrub.
- [ ] Remove the unwanted square/box identified in the referenced screenshot.
- [ ] Remove the disliked overlayed send-button treatment.
- [ ] Where two send controls are compared, use the same shape; OSL is cyan and native/underlying is
      grey.
- [ ] Animate the approved interaction, with reduced-motion and deterministic final-state support.
- [ ] Apply the requested square styling where the referenced screenshot requires it; do not infer
      which components from text alone.
- [ ] Keep meaningful content visible without JavaScript and before scroll. Current reveal animation
      can make most of a mobile full-page screenshot blank.
- [ ] Use design tokens, minimum 44×44 px tap targets, accessible labels/focus, 200% zoom, and
      non-color-only state communication.

### 8.6 Website objective gate

- [ ] Clean clone of the canonical commit reproduces deployed assets.
- [ ] Preview exposes its commit/build identity.
- [ ] Automated crawl finds no conflicting price/renewal/entitlement claims.
- [ ] Screenshot matrix: 320, 360, 390, 768, 1024, and 1440 px; JavaScript on/off;
      `prefers-reduced-motion`; before scroll; 200% zoom.
- [ ] Full-page screenshots contain all meaningful sections without having to trigger intersection
      observers manually.
- [ ] Every illustration has equivalent explanatory text and an accessible name.
- [ ] No logo implies connector support unless the support matrix says so.
- [ ] Website claims are reconciled against the exact current OSL release evidence before deploy.

Detailed website files currently include `index.html`, `pricing.html`, `download.html`,
`audit.html`, and `docs/{faq,getting-started,how-it-works,privacy,terms,threat-model}.html` in
`/mnt/c/Users/liamw/projects/oslprivacy-web`.

## 9 · Current implementation truth snapshot

This table is a coordination snapshot, not a substitute for reading the current diff. Concurrent
tabs are changing several rows. An agent must recheck the exact worktree/build before updating one.

| Subsystem | Current honest status | Completion proof still required |
|---|---|---|
| Identity/recovery/trust | `open-security-finding`; unlock/duress/auto-lock are `implemented-unwired` (verified 2026-07-26) | Full-bundle identity binding, safe registration, key-change hold, capture-safe recovery. Additionally: `verify_against_record`, `VerifyOutcome::DuressByThreshold`, `InactivityTimer` and `crates/keystore/src/duress.rs` have **no production caller** — only the `crates/keystore/src/lib.rs:58-59` re-export and keystore tests. No shipping path locks the app, counts failed attempts, or runs duress. |
| Live v3 crypto | `open-security-finding` | Authenticated attribution and stated guarantees reconciled. **Guarantees reconciled 2026-07-26** in `docs/THREAT_MODEL.md`: the live scheme is stateless hybrid `v=3` (`crates/ipc/src/wire_v2.rs:685`) — PQ confidentiality and sender-side FS only. No forward secrecy against recipient compromise, no post-compromise security, no group rotation window. Attribution finding still open. |
| Ratchet-next | `implemented-unwired` — **no production call path** (verified 2026-07-26) | Review, feature gate, persistence, downgrade pin, two test identities. The adapter `crates/ipc/src/wire_rn.rs` genuinely uses the crate, but nothing uses `wire_rn`: its only non-test reference is the `crates/ipc/src/lib.rs:76` module declaration. "Integration in flight" was optimistic — correct it before treating gate 1 as consumed. Root `Cargo.toml:25-28`'s "no other crate depends on it" is now false at the Cargo level (`crates/ipc/Cargo.toml:73`). |
| Discord protected send | `verified-live` on dated QA builds; current tree recheck required | Exact release build, all three send modes, tri-state production proof |
| Discord eye | `runtime-proven` — geometry placement measured `0 → 3` (2026-07-26) | Evidence: `osl-rehydrate-geometry-diagnosis.md`, `rel_l −8 → 0`, `placedRowCount 0 → 3`, `unplaceableRowCount 3 → 0`, executable SHA-256 `6b6a36945b42…`. Still required: final visual screenshot, inbound/peer row, history, scroll, DPI, click-through. Measured on the QA shell, not the shipping build; no test binary executed. Supersedes the `BLOCKED` row in `osl-completion-plan-2026-07-26.md` — see its Conflict C1. |
| Discord composer/window hosting | `runtime-proven` with repeated regressions | First-launch adoption, drag/resize/minimize, focus, no flash/corners, current exact build |
| Attachments/flag media | `implemented-unwired` with open findings | No plaintext staging, real row handoff, measured media rect, send/receive/view-once |
| View-once, timed deletion, bilateral burn | `implemented-unwired`; bilateral burn additionally has an **open defect** (verified 2026-07-26) | Two identities, offline/restart, second-open refusal, honest peer/platform receipts. Burn defect: the text/receipt drain recognises an inbound revocation frame and **DELETEs it from the control inbox without applying it** (`apps/osl-hub/src/broker.rs:2635-2640`), so the notice cannot be replayed once the apply path is wired. Tab 5 owns the fix; description corrected in `docs/qa/two-identity-p2p-verification.md` §6 item 4. |
| Keyserver | **RESOLVED 2026-07-26 → `verified-live`.** The escalation is closed: `0027` **is deployed**, and there is now the named Worker version the conflict was blocked on — keyserver `3f92f0f5` (control-inbox per-sender recycling, `429 recipient_inbox_full`, pubkeys minimisation), plus cipher-store `0a17547d` and migration `0029` applied. The `NOT DEPLOYED` header inside `keyserver-cf/migrations/0027_control_inbox_revocation_lane.sql:4` is **stale and must be ignored**; correcting that file belongs to the keyserver lane, not this one. `0028` link-grant is applied but **dark** behind a default-off `LINK_GRANT_ENABLED`. Source: `docs/reports/coordinator-state-2026-07-26.md`. | Three sources conflict on migration 0027: the migration header says `NOT DEPLOYED` (`keyserver-cf/migrations/0027_control_inbox_revocation_lane.sql:4`), the completion plan said applied and smoke-tested, and `keyserver-cf/DEPLOY.md:619` records that approval for the 0026/0027 deploy was given. None names a Worker version or deploy timestamp, and remote migration state is external to this checkout (corroborated by `docs/security/osl-audit-2026-07-26-codex.md:599-602`). **Escalated as a bounded owner question** — see Conflict C2 in `osl-completion-plan-2026-07-26.md` for the single read-only command. Blocks Tab 5. |
| Scrub discovery/import | active dirty integration work; broad browser evidence exists | Exact-build walkthrough, all selected browsers/profiles, revocation/back teardown |
| Free Scrub deletion | active implementation | Per-account/category scan, review, confirm, execute, verify, receipt |
| AutoScrub | active implementation; optional-module packaging unresolved | Native Pro gate, transport-scoped presence, global stop/status, module boundary |
| Native-app Scrub | `designed-only`/partial hosting foundations | Verified service recipes, ownership evidence, no focus theft, safe actions |
| Phone Scrub hook | scaffold/in progress | Deployed website UI, discriminators, coverage receipt, false-positive calibration |
| Signal | QA foundations only | Complete adapter contract and two-peer exact-build proof |
| WhatsApp | substantial separate QA worktree | Complete adapter contract and two-peer exact-build proof |
| Telegram | `externally-blocked` if stable rows remain inaccessible | Signed viability recheck; do not fake support |
| Outlook/OSL Mail | separate staged program | Mail-specific identity, content, attachment, and lifecycle qualification |
| Notes/Creative | extensive specialized worktrees, not integrated | Minimum milestone in 7.9, isolated merge and release proof |
| Website | claim truth `test-proven-only` on branch `web-pricing-truth-2026-07-26` (2026-07-26); deployment still divergent | Section 8 gate. **Owner reframe 2026-07-26: the site is pre-launch marketing for v1, not a status dashboard.** Marketing pages show the full v1 product without a `Planned` stamp on every card; honesty is concentrated in two enforced surfaces — the generated dated support matrix `/docs/status` (§8.4: per capability and per connector, with verification dates and provider-policy risk) and the point of sale, where Pro is now sold explicitly as **early access** and the purchase summary may only list capabilities flagged `sellable`. A global early-access banner carries the frame. Pricing is single-source: 18 markers, `pricing-sync --check` 0 drift, `check-claims` 0 conflicting claims across 16 pages. `check-claims` now enforces the surface distinction (23 known-bad/known-good fixtures) and `build-status --check` proves the matrix cannot drift from the manifest. Responsive/no-JS proof: 270-capture matrix, 0 failed, content visible before scroll with JS off. `per-message-sealing` corrected **back to `Beta`** — `implemented-unwired` describes a call path, not whether the cryptography works. Still required: promotion to production (owner-gated, nothing deployed), 200% zoom and the accessibility half of §8.6, and a CI gate so a deploy cannot skip the gates. |
| Release/CI | current public main historically red on Rust/TS and no protected integration line | Green exact CI, signed candidate, VM promotion, reproducible release |
| Telegram `/osl` mode | `test-proven-only`; new code loaded, currently disabled | Owner `/osl on`, live suppression/dashboard/reply-routing proof, then registry projection and remaining UX |

**Public claim eligibility is no longer derived from this table by hand.** Every website and
in-app claim must come from `docs/design/osl-public-claim-allowlist.md`, which maps each
permitted wording to the status label that earns it, the `file:line` evidence, and the
limitation that must appear alongside. It also lists phrases that may never appear —
"better than Signal", "post-quantum authentication", "cryptographic burn",
"disappears forever", "works on Gmail/Discord", "provider-tested". This closes master §24
item 5: a feature cannot become `Available` through a copy edit.

## 10 · Open security release blockers

The dated source audit found five critical, two high, and six medium findings. Recent dirty changes
may address some, but a finding closes only after a current re-audit and required runtime proof.

Critical classes:

1. Key substitution without changing the trusted identity.
2. Authenticated sender key attributed to a different caller-supplied identity.
3. Unproven public account/snowflake registration.
4. Protected non-image attachments staged as durable plaintext.
5. Recovery secrets rendered without proven capture protection.

High classes:

6. Public scope metadata deriving a scope-wide delete capability.
7. Empty multipart reservations exhausting attachment capacity.

Medium classes:

8. Non-zeroized secret buffers.
9. Plaintext social/message identifiers and metadata at rest/in tracing.
10. Cross-sender control-inbox eviction.
11. Server-visible adoption/social graph identifiers.
12. Non-atomic rate limiting.
13. Burn reporting completion after silently omitting an approved peer.

The later independent crypto review also found that the live native path did not use the claimed
Double Ratchet/sender keys and that v3 sender authentication/attribution was insufficient. Treat
that as current until the reviewed exact live call path proves otherwise.

## 11 · Required agent operating workflow

### 11.1 Start/resume protocol

Every OSL agent begins by:

1. **First encounter in this AI/account/environment:** read this file completely. **Returning
   encounter:** read the header/revision, saved memory card, section 0.5 changes since the saved
   revision, active deadline rows, and only task-linked sections. Do not pay to reread an unchanged
   giant document.
2. On first encounter, write the compact memory card defined in 11.1.1. Later, update only fields
   whose durable truth changed.
3. Read the linked subsystem document and its `Resume here` block.
4. Read the current task report, relevant security finding, and exact current diff.
5. Run a safe preflight: identify worktree/branch/HEAD, changed files, active owners, available RAM,
   required build target, test identities, and whether a virtual display is present.
6. State the task's acceptance criteria, dependencies, exclusive file ownership, and irreversible
   actions before editing.
7. Verify the baseline can fail for the known defect. A harness that reports green against the
   known-broken build is itself broken.
8. Make the smallest coherent change, inspect the final diff, run proportional gates, and write the
   compact report defined below.

Do not restart completed work because a tab compacted. Read the durable report and continue.

#### 11.1.1 Exact durable memory card

Every Claude/Codex/model account that touches OSL saves a **small structured card**, not a copy of
this specification. Keep it under roughly 40 lines and include:

1. `project`: OSL and authoritative repository/worktree paths; explicitly distinguish
   `/home/liamw/discord-privacy-client` from `/home/liamw/osl-newest-integration`.
2. `authority`: owner current instruction → later owner decision → master → subsystem → source/live
   evidence for status; source cannot silently rewrite product intent.
3. `master`: exact path, saved control revision, date read, and full-read-only-on-first-encounter
   rule. If revision changes, use digest/diff before considering a full reread.
4. `owner`: Zhao is product owner; Jester is head developer; explain mainly in simple/layman
   language, lead with outcome, and batch only genuinely owner-blocking questions.
5. `safety`: no passwords/tokens/private-content exposure; no typing without a proven protected
   target; destructive/deploy/security actions stay gated; uncontrolled traffic stays off
   unreviewed crypto.
6. `workflow`: exclusive file/interface ownership, dependency-aware parallelism, compact reports,
   exact-build evidence statuses, off-screen/VM testing, and no restarting completed work.
7. `models`: cheapest adequate model; current OSL ceiling is **high effort only—no xhigh/max**
   unless Zhao explicitly changes it; mechanical work goes to fast/GPT-5.5.
8. `docs`: semantic changes update master, layman spec, internal checklist, affected subsystem
   docs/tests/public copy, and `/osl` projection.
9. `design`: `docs/design/osl-subjective-design-feel.md` governs subjective UI/brand feel.
10. `deadlines`: every unresolved deadline with absolute date/time zone, hard/target class,
    deliverable, acceptance, source/owner, dependencies, last review, and status.
11. `active handoff`: only current subsystem report path, exact evidence/build ID, next dependency,
    and exclusive owner. Replace it when the task ends; do not accumulate logs.
12. `traps`: only recurring/high-cost traps whose expected rediscovery cost exceeds the memory
    cost, each linked to its durable report/test.

Never store secrets, credentials, raw private messages, full chat dumps, giant specs, complete
status tables, speculation, transient percentages, or long terminal output in durable memory.

#### 11.1.2 What future work must record

Update the card immediately when any of these changes:

- master path/revision or repository/worktree identity;
- durable owner preference, safety boundary, model ceiling, or communication rule;
- new, changed, missed, resolved, or superseded deadline;
- durable dependency/ownership rule relevant beyond the current task;
- proven recurring trap with positive expected token/time savings;
- active subsystem handoff after completion or transfer;
- a new model's measured best-use/cost profile after the protocol in 13.4.

Do not update durable memory for routine test counts, daily percentages, temporary agents, one-off
failures, or status already captured in a task report. Use this test:

> Will a future agent probably spend more time/tokens rediscovering this than it costs to store and
> validate this short fact?

If no, do not save it. If yes, save the conclusion plus a link—not the investigation.

### 11.2 Relevant local skills

Use skills when their trigger matches:

- `shared-ai-memory`: retrieve prior decisions/debugging before rediscovering them; current source
  and owner instruction still win.
- `project-wiki-memory`: query the existing `graphify-out/graph.json` for architecture/dependency
  questions; update the graph only when authorized and source materially changed.
- `personal-operator`: local preflight, model/profile selection, non-secret usage/status, bounded
  delegation, and durable checkpoints.
- `openai-docs`: current official OpenAI/Codex behavior only.
- `cloudflare`, `workers-best-practices`, `wrangler`, `durable-objects`, or `agents-sdk`: use for
  the corresponding Worker/D1/R2/Cloudflare change and retrieve current official docs first.
- `web-perf` and Chrome DevTools: website performance/runtime inspection.
- design/UI skills: only when visual/system design is actually in scope.

If a named skill is missing, say so and use the safest available equivalent. Never install a
random plugin/skill from an unreviewed repository just to avoid reading the source.

### 11.3 Tool installation policy

Agents may install a missing user-scoped, reversible, well-known development/test dependency when
it is necessary and normal for the authorized task. Record package, version, source, purpose, and
uninstall command.

Explicit owner approval is required before:

- Windows drivers, kernel/system patches, RDP Wrapper, security-policy changes, or admin installs;
- a paid service/purchase;
- a new externally reachable service or production deployment;
- browser/Discord token, cookie, credential, or profile copying;
- a tool that sends source, conversation context, or credentials to a third party;
- destructive cleanup of worktrees, stashes, caches containing unique work, or user data.

Prefer already-installed tools. Never silently fall back from a missing virtual display to the
owner's main monitor.

## 12 · Autonomous and off-screen testing

### 12.1 Current safe environment

This PC currently has:

- `Virtual Display Driver` by MikeTheTech, version 11.30.4.434, device status `OK`; it is reported
  unsigned, so do not reinstall/update it without explicit approval;
- primary `\\.\DISPLAY1` at `0,0,1920×1080`;
- non-primary `\\.\DISPLAY5` at `1920,0,1920×1080`;
- a QA-only headless self-test request/verdict path;
- an acceptance harness and 40 ms state/screenshot recorder in Windows `%TEMP%`;
- Windows/VM QA scripts under `scripts/qa/`;
- multiple isolated OSL app identifiers/worktrees and two test identities/VMs in progress;
- FFmpeg and OBS Studio for narrowly scoped display recording;
- Hyper-V services/module installed, though the current user may lack VM-management permission.

The `%TEMP%` scripts are useful current evidence but are not durable product source:

- `%TEMP%\launchvd.ps1`: starts a specific executable, identifies its new PID/window, refuses if no
  secondary display exists, and parks it on the non-primary display.
- `%TEMP%\accept.ps1`: objective acceptance harness with `pass`, `fail`, and `unmeasurable`; stale
  artifact rejection and pixel/window measurements.
- `%TEMP%\recorder.ps1`: passive 40 ms window/focus timeline plus screenshots and breadcrumb tail.
- `%TEMP%\osl-qa-selftest.request` / `osl-qa-selftest.json`: QA-only scenario rendezvous.

An agent must inspect the current script before relying on it; `%TEMP%` can be replaced or stale.
Durable versions should eventually live under `scripts/qa/` with tests and documentation.

### 12.2 Default no-bother test flow

1. Use a QA identifier and test identity, never the owner's production identity unless the owner
   explicitly selected it.
2. Build the exact intended frontend and Windows binary. `apps/osl-hub` is not a root workspace
   member; use the explicit manifest. Rebuild `osl-hub-ui/dist` because Tauri can serve a stale
   prebuilt frontend.
3. Stamp binary hash, bundle identifier, features, frontend asset hash, branch, and dirty-state
   fingerprint in the run record.
4. Launch directly onto the discovered non-primary display. Refuse rather than use the main screen.
   DISPLAY5 is visual separation, not a second Windows foreground/focus domain.
5. Prefer the QA self-test/headless command. It must use the real production command path, not a
   parallel fake implementation.
6. Use passive screen/window/accessibility observation. Do not move the owner's cursor or focus.
7. For move/drag testing, use focus-safe window messages/positioning and measure absolute UI-thread
   response. A relative-position test can falsely pass when all windows freeze together.
8. Capture before/after/failure screenshots and machine JSON. Validate ownership/z-order before
   trusting pixels; a screen capture contains whichever window is actually on top.
9. Reject artifacts older than the current run start. A stale green/red receipt is
   `unmeasurable`, never evidence.
10. Restore or close only QA-owned surfaces. Preserve owner applications and drafts.

If a test must activate a window, enter a system move loop, or use `SendInput`, run it in the
isolated VM path. A virtual monitor alone cannot prevent Windows from changing the single
interactive desktop's foreground window.

### 12.3 Focus/input safety

- Prefer direct QA commands, accessibility invoke, `PostMessage`, or window messages.
- Never use global `SendInput` or real mouse driving on the primary display when a focus-safe route
  can test the same invariant.
- Before any Discord composer input, positively measure the cyan ring and exact target/bundle.
- Never type a test carrier into a real conversation unless it uses a declared harmless test
  identity/conversation and the task authorizes sending.
- If a send may have occurred but proof is missing, stop and mark `delivery_uncertain`; do not retry.
- Preserve and report any existing native draft. Never classify byte-exact OSL carrier text as the
  owner's draft merely because the composer geometry grew.
- Manual owner testing uses the passive recorder: the owner reports the symptom; the agent
  cross-references timestamps, diagnostics, and screenshots rather than asking the owner to repeat
  the test.

### 12.4 What can be installed or used for stronger isolation

- Current preferred level: installed virtual display plus isolated QA app identifier.
- Next level: separate Windows user/Hyper-V VM with test OSL and test host-app accounts.
- Do not install RDP Wrapper or patch `termsrv.dll`; it is a system/security risk.
- RDP-to-self on Windows 10 Pro does not give an independent simultaneous console.
- A VM is required for truly separate focus, cursor, desktop, and host-app sessions.

Only bother the owner when an action needs a human login/2FA, informed consent, irreversible or
production authority, a subjective product judgment not represented by a measurable requirement,
or a physical/OS boundary the harness cannot exercise.

Credential-bearing VM tests follow `docs/testing/test-account-secrets.md`: disposable accounts,
just-in-time secret retrieval from the approved vault, redacted logs/screenshots, and no passwords
in prompts, environment dumps, repositories, or Telegram.

### 12.5 Evidence-grade acceptance design

Every behavioral criterion needs:

- exact preconditions and build identity;
- one positive path and one negative control;
- a measurement that can fail during the actual defect;
- `pass`, `fail`, and `unmeasurable`, never “not observed = pass”;
- threshold and rationale;
- freshness and target-identity proof;
- artifacts: command, log/JSON, screenshot where visual, and two-identity receipt where bilateral;
- cleanup and retry rules;
- regression test after the fix.

Examples:

- Drag: absolute message-response latency, timeouts, tracking error, presence transitions; not only
  relative composer position.
- Eye: known encrypted plaintext differs from visible carrier, correct native row, wrong-row count
  zero, scroll/resize/DPI changes, peer-authored row.
- Send: exact carrier readback, correct conversation binding, authority mode, native row appeared,
  tri-state result, no duplicate on uncertain proof.
- Corners/ring: exact pixel/hue/ownership predicate on composited screen, not brightness or
  `PrintWindow` on Chromium.
- Scrub: seeded owned target, preview, execution, post-state recheck, refusal/challenge path, receipt.
- Website: deterministic viewport/motion matrix, JavaScript-off content, claim/price crawl.

## 13 · AI/model/account routing and usage

### 13.1 Route by consequence, not prestige

Use the least expensive model that preserves quality:

| Work | Preferred route | Required behavior |
|---|---|---|
| File inventory, search, formatting, repetitive fixtures, mechanical edits | Codex `fast` / GPT-5.5 or Claude Haiku-class subagent | Narrow prompt, exact files, no product judgment |
| Routine implementation with a settled contract | Codex `build` or Claude Sonnet | Focused tests, final diff inspection |
| Architecture, security, crypto, payments, destructive actions, hard diagnosis | Codex `sol` or Claude Fable/Opus at **high** | Source-first reasoning, independent review; no xhigh/max under the current owner preference |
| Independent plan variants | Two strong agents, blind to each other's plan | Same inputs and acceptance contract |
| Final adjudication | Primary strong agent | Compare evidence/trade-offs; owner spec wins |
| Mechanical work inside a strong-agent task | Delegate to `codex -p fast` or smaller agent | Strong agent retains judgment and reviews output |

Do not send five strong agents at an unmeasured bug. First build the discriminating measurement,
then parallelize the independent fixes.

### 13.2 Current Codex accounts

This machine currently supports:

- `codex`: primary Codex home/account.
- `codex2`: isolated account home `~/.codex-b`, shared sessions/config/state indexes.
- `codex3`: isolated account home `~/.codex-c`, shared sessions/config/state indexes.
- `codex-switch`: serially switches the primary `~/.codex/auth.json` between named `main` and `osl`
  slots and restarts the cached app server.

Safe use:

```bash
codex-usage
codex-switch status
codex2
codex3
codex-switch main
codex-switch osl
```

- Use `codex2`/`codex3` for simultaneous independent tabs.
- Use `codex-switch` only when intentionally changing the account for subsequently launched
  primary tabs. It kills the cached app server; existing primary tabs may disconnect.
- Never resume the same session/thread in two accounts/tabs simultaneously.
- Account auth files remain mode `0600`, outside the repository, logs, prompts, and reports.
- To add another concurrent Codex account, clone the **structure** of `~/.codex-b`, share only the
  documented non-auth state/sessions, keep its `auth.json` and daemon dirs isolated, then use device
  auth. Do not copy tokens into chat or source.

`codex-usage` reads the latest provider rate-limit snapshot and reports the 5-hour and weekly
percent used. The custom statusline also stamps cx1/cx2/cx3 usage separately. These are better than
guessing from token counts.

### 13.3 Claude accounts and models

Current Claude CLI supports model aliases such as `sonnet`, `opus`, and `fable`, and effort levels
`low`, `medium`, `high`, `xhigh`, and `max`.

Those are capabilities, not permission to use all of them. **Current OSL rule: high is the maximum
effort. Do not launch xhigh/max work unless Zhao gives a newer explicit instruction.**

- In an interactive Claude tab, use `/usage` for provider-reported percentage/reset information.
- `claude auth status` verifies authentication only; it is not usage accounting.
- The local `personal-operator` status script reports subscription/auth presence without revealing
  tokens, but its numbers are local telemetry rather than provider billing records.
- If the UI/statusline cannot show a percentage, use the provider's account usage page. Never invent
  a percentage from elapsed time.

Multiple Claude accounts are a requested infrastructure task, not yet verified in this document.
The implementing agent must first verify the officially supported isolated configuration-home
mechanism for the installed Claude version, then create `claude2`/`claude3`-style launchers with
separate credentials, caches, and daemon/session writers. Use normal browser/device login. Never
copy OAuth tokens by hand, share one mutable credential file across simultaneous accounts, or put
credentials in the mirror bot. Document switching, status, logout, recovery, and update behavior,
then run a two-account concurrency test.

### 13.4 Dormant “AI new model” protocol

Trigger this when a materially new frontier/general-purpose coding model becomes genuinely
available to Zhao, not for every minor snapshot. Verify availability, pricing, limits, context/tool
support, and identity from official sources before spending project time.

**Safe pause:** pause new dispatches, merges, deployments, and architecture decisions long enough
to snapshot the project. Do not kill agents mid-edit, abandon active builds, or corrupt file
ownership. Let bounded in-flight work reach a safe checkpoint, then freeze it.

Start one isolated tab using the new model at **high effort**. Give it this master, the simple spec,
internal checklist, subjective-design guide, current DAG/deadlines, current security audit/threat
model, compact subsystem `Resume here` reports, source maps, exact diffs, evidence index, and public
website claims. Its first pass is read-only.

It performs five passes:

1. **Truth audit:** intent versus source/live evidence, public claims, security boundaries,
   duplicated truths, stale status, and missing dependencies.
2. **Workflow/document redesign:** find token waste, hidden conflicts, and weak guidance. It may
   directly improve structure, indexes, memory cards, prompts, tests, and coordination documents
   while preserving owner decisions and recording every semantic delta.
3. **Architecture/quality review:** identify simpler designs, integration risks, missing tests, and
   high-return refactors. Product behavior changes remain proposals until Zhao accepts them.
4. **Creative product review:** suggest meaningful features, removals, or simplifications with user
   benefit, security/privacy cost, dependencies, estimated scope, and why now/later. Send each
   serious idea as an `/osl IDEA` alert; never silently add it to committed scope.
5. **Model-efficiency benchmark:** determine where the model is actually better value than current
   models rather than assuming newer means better everywhere.

Benchmark every available model/profile on the same compact OSL suite:

- one mechanical search/edit task;
- one settled implementation/review task;
- one hard diagnosis with known/hidden evidence;
- one architecture/security judgment;
- one concise owner explanation.

Record per model/effort: acceptance score, factual defects, security misses, rework, wall time,
input/output tokens, usage-window or monetary cost, tool reliability, context retention, and human
review time. Compute **quality-adjusted cost** from accepted useful work divided by total spend,
rework, and waiting cost. Route each task class to the cheapest profile whose lower-confidence
quality meets its threshold. Re-test after major model/tool changes and add an expiration date;
never route by prestige, provider marketing, or a model's self-assessment alone.

Deliver:

- before/after document map and exact edits;
- accepted workflow improvements separated from product proposals awaiting Zhao;
- evidence-backed model routing table with retest date;
- risks/rollback for structural document changes;
- refreshed memory card/revision, derived checklists, Telegram projection, and tab prompts;
- short layman owner summary.

Resume normal work only after the coordinator confirms owner authority, deadlines, safety gates,
file owners, and unfinished tasks survived. The new model may improve organization; it may not
erase history, silently rewrite Zhao's intent, or approve its own product ideas.

### 13.5 Health and usage preflight

Useful local commands:

```bash
python3 ~/.codex/plugins/cache/personal/personal-operator-toolkit/*/skills/personal-operator/scripts/preflight.py
python3 ~/.codex/plugins/cache/personal/personal-operator-toolkit/*/skills/personal-operator/scripts/status.py
codex-usage
codex doctor
claude doctor
headroom doctor
```

Resolve the wildcard to the installed version if the shell does not. Do not publish their output if
it contains paths or account identifiers that are irrelevant to the task.

Before a large parallel Rust wave, check memory/swap and duplicate `rust-analyzer` processes. Past
duplicate indexes consumed roughly 12 GB and forced heavy swapping/WSL crashes. Throttle builds and
agent count before the machine becomes unstable.

### 13.6 New-window bootstrap, downgrade detection, and freeze recovery

Open a new window only when its first useful task is dependency-ready, has non-overlapping
files/interfaces, and saves more wall time than its coordination/build pressure costs.

Bootstrap:

1. Check account/model usage and choose an isolated launcher (`codex2`/`codex3` or a verified
   Claude-account launcher). Do not use a serial account switch that disconnects active tabs.
2. Give the new window the universal cold-start prompt from section 25 plus one bounded task capsule:
   goal, non-goals, repository/worktree, spec IDs, dependencies, exact file/interface ownership,
   acceptance, forbidden actions, report path, and upstream/downstream tabs.
3. Require it to record provider-reported model identity, effort, account alias, session/tab ID,
   start time, master revision, branch/HEAD/dirty fingerprint, and exclusive ownership before work.
4. Have it acknowledge the first discriminating measurement and dependency state. If it cannot,
   the window is not ready and must not start broad edits.
5. Register tab/session → task → files → dependency → Telegram-message mapping so replies and
   handoffs cannot reach the wrong tab.

There is no honest way to guarantee a provider never changes routing. Do not disguise credential,
security, or policy-sensitive work to bypass a provider's safeguards. Instead:

- Treat cybersecurity and credential-store analysis as a known routing-risk area: prior Claude work
  unexpectedly moved from Opus-class reasoning to 4.8 while reading login/credential-adjacent
  code. Preserve the hard invariant (for example, never read/decrypt `password_value`) and split
  raw credential access away from reasoning; do not weaken or conceal the task to prevent routing.
- keep raw credentials/secrets out of every model context;
- phrase credential-adjacent work around schemas, invariants, fixtures, and redacted source;
- when the platform explicitly reports a model/effort change, stop at the next safe checkpoint,
  preserve the diff/report/evidence, alert `/osl`, and reassign judgment to a verified strong model;
- have a second strong model reconcile work completed under an unexpected downgrade before trust;
- do not infer a downgrade solely because an answer feels weaker. Use provider/session metadata,
  UI/status output, or a known capability probe; otherwise label it `suspected`.

Freeze/watchdog behavior:

- A working tab writes a small heartbeat only at meaningful boundaries: measurement captured,
  edit checkpoint, focused tests, heavy build started/finished, runtime proof, or blocker.
- Long builds are monitored by process state and fresh output/artifact timestamps, not repeated
  “are you alive?” prompts.
- If no heartbeat/artifact/tool progress occurs beyond the task's expected bound, the coordinator
  first inspects agent/process/tool state. It never launches a duplicate editor blindly.
- On a confirmed freeze, interrupt only after preserving recoverable state, then resume the same
  task from its report/diff in a new verified window. Check for half-applied edits, stale verdicts,
  locks, orphan processes, and unowned files before continuing.
- Telegram alerts for a model change, suspected freeze threatening a deadline, lost work, or unsafe
  half-edit state. Routine healthy waiting remains silent.

## 14 · Efficient architecture, planning, and debugging

### 14.1 Research depth

Use progressive depth:

1. **Map (5–15 minutes):** entry point, live call path, state owner, interface, tests, feature flags,
   current diff, known evidence. Search before reading entire files.
2. **Discriminate:** list the smallest set of hypotheses and one observation that separates them.
3. **Research externally only where needed:** current third-party APIs, platform behavior,
   standards, security claims, licensing, or unfamiliar architecture. Prefer primary official
   documentation/research papers and record date/version.
4. **Deep design:** only after the live path and uncertainty are clear. For security/architecture,
   create two independent strong-model plans and adjudicate.
5. **Stop researching** when the next uncertainty is empirical. Build the measurement instead.

### 14.2 Fast fault-isolation loop

1. Reproduce once on an identified build.
2. Convert the symptom into a falsifiable invariant.
3. Add content-free diagnostics that name stages/classes/lengths, not plaintext or identifiers.
4. Run one test that distinguishes the leading hypotheses.
5. Fix the shared root primitive, not every downstream symptom.
6. Add a regression test that would fail on the old defect.
7. Run focused tests, correct platform compile, exact build, unattended runtime harness, then visual
   or two-party evidence as appropriate.
8. Remove or gate dangerous diagnostic/re-drive behavior before release.
9. Update status and report. If it still fails, preserve the new evidence and repeat; do not start
   over or relabel the same failure.

Known recurring traps:

- source-shape/string tests do not prove Win32 runtime behavior;
- Linux `cargo check` may compile only non-Windows stubs;
- QA and production feature gates often diverge;
- stale executables/assets can make a correct-looking test run old code;
- file receipts can survive restarts and impersonate the current run;
- MSAA/UIA can expose one rich-text leaf instead of the whole document;
- foreground/focus calls can fail silently;
- screen capture measures the topmost composited surface, not the requested window;
- two windows freezing together can make relative tracking error look perfect;
- killed agents can leave half-applied syntax and stale verdict files.

### 14.3 Plan quality contract

An execution plan must include:

- goal and non-goals;
- owner decision/spec IDs;
- baseline/current status and evidence;
- dependency DAG and critical path;
- tasks small enough to finish/report independently;
- exclusive file/interface ownership;
- interfaces/contracts before implementation;
- per-task positive/negative acceptance tests;
- exact build/runtime environments;
- security/privacy/failure invariants;
- integration order, rollback, and migration;
- public-copy/document impact;
- decision points requiring owner input;
- “Resume here” with next command and expected result.

A second planner should attack assumptions, missed dependencies, impossible requirements, and tests
that cannot fail. The coordinator merges the best plan; it does not average incompatible designs.

## 15 · Dependency-aware parallelization

### 15.1 Build the DAG before opening tabs

Each task has:

- `needs`: contracts/evidence/tasks that must finish first;
- `produces`: interface, implementation, evidence, or decision;
- `owns`: exclusive files/interfaces;
- `accepts`: objective completion gates;
- `unblocks`: downstream tasks.

Only run tasks concurrently when their `owns` sets are disjoint and neither consumes an unfinished
output from the other. If two tasks need the same central file, one agent owns and integrates them
serially; other agents prepare read-only patches/reports against stable interfaces.

### 15.2 Current broad dependency graph

```text
authoritative spec + status baseline
├── security P0 contract fixes
│   └── ratchet persistence/wire/capability gate
│       └── controlled two-identity crypto proof
│           └── uncontrolled-traffic eligibility after external review
├── common adapter contract + evidence harness
│   ├── finish Discord reference adapter
│   ├── Signal adapter
│   ├── WhatsApp adapter
│   ├── Telegram viability recheck or externally-blocked decision
│   └── Outlook/OSL Mail-specific program
├── lifecycle contract
│   ├── attachments + structure-compatible carriers
│   ├── view-once
│   ├── timed deletion
│   └── bilateral burn + receipts
├── Scrub account model + consent/ownership contracts
│   ├── browser/profile discovery
│   ├── Free attended deletion
│   ├── native-app/hosted-session ports
│   ├── optional Pro AutoScrub module
│   └── website username-only demo
├── website canonical pricing/claim source
│   ├── visual fixes
│   ├── Scrub/PWS/Burn explanations
│   └── messenger/email comparison research
└── Notes/Creative minimum milestone

all verified supported-app + security + lifecycle gates
└── preservation/integration wave
    └── signed exact release + website truth reconciliation
        └── major release
```

### 15.3 Work that can usually run concurrently

- Website visual/content work and desktop adapter work, provided website claims remain status-gated.
- Read-only security review and implementation in files the reviewer does not edit.
- Test harness work and product fixes when the harness owns separate scripts/interfaces.
- Browser extraction, hosted/native Scrub port design, and phone username-demo work after the common
  account/consent contract is frozen.
- Signal and WhatsApp reconnaissance/profile mapping; implementation starts only after the common
  adapter contract stabilizes.
- Notes/Creative isolated milestone and messaging work if no shared Hub registry/manifest file is
  edited simultaneously.
- Documentation/status projection can run read-only during implementation and write only after the
  owning implementation reports.

### 15.4 Work that must be serialized

- Multiple changes to `apps/osl-hub/src/main.rs`, `broker.rs`,
  `native_discord_overlay.rs`, `apps/osl-hub-ui/src/main.ts`, shared preferences, Tauri
  capabilities, root manifests, or migration registries.
- Ratchet wire IDs, capability advertisement, persistence, and downgrade pin.
- Lifecycle events sharing one broker dispatch: view-once, expiry, burn, receipts, attachment notices.
- Pricing, terms, privacy, checkout, and entitlement copy after the pricing decision.
- Production deploy/migrations before their exact code and rollback are reviewed.
- Final merge waves across dirty worktrees.

### 15.5 Tab-launch suggestions

The coordinator should continuously look for a ready independent node. When one exists and
additional capacity will shorten the critical path, the Telegram `/osl` update may suggest:

> **Suggested new window — `<scope>`**  
> Why now: `<dependencies satisfied; independent files>`  
> Expected unblock: `<downstream task>`  
> Model/account: `<recommended profile>`  
> Copy-paste prompt: `<bounded prompt>`  
> Do not start if: `<specific active owner/dependency>`

Do not suggest a new tab merely because an account has tokens left. Do not start a downstream tab
whose inputs are still being designed; that creates rework and merge conflicts.

### 15.6 Delegation to the head developer

Default head-developer suggestions should emphasize cohesive website work that does not collide
with crypto/adapter internals:

- canonicalize production/preview branch and make build identity visible;
- implement the responsive username-only Scrub demo from the frozen response contract;
- reconcile pricing/terms/privacy/checkout from one manifest after Zhao chooses pricing;
- implement PWS/Burn explanation and animation with reduced-motion/static fallback;
- execute owner screenshot-specific visual fixes;
- build the support/privacy comparison page shell and citation system;
- build deterministic responsive screenshot tests and JavaScript-off/reduced-motion checks.

Do not delegate unsourced security rankings, crypto guarantees, release-status wording, or pricing
decisions. Provide Jester the approved source facts and require a preview URL, commit, screenshot
matrix, exact changed files, and concise handoff.

## 16 · Telegram terminal mirror `/osl` mode

Target project: `/home/liamw/claude-bridge/mirror_bot.py` and its transparent PTY mirror. The live
bot is a large uncommitted working tree; **never reset it to git HEAD**. Deploy through the existing
supervisor/singleton lock by terminating the live bot and allowing the supervisor to respawn it.
Never start a second bot process, expose its token, loosen config permissions, or restart-loop into
a Telegram flood ban.

### 16.1 Mode behavior

Add a persisted owner-only `/osl` mode:

- disables ordinary per-tab live/semi mirroring and routine “every tab finished” notifications;
- does not delete or corrupt terminal sessions;
- binds one or more tabs/tasks to the OSL project coordinator;
- sends only:
  - a moderate-or-larger verified milestone;
  - a new material blocker/approval/physical test requiring “come back to your PC”;
  - a security/safety incident;
  - a high-value creative product suggestion;
  - a dependency-aware suggestion to open a useful new window;
  - the final all-required-work completion report.
- supports `/osl`, `/osl on`, `/osl off`, `/osl status`, `/osl quiet`,
  `/osl bind <tab>`, and `/osl unbind <tab>` or equivalent discoverable commands.
- every periodic/milestone/blocker message stores a durable bounded mapping from Telegram
  `(chat_id,message_id)` to the exact mirror session/task/tab that produced it.
- replying to one of those updates injects the reply into **that exact tab** using the existing
  safe PTY text-then-separated-Enter route. It never uses whichever tab happens to be active.
- the bot acknowledges the destination (`→ #N <tab/task>`), refuses expired/unknown mappings, and
  offers the closest safe `/osl bind` command rather than guessing.
- routing mappings survive bot restart within a bounded LRU/TTL and are deleted when the tab closes.
- alt-user replies keep the existing confirmation/autoconfirm security policy; `/osl` never bypasses
  owner approval rules.

### 16.2 Checklist source

The bot reads the internal company checklist described in section 20, or a generated machine-readable
projection of it. It must not scrape arbitrary agent narration and guess completion.

Each milestone update contains:

- milestone name and evidence/build identity;
- newly checked items;
- regressions/reopened items;
- entire compact OSL checklist grouped by subsystem;
- blockers and critical path;
- suggested independent windows/head-developer work;
- the progress meter and ETA below.

#### 16.2.1 Four-level customizable update hierarchy

`/osl` maintains four linked checklist scopes:

1. **Overall project** — the complete OSL release scope and final weighted meter.
2. **Major workstream** — Scrub, encryption/two-party, Discord/app adapters, website, Notes,
   security/release, and infrastructure.
3. **Window/lane** — the bounded outcome assigned to one tab/session, between a major workstream and
   its individual tasks.
4. **Small task** — the implementation/test/document steps inside that window.

Each scope has a stable ID, parent ID, owner/tab, dependency list, status, weighted acceptance,
evidence, last update, progress/ETA/confidence, and one dedicated Telegram message. Zhao can choose
which scopes are expanded, collapsed, muted, pinned, or included in periodic summaries without
changing the underlying project truth.

Notification behavior:

- A small-task checklist is created once, preferably silently, then **edited in place** as boxes
  change. Checking a small item does not send a new message or notification.
- Window/lane and major-workstream dashboards are also edited in place on every child completion so
  Zhao can inspect current truth at any time.
- **Default notification policy:** small task = `edit-only`; window/lane = `edit-only`; major
  workstream = `notify-on-complete`; overall = `notify-on-complete`. A moderate-or-larger verified
  milestone, blocker, incident, deadline risk, or creative idea may still alert under its own rule.
- Notification policy is customizable per scope as `edit-only`, `notify-on-complete`,
  `notify-on-change`, or `inherit`. Changing it affects alerts, never progress truth/history.
- A new alert links to or quotes the updated dedicated dashboard instead of repeating every small
  task. Its reply mapping still routes to the correct originating tab/coordinator.
- Completing/reopening a child recalculates every ancestor's checklist, weight, critical path,
  velocity, ETA, and timestamp atomically. No level keeps an independent hand-edited percentage.
- Scope moves/merges preserve history and stable IDs. Completed task messages may be archived from
  the active view but remain queryable; do not delete audit history.
- Edits use change detection and Telegram rate limiting. If an edit fails or the message is gone,
  create one replacement, update the durable mapping, and avoid duplicate-notification floods.

Suggested commands or equivalent buttons:

```text
/osl view overall|scope|window|task <id>
/osl expand|collapse <id>
/osl mute|unmute <id>
/osl pin|unpin <id>
/osl notify <id> edit-only|complete|change|inherit
/osl tasks <window-id>
/osl history <id>
```

The initial default is: overall + major workstreams visible in milestone summaries, active windows
visible on request, small tasks maintained silently by edits.

### 16.3 Progress meter and ETA

Every `/osl` update ends with a timestamped progress block:

```text
OSL progress  [████████░░░░░░░░░░░░] 41%
Verified work: 123 / 300 weighted acceptance points
Velocity: 8.4 verified points / active day (7-day EWMA)
ETA: 21 active days · calendar estimate 3–5 weeks · confidence low
Critical path: ratchet review → two-identity proof → remaining adapters
Blocked/excluded: Telegram accessibility (external), owner pricing decision
Updated: 2026-07-26 14:32 PDT
```

Rules:

- Percentage is earned value: completed, non-superseded, objectively verified acceptance-point
  weight divided by the frozen current total. It is not lines changed, tests counted, agent opinion,
  or a smooth timer.
- New owner scope increases the denominator and is shown as a scope-change event; do not conceal a
  percentage decrease.
- Regressions reopen weight and reduce progress.
- Externally blocked work is shown separately and remains in the release denominator if the owner
  still requires it.
- Velocity uses timestamped deltas in verified weight. Maintain short and long EWMAs and ignore
  intervals with no active work when calculating active-time velocity.
- ETA uses remaining critical-path weight and observed throughput, not total parallel work divided
  by agent count. Show a range and confidence. If insufficient history or a blocker dominates,
  say `ETA unknown` rather than inventing one.
- Update on a verified milestone/checklist change and, during long work, at a bounded interval only
  if progress or the critical path changed. No second-by-second timer edits.
- Telegram Bot API flood control, edit-on-change, persisted backoff, and singleton behavior remain.

### 16.4 Privacy and notification threshold

Never include plaintext messages, carrier text, conversation names, account handles, recovery
material, file contents, tokens, credentials, or raw logs in Telegram. Fixed labels, counts, status,
build IDs, and redacted paths are sufficient.

A “moderate milestone” means a verified subsystem gate, resolved P0/P1 finding, completed task that
unblocks another lane, exact-build runtime success, or material plan/spec decision. A compile, one
unit test, ordinary agent completion, or speculative hypothesis is not a Telegram milestone.

### 16.5 Creative suggestion alerts

Agents may send a separate `IDEA` alert when work reveals a genuinely valuable new feature,
architecture simplification, test capability, or product improvement—even if it is outside the
current task. Do not silently implement the expanded scope.

Send only ideas that appear to have meaningful user value, safety value, differentiation, or large
future cost savings. Combine small cosmetic thoughts into the normal report.

```text
IDEA — <plain-language name>
What prompted it: <current feature/evidence>
User benefit: <one sentence>
How it fits OSL: <spec/product connection>
Estimated scope: small / medium / large
Dependencies/conflicts: <existing work/spec>
Security/privacy/business risks:
Recommendation: now / after <milestone> / backlog / reject
Why it may be worth the scope:
Reply: approve discovery / backlog / reject
```

The bot stores approved/backlogged ideas under stable IDs and links them to the dependency graph.
An idea does not enter the 300-point denominator or ETA until Zhao accepts it as product scope.

### 16.6 Dedicated requests to the owner

An agent that genuinely needs Zhao sends one dedicated `/osl` request through the coordinator/bot:

```text
ACTION NEEDED — <plain-language title>
Why I need you: <one sentence>
What I already tried: <short exhaustive summary>
What happens if we wait: <blocked scope; no drama>
Recommended choice: <plain-language recommendation and why>
Other safe choice: <trade-off>
Exact action: <one bounded click/login/decision/test>
Time needed from you: <honest estimate>
Safety/privacy effect: <plain language>
Reply with: <short answer syntax>
```

Do not send raw logs, a technical essay, or one message per failed attempt. Batch non-urgent owner
needs. Continue every independent safe task before declaring blocked. If an approval is required
only for the final deploy/destructive action, prepare, test, document, and stage everything else
first so the owner's action is the last bounded step.

### 16.7 Potentially bad events

Alert immediately, regardless of milestone threshold, for:

- possible plaintext sent where encryption was expected;
- wrong account, wrong conversation, wrong environment, or production/test identity confusion;
- secret/recovery phrase/token/credential exposure;
- destructive action with uncertain target/result;
- possible data loss, repository/worktree overwrite, lost uncommitted work, or corrupted state;
- security guarantee regression or newly discovered critical/high finding;
- a successful host send reported as failure, or any state that could cause duplicate retry;
- production deploy/migration mismatch, unexpected billing, or external-service incident;
- capture protection failure before a secret frame;
- WSL/system instability threatening active work.

The alert states observed evidence, containment already performed, what was not touched, and the
single safest next action. It must not include the sensitive content itself.

## 17 · Capability acquisition, shared memory, and PC sandbox boundaries

### 17.1 Shared memory

Before asking Zhao to repeat context:

1. Use `shared-ai-memory`/the configured sharedMemory search with a narrow OSL query.
2. Prefer curated memory/wiki results over raw transcripts.
3. Fetch the full item only if the excerpt is insufficient.
4. Treat it as historical evidence; this master, owner instruction, and current source win.
5. Save only the structured fields in 11.1.1 and short durable non-sensitive conclusions that
   prevent rediscovery, with a link to the master/subsystem report. Never save secrets, raw
   credentials, private message content, large document copies, or speculative status.
6. Check the stored master revision before rereading. If unchanged, use the memory card plus the
   active subsystem/report. If changed, read the revision digest and targeted diff; reread the
   whole master only when the delta cannot be recovered safely.
7. Future deadlines always receive an absolute-date memory entry under 11.1.1. Never rely on a raw
   Telegram/chat phrase such as “next weekend.”

If an agent lacks sharedMemory/MCP access, it should:

- inspect its available skills/tools first;
- use the local curated memory files under `.ai-env/memory` or the project memory directory
  read-only;
- ask the coordinator to retrieve the one needed item, not request a whole chat dump;
- record the missing capability in its report and continue from repository evidence.

### 17.2 MCPs, CLIs, and missing features

- Discover tools rather than inventing names or schemas.
- Read the applicable skill/instructions fully before acting.
- Prefer existing local resources and project scripts.
- For current external facts, use official/primary sources and record version/date.
- Install only under the policy in 11.3.
- If a capability is unavailable, search for a safe local equivalent, then produce a bounded setup
  plan. Do not broaden permissions or disable security merely to make a test easier.

### 17.3 Using the PC as a sandbox

The machine is available for in-scope OSL development/testing, but “sandbox” does not mean
permission to risk personal data:

- Use separate app identifiers, `%APPDATA%`/WebView user-data roots, test accounts, virtual display,
  temporary directories, and VMs.
- Resolve exact paths and targets before writes/deletes.
- Never copy or inspect browser/Discord passwords, tokens, cookies, private conversations, or
  unrelated personal files.
- Never reuse broad variables such as `$HOME` as a destructive target.
- Do not alter primary-screen focus/cursor when the off-screen route exists.
- Do not deploy, purchase, message third parties, change OS security, or perform destructive
  cleanup without the required authority.
- Clean temporary test state only after preserving the evidence/report needed to reproduce it.

## 18 · Token/time economy and cross-tab coordination

### 18.1 Token-saving communication

- Lead with outcome/evidence, then the minimum explanation.
- Use stable IDs, paths, line anchors, hashes, and links instead of re-explaining architecture.
- Search first; read targeted ranges and diffs rather than entire files.
- Ask agents for structured deltas, not narrated exploration.
- One report owns full detail. Master/checklists/memory link to it.
- Summarize failed hypotheses only when they prevent future repetition.
- Prompts state goal, non-goals, exact files, inputs, acceptance, forbidden actions, and report shape.
- Do not paste the master into every prompt; point to it and require a complete read.
- At compaction, write a 5–10 line `Resume here` block: exact state, evidence, next command, expected
  result, blocker, file owner.
- Use mechanical models for mechanical output and keep strong-model context for decisions.
- Avoid repeated full builds/tests when a focused test answers the current question.

Good task phrasing:

> Read the master and `<subsystem>`. Own only `<files>`. Establish `<baseline>`; make `<invariant>`
> true; prove with `<positive/negative/runtime gates>`. Do not touch `<excluded scope>`. Report only
> decisions, diff, commands/evidence, remaining unknowns, and next dependency.

Bad task phrasing:

> Look through everything, fix whatever seems wrong, and tell me all your thoughts.

### 18.2 Cross-tab control

Maintain one coordinator. Every tab announces:

- task ID/scope;
- worktree/branch/HEAD;
- exclusive files/interfaces;
- dependencies and produced contract;
- status and estimated next milestone;
- report path.

The coordinator maintains the ownership/DAG table and resolves collisions before edits. Tabs:

- consume cross-tab checkpoints without restarting completed work;
- do not edit another tab's files;
- send interface questions, not unsolicited competing patches;
- checkpoint before model/account/session limits;
- stop and report if the file changed under them;
- never run overlapping heavy builds against the same target directory;
- release ownership explicitly when done or interrupted.

### 18.3 WSL stability

- Run the preflight before a large wave.
- Limit active expensive Rust/Windows builds to one per target directory; use separate target dirs
  only when the disk/RAM cost is justified.
- Default heavy build parallelism to a measured safe value (historically `-j 4`), reduce it if swap
  grows or WSL becomes sluggish.
- Do not launch full frontend, Rust, Windows cross-compile, rust-analyzer indexing, and multiple
  test binaries simultaneously.
- Kill duplicate orphaned `rust-analyzer`/build processes only after identifying exact PIDs and
  owners; never broad-kill active agent work blindly.
- Avoid long unbounded log streams; bound output and poll background jobs.
- Save edits incrementally with compiling checkpoints because agents have been killed mid-edit.
- Verify build/verdict timestamps and binary hashes after any crash.
- If memory pressure rises materially, stop dispatching, let the current build finish, preserve
  reports, and notify via `/osl` only if active work is at risk.

## 19 · Spec-derived, individualized, and “fuck-around” testing

### 19.1 The spec is the test oracle

Before implementing a feature, translate the owner's plain-language behavior into numbered,
observable acceptance statements. Review the translation against sections 6–8 and linked decisions.
The test must prove those statements, not a convenient approximation of the current code.

For every requirement:

```text
SPEC-ID:
Owner-visible promise:
Preconditions:
Action:
Expected visible result:
Expected state/data/network result:
Forbidden result:
Failure copy/recovery:
Evidence source:
Applies with these other features/states:
```

A test that can pass while the owner-visible promise is false is invalid. A test that asserts a
function name, source string, mock receipt, or internal flag without proving the promised boundary
is supportive only, never completion evidence.

### 19.2 Individual test capsules

Build small reusable test capsules to cut iteration cost:

- one feature/invariant;
- seeded deterministic state;
- direct scenario trigger;
- exact build identity;
- fixed content-free diagnostics;
- 10–120 second bounded runtime;
- one JSON verdict plus minimal screenshots;
- failure localization to the first broken stage;
- safe cleanup and rerun;
- negative control proving the capsule can fail.

Run the capsule after every relevant edit; run the full matrix only at the integration boundary.
Promote a repeated manual diagnosis into a capsule after the second occurrence.

### 19.3 Interaction/state matrix

Every new feature lists the existing controls and lifecycle events it can interact with:

- Lock on/off, Eye on/off, whitelist allow/revoke;
- all three send modes and Pro typing simulation;
- native composer focused/empty/nonempty;
- drag, resize, DPI/zoom/theme, scroll, minimize `−`, maximize/restore, close `X`, taskbar close,
  Alt+F4, restart, crash/recovery;
- app/account/conversation switch;
- offline, rate limit, server error, stale response, revoked key, expired/viewed/burned content;
- attachment/text/image/video/file;
- Scrub running/paused/stopped while the user uses another app;
- capture protection allowed/refused;
- Free/Pro/expired entitlement;
- first launch/onboarding complete;
- accessibility/screen-reader/keyboard-only/reduced-motion.

For each pair that can share state, document expected behavior and add a test or a justified
non-interaction. New features are incomplete until the interaction matrix is reviewed.

### 19.4 “Unforeseen fuck-around” tests

After deterministic paths pass, exercise realistic messy sequences:

- rapid/random button mashing;
- repeated Lock/Eye/allow/revoke while dragging/resizing/minimizing;
- Enter mode changes with nonempty drafts and network transitions;
- switch accounts/conversations mid-operation;
- close via `X`, taskbar, Alt+F4, host close, OSL close, and crash at each stage;
- scroll during rehydrate/paint; focus native/OSL composer back and forth;
- pause/resume/stop Scrub while unrelated apps are used;
- duplicate/stale/reordered events and repeated view/burn/expiry requests;
- restart at every persisted state transition;
- resource pressure and delayed responses.

Use model-based/state-machine/property testing to generate sequences from the documented state
graph, then minimize any failure to the shortest reproducer. Invariants—no plaintext downgrade,
wrong target, duplicate destructive action, unprotected secret frame, lost draft, false receipt, or
orphaned window—must hold after every step, not only at the end.

Randomness is seeded and recorded. A chaos test without a reproducible seed, build identity, event
trace, and post-state oracle is entertainment, not evidence.

### 19.5 Integration design rule

Before adding a new state/flag/worker:

1. Identify the single authority that owns it.
2. Search every writer/reader and lifecycle reset.
3. Place it in the dependency and interaction graphs.
4. Define serialization/migration/default and downgrade behavior.
5. Define startup, restart, crash, account switch, and teardown.
6. Reuse an existing contract/ledger rather than create a second truth.
7. Add drift tests where native/renderer/website mirror the same data.
8. Document conflicts so the next agent sees “X conflicts with Y” before editing.
9. Prefer removing a state or branch over coordinating two indistinguishable writers.

The simplest design satisfying the full spec and failure model wins—not the fewest lines in the
happy path.

## 20 · Documentation, reports, derived views, and memory

### 20.1 Document set

- **Authority:** this file.
- **Layman product view:** `docs/design/osl-simple-spec.md`.
- **Internal company checklist:** `docs/design/osl-internal-build-checklist.md`.
- **Subjective UI/brand authority:** `docs/design/osl-subjective-design-feel.md`.
- **Subsystem designs:** the relevant `docs/design/`, `docs/plans/`, `docs/testing/`, and
  `docs/security/` files linked from a task/status.
- **Task reports:** `docs/reports/<task-id>.md` when the detail is durable and material.
- **Decision records:** `docs/decisions/<decision-id>.md` for a choice too detailed for one master
  row.
- **Evidence:** content-safe artifacts under `docs/evidence/<acceptance-id>/` or a durable external
  path named in the report.

Do not create a new “master,” “final,” or “ultimate” spec. Update this one.

### 20.2 Mandatory three-view update

Any accepted change to product intent, feature status, release scope, pricing/tier behavior,
security truth, or critical path must update in the same task:

1. this master specification;
2. the layman spec if the user-facing explanation changed;
3. the internal company checklist if intent/status/dependency/evidence changed;
4. the Telegram machine projection/checklist once `/osl` mode exists;
5. affected subsystem docs/tests/public copy;
6. the compact memory card when a field in 11.1.2 changed—never for routine status.

The master directs AI updates as follows:

- Product behavior change → update the numbered locked decision and layman explanation.
- Implementation/evidence change → update the master status row and internal checklist.
- Dependency/owner/blocker change → update DAG/internal checklist/Telegram projection.
- Security finding change → update blocker section, status, acceptance, and public claim eligibility.
- No semantic change → do not churn the layman spec.
- Durable design-feel/brand change → update the subjective-design guide and affected tokens,
  components, asset registry, screenshots/tests; update this master only if behavior/scope changed.
- Deadline change → update section 2.1, internal checklist, `/osl` ETA projection, task priority,
  and every AI account's compact unresolved-deadline memory entry.

A reconciliation test/task should periodically compare stable feature IDs and statuses across all
three documents and refuse missing/unknown IDs.

### 20.3 Efficient task report template

```markdown
# <TASK-ID> — <title>
Status/build/worktree/owner:
Spec IDs and acceptance:
Dependencies / exclusive files:
Starting evidence:
Decision made:
Changed interfaces/files:
Verification commands and exact results:
Runtime/screenshots/two-party evidence:
Negative controls/failure injection:
Remaining failures/unknowns:
Conflicts/spec or public-copy updates:
Integration/rollback:
Resume here:
```

Write facts and deltas, not a diary. Include a failed hypothesis only when it prevents a future
agent from repeating an expensive path.

### 20.4 Spec/architecture change procedure

Agents are authorized—and required—to modify this specification when Zhao gives a clear product,
architecture, tier, workflow, release, or behavior change. Do it during the same task/turn once the
new intent is understood; do not wait for a separate “update the docs” request. A diagnosis,
implementation detail, agent preference, code comment, or old memory is not authority to rewrite
product intent.

When Zhao changes a spec or architecture:

1. Record the new decision, date, owner wording, and affected spec IDs.
2. Identify every competing prior claim and mark it `superseded`.
3. Build an impact map: UI, native, crypto/wire, persistence/migration, network/deploy, consent,
   pricing/tier, test matrix, docs/site, telemetry, rollback, dependencies, current tasks.
4. Update the contract/default/migration and acceptance statements before implementation.
5. Recalculate DAG, work weights, percentage denominator, critical path, ETA, and merge plan.
6. Update master, layman spec, internal checklist, Telegram projection, subsystem docs, tests, and
   public claims in the same workstream.
7. For architecture/persistence/wire changes, specify transition compatibility, downgrade
   resistance, rollback, old-data handling, and removal date.
8. Notify the owner of the conflict resolution and any percentage/ETA change in plain language.

### 20.5 Communication style with Zhao

Zhao is the product owner but does not want developer-only explanations. Default to:

- plain language first;
- what works/does not and what the user sees;
- simple recommendation and reason;
- technical detail only where it changes the decision or is requested;
- honest uncertainty and limitation;
- one bounded ask after all independent work is exhausted.

Avoid unexplained acronyms, stack traces, raw code, and long internal narration. If technical terms
are necessary, define them in one sentence.

### 20.6 Trap ledger and token-return rule

Record a mistake/trap only when its expected future savings exceed the reading/writing cost.

Score it informally:

```text
expected rediscovery cost
= recurrence probability × time/tokens/data-risk if repeated × likely number of future agents

recording cost
= words future agents must read × how broadly the warning is placed
```

Write the trap when it caused or could cause a wrong diagnosis, security/privacy incident, data
loss, stale-build test, repeated failed cycle, WSL crash, expensive build, cross-tab collision, or
more than roughly one future focused investigation. Skip ordinary syntax errors and obvious
one-time typos.

Put the warning at the narrowest durable location:

- code comment next to a surprising invariant only when changing that code could reintroduce it;
- focused regression test when behavior can be mechanically pinned;
- subsystem `Traps` table for cross-file/runtime behavior;
- master file only for project-wide workflow/safety traps;
- shared memory only for a short pointer to the durable source.

Use this compact format:

```text
TRAP-ID / context:
Symptom:
Actual cause:
Why the obvious approach fails:
Fast discriminating check:
Safe fix/invariant:
Evidence/test:
Last verified build/date:
```

Never paste the whole debugging story. Prefer one regression test plus a 3–8 line warning. Periodic
maintenance removes warnings whose invariant is now structurally impossible, merging duplicates
and marking historical traps `superseded`. A trap ledger that every agent must reread but rarely
saves work is itself a token-cost bug.

## 21 · Safe repository and storage cleanup

Cleanup is a scheduled maintenance task, never a reflex during debugging or a low-disk panic.

### 21.1 When to clean

- after a feature wave is fingerprinted/reported and integrated or safely archived;
- before a major release, after preserving every unique dirty worktree;
- when disk pressure crosses a declared threshold and active builds are not running;
- after stale build/test artifacts are proven reproducible;
- after the owner approves removal of material worktrees/stashes/VMs/accounts.

Do not clean while agents edit/build, after WSL/crash uncertainty, or before comparing dirty
worktrees and stashes.

### 21.2 Read-only cleanup inventory

1. Disk usage by filesystem, repository, worktree, target, node_modules, cache, VM, and temp area.
2. `git worktree list --porcelain`, branch/HEAD/upstream, dirty tracked/untracked count and
   fingerprints.
3. Stashes and untracked unique source/docs/assets.
4. Active processes/locks/agent ownership.
5. Artifact age and whether a report references it.
6. Recovery plan and expected reclaimed bytes.

### 21.3 Cleanup order

Safest first:

1. expired QA screenshots/logs whose material evidence is summarized and linked;
2. reproducible build outputs in explicitly resolved target directories;
3. package caches/duplicate dependency directories that lockfiles can restore;
4. orphaned analyzers/processes after owner/PID verification;
5. stale registered worktrees only after proving their paths/branches contain no unique work;
6. old VMs/test profiles only with explicit owner approval and exported evidence;
7. branches/stashes only after semantic reconciliation and owner approval.

Use recoverable moves/trash where practical. Never use destructive recursive commands against
`$HOME`, `~`, `/`, the workspace root, globs, or unresolved variables. Report what was removed,
bytes reclaimed, and recoverability.

The known dirty-worktree collision risk is severe: many trees contain unique uncommitted changes to
`main.rs`, `main.ts`, `lib.rs`, styles, capabilities, and manifests. No broad cleanup is authorized
until the integration inventory is complete.

## 22 · Integration and merge plan

The owner's current policy is to keep the active large work local and perform a major integration
only when encryption and every offered app satisfy their gates.

Before integration:

1. Freeze/fingerprint every worktree, untracked file list, stash, branch, upstream, and relevant
   deployed version.
2. Choose one authoritative integration line from current GitHub `main`; do not assume local
   `main`, `osl-newest-integration`, or the Discord eye branch is the source of truth.
3. Build a semantic inventory by subsystem, including working-copy-only changes.
4. Integrate shared contracts/security foundations first.
5. Integrate narrow subsystem patches in dependency order.
6. Serialize central-file owners and resolve interfaces rather than line-merging competing copies.
7. Reconcile manifests/capabilities/migrations at each wave boundary.
8. Run security re-audit, stale-claim scan, all feature configurations, Windows exact builds,
   isolated two-identity proof, off-screen UI matrix, website claim/pricing gate, and signed release
   path.
9. Preserve rollback points and do not delete source worktrees until the integrated release is
   independently proven.

Current public repository governance also needs branch protection/rulesets, current PR cleanup,
green Rust/TypeScript CI, a proven signed-candidate/promotion flow, and an actual reproducible
release record.

## 23 · Detailed source index

Read only what the task needs:

- Safety/release sequencing: `docs/design/osl-plan-b-safety-first.md`.
- Honest ship/support cut analysis: `docs/design/osl-plan-a-ship-first.md`.
- Current detailed Discord history: `docs/OSL-DISCORD-STATE-MAP.md` (historical status can be stale).
- Adapter lessons: `docs/design/osl-adapter-playbook.md` and portability/spec files.
- Security findings: `docs/security/osl-audit-2026-07-26-codex.md` plus later re-audits.
- Late lifecycle requirements: `docs/design/osl-requirements-addendum-2026-07-26.md`.
- Discord completion checkpoint: `docs/design/osl-completion-plan-2026-07-26.md`.
- Two-identity testing: `docs/qa/two-identity-p2p-verification.md`.
- VM release gate: `docs/testing/hub-release-candidate-vm-gate.md`.
- Scrub execution: `/home/liamw/osl-newest-integration/docs/plans/scrub-to-spec-plan.md`,
  `scrub-autoscrub-architecture.md`, and `scrub-detection-opus5-plan.md`.

Every detailed subsystem document should end with:

```text
Resume here
Current verified state:
Exact build/worktree:
Current owner and exclusive files:
Next unblocked action:
Command/scenario:
Expected result:
Known blocker/risk:
Master/internal-checklist rows to update on completion:
```

## 24 · Approved workflow infrastructure not yet built

Zhao approved these additions. They are implementation backlog for separately scoped tabs/agents,
not permission for the master-spec coordinator to start building them while maintaining these
documents. The coordinator should place them in the dependency graph and issue bounded prompts as
files/capacity become available.

For each completed infrastructure item, the implementing tab must:

1. prove it with its own known-good/known-bad acceptance test;
2. add the exact command/workflow to the relevant operating section above;
3. change the item below from “not built” to its evidence status and link its task report;
4. update section J of the internal checklist, its earned points, dependencies, progress, and ETA;
5. update the layman spec only if the owner's visible experience changed;
6. update/generate the machine registry and Telegram `/osl` projection;
7. save only a short durable shared-memory pointer if the entry workflow/tool location changed;
8. remove superseded temporary instructions so agents use the built path rather than the proposal.

1. **Machine-readable feature registry.** Before `/osl` progress automation goes live, move stable
   IDs, weights, statuses, dependencies, evidence links, and timestamps into one reviewed
   JSON/YAML registry. Generate the internal checklist and Telegram projection from it. Keep product
   prose in the master; do not ask a bot to parse arbitrary Markdown as state.
2. **Evidence freshness policy.** Store `verified_at`, build hash, environment, and host-app version.
   Mark runtime evidence stale when the relevant app/OS/adapter/build changes. Pure math tests do not
   expire merely because time passed.
3. **Ownership leases.** The coordinator records file/interface owner, heartbeat, expected release,
   and recovery procedure. Expired lease means inspect the diff/report before reassigning—never
   assume the file is clean.
4. **Decision/approval inbox.** Batch owner decisions with recommendation, deadline/impact, and safe
   default. Prevent the same unresolved decision from being asked by multiple tabs.
5. **Public-claim allowlist.** Generate website/app feature labels from exact evidence eligibility.
   A feature cannot become `Available` merely through copy edits.
6. **Incident ledger.** Potential plaintext, wrong-target, data-loss, secret/capture, duplicate-send,
   and deployment incidents receive an ID, containment, root cause, regression proof, and status.
7. **Progress calibration review.** Periodically compare weighted estimates with actual elapsed work,
   split oversized rows, remove gaming incentives, and show confidence/blocked time honestly.
8. **Harness self-tests.** Every acceptance harness has known-good and known-bad fixtures/builds so
   the verification system itself cannot silently become an all-green source-shape test.

## 25 · Universal cold-start prompt

Use this for any new Claude/Codex/model account, then append the bounded task:

```text
You are joining OSL work cold.

FIRST: read this file completely:
/home/liamw/discord-privacy-client/docs/design/osl-master-decision-2026-07-26.md

If this is the first OSL encounter for this AI/account, read the whole master and save the compact
structured memory card required by sections 11.1.1–11.1.2. If returning, compare the saved master
revision with the header and read only the revision delta, active deadlines, and task-linked
subsystem/report. Do not copy the whole spec or volatile status into memory.

The owner's current instruction and current workspace win over history. Do not restart completed
work. Preserve unrelated dirty changes. Before edits, report worktree/branch/HEAD, current diff,
dependencies, acceptance criteria, exclusive files/interfaces, and the first measurement.

Follow the dependency/parallelization rules. Do not touch files owned by another tab. Use the least
expensive model that preserves quality and delegate mechanical work. High is the current maximum
reasoning effort; do not use xhigh/max unless Zhao explicitly changes that. Use off-screen/focus-safe QA,
exact build identity, negative controls, and pass/fail/unmeasurable evidence. Never infer “works”
from source shape or tests alone.

Explain owner-facing issues to Zhao in simple language first. Exhaust independent work before one
bounded ask. Send `/osl` Telegram events only under the milestone, action-needed, incident,
creative-idea, or tab-suggestion rules. Never include sensitive content.

When intent/status/dependencies/security truth changes, update the master plus:
docs/design/osl-simple-spec.md
docs/design/osl-internal-build-checklist.md
and the `/osl` projection. Write a compact task report and Resume-here block.

Assigned task:
<goal>

Own only:
<files/interfaces>

Do not:
<non-goals/forbidden actions>

Required acceptance:
<objective positive, negative, runtime, screenshot/two-identity gates>

Report:
<decisions, diff, exact evidence, remaining unknowns, integration handoff>
```
