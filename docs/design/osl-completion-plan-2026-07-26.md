# OSL completion plan — 2026-07-26

Every outstanding requirement, mapped to owners and sequenced by dependency.
Rule that governs everything: **one file has exactly one owner at a time.** Most of the
2026-07-25/26 session's lost time came from two agents editing one file, not from the
problems themselves.

## Status of the whole surface

| # | Requirement | State |
|---|---|---|
| 1 | Encrypted send reaches Discord | **DONE** — verified by screenshot, 4 covers in the live conversation |
| 2 | Eye decrypts + paints over rows | **BLOCKED** — offscreen filter drops the visible rows |
| 3 | Composer active 100%, never self-disables, no flash | Partially — hides itself after a send (diagnosed, unfixed) |
| 4 | Composer disappears on lock-off / − box | **DONE** (90 ms) |
| 5 | 0-frame drag follow | Not built — needs `WM_WINDOWPOSCHANGED` |
| 6 | 0-frame scroll, correct at every offset | Not built — needs `SetWinEventHook`; no wheel edge exists |
| 7 | 0-frame engage | Not reachable as literally specified (see Honest limits) |
| 8 | Resize / DPI resistant | Not proven |
| 9 | Nitro / GUI-change resistant | Not proven — must key off measured geometry only |
| 10 | 1:1 with Discord typography + row geometry | Not proven for the transcript |
| 11 | White corner artefacts | **DONE** |
| 12 | Caret lands in OSL composer on lock | **DONE** (`AttachThreadInput`) |
| 13 | New encryption scheme built | **DONE** — `osl-ratchet-next`, wire `0x10` |
| 14 | New encryption **wired** | Not wired — owner has now asked for it |
| 15 | Image/file send, 1:1 in Discord | **NOT STARTED** — a send touches Discord zero times |
| 16 | View-once images + messages | Built, two-phase path never executed |
| 17 | Timed deletion | Built, **not wired** to broker |
| 18 | Bilateral burn | Built, **not wired**; guided Discord deletion built but inert |
| 19 | View-once link for non-OSL | Designed, not built |
| 20 | Non-OSL link warns the sender | Wording drafted, not built |
| 21 | Verified both ways (P2P) | Blocked on a second identity |
| 22 | Keyserver deployed | **DONE** — 0026 + 0027 applied, worker live, smoke-tested |
| 23 | Second OSL identity | Not created |

## Wave A — parallel, disjoint files, start immediately

**A1 · Eye correctness and 0-frame** — owns `native_discord_overlay.rs`, `broker.rs`, `overlay.ts`, `overlay.css`
Covers 2, 3, 5, 6, 8, 9, 10. Order: offscreen filter → scroll re-arm → composer-hidden (b) →
`WM_WINDOWPOSCHANGED` 0-frame follow → resize/DPI/Nitro robustness → 1:1 typography.
This is the critical path; nothing downstream is provable until the eye paints.

**A2 · Images/files 1:1 in Discord** — owns `native_attachment_transport.rs`, `crates/ipc/attachment_wire.rs`,
`crates/ipc/decoy_mp4.rs`, `native_image_viewer.rs`, `peer_attachment_io.rs`
Covers 15, 16. Must first verify Discord still passes `video/mp4` untranscoded; if not, the
decoy container premise is dead and the spoiler-block fallback is the design.
Recommendation on record: R2 stays authoritative, the Discord file is a payload-free decoy
purely for native layout, so OSL keeps deletion control.

**A3 · Non-OSL view-once link** — owns `cipher-store-cf/`, the landing page, `crates/crypto` link lane
Covers 19, 20. Fragment-held key, server structurally incapable of decrypting, one-time
retrieval, angle-bracket URL so Discord's unfurler cannot burn the view, single neutral aged
domain, generic landing page identical for every outcome, and sender-facing wording that
claims only "the link dies after one view".

## Wave B — after A1 releases `broker.rs`

**B1 · Wire burn + timed deletion + ratchet** — owns `broker.rs`, `main.rs`, `security.rs`, `crates/ipc`
Covers 14, 17, 18. Three separate wirings, one owner, because all three touch the same
call sites and doing them serially in one head avoids the collision class.
Order: timed-deletion clocks → burn `0x0A`/`0x0B` arms → ratchet `0x10` behind the version pin.

## Wave C — needs a second identity

**C1 · Two-identity P2P verification** — covers 21, 23.
Second identity runs beside the first (`%APPDATA%`-keyed identity, identifier-keyed mutex and
WebView2 UDF). Proves: real cross-identity decrypt, Received/Opened receipts, two-phase
view-once reveal, bilateral burn apply-on-peer, inbox eviction under real traffic, and the eye
painting rows OSL did not send.

## Honest limits — state these, do not quietly design around them

- **0-frame engage is not reachable as specified.** Capture resistance must be proven before the
  composer is ever visible; revealing on a cached measurement can show an unverified surface for
  a frame. Achievable: low tens of ms, from 1690–2390 ms today. 0-frame *follow* and *scroll* are
  reachable.
- **`encrypt_v3` has no forward secrecy.** Burn, SKDM and session-reset deliberately stay on it,
  because recovery cannot depend on the ratchet it repairs. Marked WEAKER in the claims table.
- **`osl-ratchet-next` is unreviewed.** `DESIGN.md` carries a standing instruction against real
  traffic before external cryptographic review. The owner has asked for it wired; wiring it
  behind the monotone version pin (no silent fallback) is the mitigation, not a substitute.
- **View-once and burn cannot be enforced against a modified client, a screenshot, or a camera.**
  Never write "disappears forever".
- **Guided Discord deletion is inert by design** until three facts are measured live: the row-menu
  key, and Discord's exact menu/dialog/button labels. Posted keys are proven to work; posted
  wheel messages are proven not to.

## Standing engineering rules

- Never compare a live element against geometry recorded earlier — that single assumption caused
  five distinct defects (write stall, discarded correct write, stranded cover, Enter gate that
  could only pass when empty, reclaim refusing OSL's own text).
- Fail closed everywhere; absent decode means do not paint, never paint at a guess.
- Never log plaintext, cover, draft, key material or conversation names — fixed `&'static str`
  labels, counts, roles, lengths and shapes only.
- `--features core` is mandatory for `apps/osl-hub` library tests, or the run is vacuous.
- The bin cannot build on Linux (`rfd` backend), so `#[cfg(test)]` in bin modules never executes.
- One cargo invocation at a time; rust-analyzer double-indexing OOMs the WSL VM.

Resume here
Current verified state:
Exact build/worktree:
Current owner and exclusive files:
Next unblocked action:
Command/scenario:
Expected result:
Known blocker/risk:
Master/internal-checklist rows to update on completion:
