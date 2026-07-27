# OSL Discord — State Map: Intended vs Actual

**Status:** descriptive map only. No fixes are proposed here; a separate plan document owns that.
**Date of survey:** 2026-07-25. **Tree:** `/home/liamw/discord-privacy-client`, branch `HEAD` (detached), working tree clean.
**Method:** static read of source and doc comments. Nothing was built, run, or launched. Live-verified facts supplied by the owner are marked *(verified live)*.

> ### Current correction — the 2026-07-25 eye findings are retracted as current claims
>
> Sections 4.3–5.4 below are preserved as the historical survey record, not as a
> description of current production source. The renderer now invokes
> `rehydrate_native_discord_overlay_history`, and rehydrated rows carry placement
> geometry to it. The later `2dc9172` mitigation accepts only peer-to-self wires
> on that history path, but independent review proved this is **directional
> suppression, not authenticated visible-row attribution**: neither the prose
> token nor `VisibleMessageRow` binds a Discord poster/message ID to the cover.
> In particular, the historical statements that both orientations are accepted
> and that wire orientation proves who posted a visible row must not be quoted as
> current behavior.
>
> ### ⚠ The tree changed while this map was being written
>
> `apps/osl-hub/src/native_discord_overlay.rs` grew from **5,696 → 5,816 → 5,991 lines** across this survey — another session is editing it continuously. Citations for that file were re-verified against the 5,816-line version and the headline finding re-confirmed at 5,991; **individual line numbers in that one file may already be off by a few lines. Search by function name, not by line.** Every other file was stable: `native_discord_adapter.rs` 16,860; `main.rs` 4,661; `broker.rs` 6,843; `native_window_host.rs` 9,960 — identical at the start and end of the survey.
>
> **Two rounds of concurrent edits touched the exact code discussed in §1.4 and neither fixed the defect there** — the second round documented the inverted behaviour as if it were correct. See §1.4a. This is worth knowing before reading anything else here.

---

### Where to start

| If you want… | Go to |
|---|---|
| The product model, in the owner's terms | §0 |
| Why the composer takes no input right now | **§1** — this is the top-priority section |
| Why the eye draws nothing | §4.3 |
| Why a friend's message never appears | §5 |
| Everything that is wrong, ranked | **§10** |
| What is not known and must be observed, not read | §11 |

---

## 0. How it is supposed to work

This is the owner's authoritative product model. Where a doc comment in the code says something different, the code comment is **wrong** and that disagreement is recorded as a finding, not as a design alternative.

1. **The lock is encryption, and only encryption.**
   Lock ON: what the operator types is encrypted, and a cover sentence ("flagtext") is what actually goes to Discord.
   Lock OFF: ordinary Discord typing.
   The lock changes **nothing** about what is displayed.

2. **The eye is the only control over display.**
   Eye OFF: plain native Discord — full history, everyone's messages, scrollback, cover text shown as-is, nothing drawn over it.
   Eye ON: OSL draws its decrypted text over those Discord rows, in place.

3. **Lock ON + eye OFF** means that after Enter the operator sees the cover sentence appear in the conversation, like any other Discord message.

4. **The composer always shows the operator's real words while typing**, regardless of the eye.

5. **The eye must work on ANY row OSL can decrypt** — including rows OSL did not itself send, and rows that predate the current session.

6. **On Enter the flagtext is sent instantly and in the background**, then immediately masked by the eye. No visible intermediate state, no flashing.

7. **The protected composer is active 100% of the time and never hides or deactivates itself.**

### How the implementation is shaped (neutral description)

OSL runs as a Tauri app (`apps/osl-hub`, Rust) with three relevant windows:

| Window | Label | Built at | Role |
|---|---|---|---|
| OSL shell | `main` | Tauri app setup | The trusted parent/owner of everything else |
| Protected composer | `native-discord-overlay` | `native_discord_overlay.rs:2523` `build_overlay_window` | A transparent WebView (`apps/osl-hub-ui/overlay.html` + `src/overlay.ts`) sized to Discord's measured composer rectangle. This is the box the operator types their plaintext into. |
| Capture shield | `native-discord-shield` | `native_discord_overlay.rs:2485` | An opaque black window that sits immediately behind the composer to defeat screen capture, sized to exactly the rows OSL is painting. |

Discord itself is a **separate process** whose top-level window OSL "borrows": OSL re-owns it by writing `GWLP_HWNDPARENT` to OSL's `main` HWND (`native_window_host.rs:6451`, `:6508`) and tethers its rectangle to OSL's host area. Discord is therefore an ordinary **z-order sibling** of the protected composer, both owned by `main`.

Everything OSL knows about Discord's internals comes from Windows accessibility — MSAA (`IAccessible`, `accHitTest`) and UI Automation. There is no OCR, no injection into Discord's process, and no Discord API use on this path.

### Build flavours — read this before anything else

`apps/osl-hub/Cargo.toml:37` sets `default = []`, and `:44` declares `discord-qa-shell = []`. **The QA shell is not in the default feature set**, so a shipping build takes every `#[cfg(not(feature = "discord-qa-shell"))]` branch.

That distinction is load-bearing for this whole document. Several behaviours that appear to exist when reading the file top-to-bottom exist **only in the QA build**. The production variants are frequently stubs that return `false` or an empty vector. A reader who does not track the `cfg` will conclude the feature works.

The most consequential examples, all detailed below:

| Capability | QA shell | Production |
|---|---|---|
| Composer focus reclaim after Discord steals focus | real (`native_discord_overlay.rs:2890-2902`) | `false` — disabled (`:2905-2913`) |
| Focus acquisition with `SetForegroundWindow` + verification | yes (`:3189-3211`) | bare `window.set_focus()`, unverified (`:3213-3218`) |
| Composer carrier stack | raises to `HWND_TOPMOST` (`:1500-1563`) | same-band reorder only; topmost forbidden (`:1646-1671`) |
| Per-row overlay bindings sent to the renderer (`visibleCarrierRows`) | yes (`main.rs:1457-1458`) | **field does not exist** |
| Painted row rectangles | just-sent carrier rows | `rehydrated_row_rects` cache, never populated in production (see §4.3) |
| Composer re-measure on a periodic/ready basis | yes | transition + backstop only (`:2997-3014`) |
| Composer z-order breadcrumb file | yes (`:1443-1474`, newly added) | absent |

### The written docs describe a different implementation

Before treating anything in `docs/` as a specification for this feature: **it is not one.**

`README.md:17-20` says so directly:

> "This README describes the original Discord-specific client. The newer multi-service standalone shell under `apps/osl-hub*` is a separate pre-release preview…"

`docs/phase-7-design.md`, `docs/phase-7c-selectors.md`, `docs/phase-7c-manual-tests.md` and `docs/phase-9-c1-permissive-decrypt.md` are all written against `src-tauri/src/injection/boot.js` — a WebView2 injection into discord.com. The feature mapped in this document is the *native accessibility adapter* in `apps/osl-hub`, which shares no code with it.

Consequences for this map:

- **The eye is documented nowhere.** A search of `docs/**/*.md` and `README.md` for *eye*, *visibility toggle*, *reveal*, *show decrypted*, *hide decrypt* returns only two incidental prose hits in `docs/design/layer-10-discord-internals.md`. The eye exists **only in source comments** (`native_discord_overlay.rs:202-231`; `overlay.ts:515-527`; `main.ts:3513-3520`). The owner's authoritative model is, today, documented only in code comments — every design doc predates it.
- **The lock means something else in the docs.** There it is a tri-state whitelist-summary icon whose click performs a bulk whitelist mutation (`docs/phase-9-c1-permissive-decrypt.md:50-53`; `README.md:64-74`). In the model and in the code it is a binary open/close of OSL's protected composer. Same icon, different control.
- **The docs specify the opposite display default.** `docs/design/osl-gui-final-plan.md:425` and `:446` say inbound capsules render decrypted **immediately** with *"a control to reveal the original carrier"* one action away. The model says eye OFF is plain Discord and *"nothing is 'revealed'; there was never anything on top"* (`overlay.ts:518-524`).
- **The docs give display a second suppressor.** `docs/design/osl-gui-final-plan.md:425` and `docs/design/osl-social-hub-implementation-plan.md:317` say low decryption or layout confidence must keep the native carrier visible. The model says the eye is the only control over display.
- **The former generic-overlay contradiction is resolved in current docs.**
  `docs/design/external-overlay-security-contract.md` now limits its lock,
  retention and uncertainty behavior to an uncalled source prototype, explicitly
  says no production lifecycle uses that cache, and distinguishes the separate
  shipping native Discord renderer.
- **Other planning documents still describe conditional composer visibility.**
  Those design targets must be assessed against the separate native Discord
  overlay rather than treated as production evidence from `external_overlay.rs`:
  `docs/design/osl-gui-final-plan.md:361`, `:363-365`, `:467`, `:600`, and
  `docs/design/osl-social-hub-implementation-plan.md:270`.
- **The docs contradict each other on cover text.** `README.md:50-54` says the carrier is *"generated by a language model"*; `docs/design/osl-gui-final-plan.md:432` and `:482` forbid exactly that, and `docs/design/osl-social-hub-implementation-plan.md:303` forbids steganography outright — while `crates/stego` implements it and is on the live send path.

**Every one of these is a finding, not an alternative design.** The owner's model wins in all cases. But it means there is currently **no written specification of this feature that agrees with the product**, which is a plausible structural cause of the reactive, symptom-at-a-time work this map exists to replace.

One point where the docs *do* agree with the model, and it is worth keeping: `docs/phase-9-c1-permissive-decrypt.md:69-71` — *"OSL guarantees: keys-bound recipients can decrypt; no one else can. That's it. OSL does NOT decide who you want to hear from."* That is the doc statement closest to "the eye must work on ANY row OSL can decrypt". The Hub code contradicts it by requiring per-friend scope approval before any decrypt (`broker.rs:1583`; `security.rs:1003-1013`).

---

## 1. TOP PRIORITY — the protected composer's window, z-order and focus subsystem

### 1.1 Observed symptom *(verified live, do not re-derive)*

The protected composer window exists at its correct rectangle but is **not above the borrowed Discord window** and receives no input. Typing lands in Discord's own box and Enter sends **plaintext**. The distinguishing feature is the cyan ring `.composer-box::after`, `rgba(73,214,255,.55)` (`apps/osl-hub-ui/src/overlay.css:272-281`) — OSL's composer has it, Discord's does not.

### 1.2 The intended contract, as the code states it

> `native_discord_overlay.rs:1585-1586`
> "Place the protected composer directly above the borrowed Discord window without changing its z-order band."

> `native_discord_overlay.rs:1379-1387`
> "Discord is borrowed as a top-level window owned by the same trusted OSL parent, so it is an ordinary z-order sibling of the composer: activating it raises it above the composer, which then sits behind Discord while Windows still reports it visible."

> `native_discord_overlay.rs:1652-1656`
> "Preserve the reviewed production contract exactly: capture-excluded overlay immediately above its opaque shield, and never in the topmost band. Activating the borrowed Discord window raises that sibling above the composer, so the composer is first re-inserted immediately above Discord and the shield is then restacked immediately behind the composer."

> `native_discord_overlay.rs:3921-3927`
> "Raising an already-correct topmost WebView on every full guard interrupts WebView2 keyboard delivery on Windows. The exact stack therefore only needs mutation when opening, after a verified geometry rebuild, or when the bounded read-only probe above proved Discord is now above the composer."

Production also **forbids** the composer from being topmost: `verify_owned_overlay_window` (`:1788-1809`) rejects the window outright when `WS_EX_TOPMOST` is set (`forbidden_ex_style` at `:1800`), and this runs every guard pass through `verify_owned_overlay_pair` (`:3856`). So in a shipping build the composer must win z-order by ordinary sibling reordering, and only by that.

### 1.3 The actual mechanism

The guard loop (`start_guard`, `native_discord_overlay.rs:3296`) ticks about every 16 ms. Its z-order work is:

1. **Probe** — `protected_composer_is_above_discord` (`:1389-1414`), at most every 200 ms (`PROTECTED_STACK_PROBE_INTERVAL`, `:1348`; gate at `:3484`). It walks `GW_HWNDPREV` **upward** from Discord's root looking for the overlay HWND (`resolve_window_is_above`, `:1357-1377`), bounded to 128 steps (`PROTECTED_STACK_WALK_LIMIT`, `:1343`). `Some(true)` = above, `Some(false)` = proven below, `None` = undecided → treated as no drift. **This probe is correct.**
2. **Decide** — `:3488-3489`: `stack_drifted = stack_probe_due && active_protected_stack_drifted(...)`.
3. **Correct**, only on one of four transitions — `:3928`: `if !ready || geometry_changed || composer_restored || stack_drifted { active_ensure_carrier_stack(...) }`.
4. Production `active_ensure_carrier_stack` (`:1646-1671`): `raise_protected_composer_above_discord` (`:1657`) → `ensure_shield_stack` (`:1658`) → `enforce_transparent_protected_composer` (`:1669`).

### 1.4 **The raise is inverted — highest-confidence root cause of the blocker**

`raise_protected_composer_above_discord`, `native_discord_overlay.rs:1625-1635`:

```rust
SetWindowPos(
    overlay_hwnd,
    discord_root,          // <-- hWndInsertAfter
    0, 0, 0, 0,
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
)
```

Win32 `SetWindowPos`'s second parameter is `hWndInsertAfter`: "a handle to the window **to precede** the positioned window in the Z order." The positioned window is placed **below** it. So this call puts the protected composer **immediately behind the borrowed Discord window** — the opposite of what the function's name and its callers' comments claim.

This is not an outside inference about Win32 semantics. **The same file depends on the opposite direction 300 lines earlier and empirically verifies it:**

- `ensure_shield_stack`, `:1295-1306`: *"Put the opaque shield immediately **behind** the capture-excluded overlay"* — and does `SetWindowPos(shield_hwnd, overlay_hwnd, …)`.
- It then **proves** the result at `:1318-1327`: `GetWindow(overlay_hwnd, GW_HWNDNEXT) == shield` and `GetWindow(shield_hwnd, GW_HWNDPREV) == overlay`. `GW_HWNDNEXT` is the window *below*; `GW_HWNDPREV` is the window *above*. So `SetWindowPos(A, B)` demonstrably leaves A one step **lower** than B, and this file already relies on that.

Within one file, the codebase both asserts and depends on `SetWindowPos(A, B) ⇒ A below B`, and then issues that same call expecting `A above B`.

**Consequences, all consistent with the live symptom:**

- The composer is placed below Discord on every one of the four transitions, including the first reveal.
- The 200 ms probe then correctly reports `Some(false)` — walking up from Discord never reaches the overlay.
- `stack_drifted` is therefore **permanently true**, so the guard runs `active_ensure_carrier_stack` every 200 ms indefinitely, re-applying the same inverted write. This is a non-converging correction loop, not a one-off.
- That path also calls `enforce_transparent_protected_composer` unconditionally (`:1669`), so a DWM blur-behind write happens every 200 ms too — exactly the "cadence" the surrounding comments (`:1549-1553`, `:1659-1668`) insist must never happen.
- The comment at `:3921-3927` warns that re-asserting the stack on a cadence "interrupts WebView2 keyboard delivery". That warning is currently being triggered continuously, so it is plausible this alone would break typing even if focus and z-order were otherwise fine.

**Corroborating detail:** `raise_protected_composer_above_discord` has **no post-write verification**, unlike `ensure_shield_stack` (`:1318-1327`). A verification step of the kind the shield already has would have caught this the first time it ran.

**Test coverage cannot catch it.** `the_protected_stack_is_re_asserted_only_when_it_actually_drifted` (`:5703-5740`) is a *source-text* test: it string-searches the function body for the presence of `window_is_topmost(...)` and the absence of `HWND_TOPMOST` / `SWP_SHOWWINDOW`. It never checks the argument order of `SetWindowPos`, and it cannot — it never executes the function. Several other z-order guarantees in this file are protected the same way.

### 1.4a What the concurrent edit changed — and did not

During this survey another session modified exactly this code. The changes were:

| Change | Where (new numbering) |
|---|---|
| New `composer_zorder_label` + `qa_record_composer_zorder` — writes a one-line `composer_zorder=… visible=… topmost=… stage=…` breadcrumb to `%TEMP%\osl-discord-qa-composer-zorder.txt` on the probe cadence | `:1424-1477`, called at `:3495` |
| New band predicate `composer_raise_is_a_same_band_reorder(overlay_topmost, discord_topmost) = overlay_topmost == discord_topmost` | `:1581-1583` |
| The raise's bail-out condition replaced: was `window_is_topmost(discord_root)`, now `!composer_raise_is_a_same_band_reorder(...)` | `:1613-1621` |
| `raise_protected_composer_above_discord` is now compiled into **both** builds (`#[cfg(target_os = "windows")]` only) instead of production-only | `:1595` |
| The QA carrier stack now calls the raise *after* its `HWND_TOPMOST` raise | `:1555-1561` |

**The `SetWindowPos` argument order is unchanged.** The defect described in §1.4 is fully present in the current file.

Worse, the new doc comment now writes the inverted semantics down as if they were the intent — `:1565-1570`:

> "**`SetWindowPos(composer, discord, ...)` inserts the composer immediately *after* Discord in one z-order list**, which is only a correction while both windows are in the same band."

"After" in a Windows Z-order list means **lower**. The comment states the behaviour correctly and then calls it "a correction". So the edit hardened the *precondition* (only reorder within a band) while leaving the *direction* wrong, and has now documented the inversion as correct. This makes the defect harder to spot on the next read, not easier.

One further consequence of the new band guard: in the **QA** build the composer is deliberately `HWND_TOPMOST` while Discord is not, so `composer_raise_is_a_same_band_reorder` returns `false` and the newly added QA call at `:1561` is a permanent no-op. Its own comment concedes this (`:1555-1560`: *"the band guard inside makes it a no-op the moment the composer is topmost and Discord is not"*). The QA build therefore still proves nothing about the production path — which is the same structural blind spot that let the defect ship (see §0, build flavours).

### 1.5 The second half: focus is never reclaimed in production

Even with correct z-order, keyboard focus is a separate problem.

| Fact | Evidence |
|---|---|
| The composer is created unfocused and invisible | `:2540-2560` — `.always_on_top(false)`, `.focused(false)` (`:2554`), `.visible(false)` (`:2555`) |
| Focus is requested exactly once, at the first reveal | `:3909-3919`, inside `if !ready { … }` |
| The production focus routine is a bare Tauri call with no verification | `:3213-3218` — `window.set_focus()` only. The QA variant (`:3189-3211`) additionally calls `SetForegroundWindow` and polls up to 10 × 20 ms for confirmation. |
| **The focus-reclaim path is compiled to `false` in production** | `:2905-2913` — `active_should_reclaim_composer_focus` returns `false` unconditionally. The real predicate `should_reclaim_composer_focus` (`:2860-2867`) exists and is reachable only from the QA variant at `:2890-2903`. |
| Consequently `focus_reclaim_pending` is always `false` in production | `:3387-3397` |
| …so `active_focus_overlay` and the `OVERLAY_REFOCUS_EVENT` emit in the reclaim branch are unreachable in production | `:3459-3470`. The renderer handler `overlay.ts:1479-1481` (`draft.focus({preventScroll:true})`) is therefore dead in a shipping build. |
| The only other re-focus is after a surface resample, and it is best-effort | `:3880` — `let _ = window.set_focus();`, gated on `restore_overlay_focus` |
| After a send, the backend does try to hand focus back | `main.rs:1434-1436` — `let _ = window.set_focus();` |

So: once Discord takes the foreground for any reason, production OSL has **no mechanism at all** to take it back. The design intent for that mechanism exists, is written, and is switched off for shipping builds.

### 1.6 Other actors that push Discord above the composer

| Actor | Evidence | Effect |
|---|---|---|
| **Engaging the lock itself, in production** | `main.ts:3383` — `if (!discordQaShell && !(await focusActiveNativeCompanion()))` … | Immediately before the composer is revealed, production OSL **deliberately brings Discord forward**, which raises it to the top of the band and gives it the foreground. The QA build skips this step entirely. |
| `NativeWindowHostState::focus` on a borrowed Discord | `native_window_host.rs:5827-5829` — `ShowWindow(SW_RESTORE)`, `BringWindowToTop(window)`, `SetForegroundWindow(window)` | This is what `focusActiveNativeCompanion` reaches. Raises Discord to the top of the band and gives it the foreground. |
| The borrowed-window tether repair | `native_window_host.rs:3978-3982` — `insert_after = HWND_TOP` when `plan.zorder` | Raises Discord to `HWND_TOP`. The comment at `:3926-3935` says it deliberately avoids doing this unconditionally and defers the composer's relative order to the overlay guard — but when the repair plan does fire, it still puts Discord above the composer. |
| Any user click into Discord | (Windows behaviour) | Ordinary sibling activation raises Discord. This is exactly the case the guard was written for. |

### 1.7 Where the raise silently does nothing

`raise_protected_composer_above_discord` returns `Ok(())` without acting when (`:1613-1621`):
- Discord's root HWND is null,
- Discord's root **is** the overlay, or
- **the two windows are in different z-order bands** — `!composer_raise_is_a_same_band_reorder(overlay_topmost, discord_topmost)`, `:1581-1583`.

The matching probe returns `None` — "no drift" — when the overlay is not topmost and Discord is (`:1400-1402`). So if anything ever promotes Discord into the topmost band, production OSL stops trying entirely and reports no problem. This is deliberate (production forbids the composer joining that band, `:1800`) but it is an unreported, silent surrender: the operator gets a composer they cannot type into and no signal at all.

### 1.8 A secondary hypothesis worth keeping on the list *(lower confidence)*

Probes temporarily add `WS_EX_TRANSPARENT` to OSL's own windows so accessibility hit-testing can see through them (`native_discord_adapter.rs:2160`, `:2274-2283`, `:5570-5600`). A window carrying that bit is **click-through and receives no mouse input**, which would also produce "typing goes into Discord's box".

The code is aware of the hazard and mitigates it: a `Drop` guard restores the bit on every exit path including unwind (`native_discord_adapter.rs:5721`), a `FORCED_CLICK_THROUGH` ledger is written *before* the style change (`:5598`), and `restore_stale_click_through` (`:5608`) reclaims anything held longer than `PROBE_CLICK_THROUGH_MAX_HOLD_MS = 4_000` (`:2266`). The doc comment at `:5586-5593` states plainly that a thread that never returns leaks the bit and "would leave OSL's own UI permanently unclickable while still looking fine."

I could not determine statically whether this is happening. It is a real, documented mechanism for the exact symptom, and it should be ruled out by observation rather than by reading. **Uncertain.**

Note that `WS_EX_TRANSPARENT` is explicitly preserved by the composer's frame-cleanup masks (`native_discord_overlay.rs:4033-4062`), so nothing in the frame contract would strip a leaked bit.

### 1.9 What is NOT the cause (ruled out)

| Candidate | Why it is ruled out |
|---|---|
| The composer is permanently `WS_EX_TRANSPARENT` by design | No. `enforce_transparent_protected_composer` (`:2156-2222`) only manipulates DWM blur-behind and NC-rendering policy for per-pixel alpha. It sets no extended style and touches no z-order; the doc comment at `:2150-2154` says so explicitly. |
| The frameless window hook swallows input | No. `protected_frameless_window_proc` (`:1928-1968`) intercepts only `WM_NCCALCSIZE` and `WM_NCPAINT` and chains everything else to the previous procedure. |
| The composer is hidden or destroyed | Not in the observed state — the owner reports the window exists at its rect. (It *does* hide itself in other situations; see §1.10.) |
| The composer is topmost and fighting Discord | Production forbids topmost and fails the session closed if it is set (`:1800`, checked every guard pass via `verify_owned_overlay_pair` at `:3856`). |

### 1.10 The composer hides itself — five ways

The product model says the protected composer must be active 100% of the time and must never hide or deactivate itself. The guard hides it in four distinct places:

| # | Trigger | Evidence | Notes |
|---|---|---|---|
| 1 | Foreign app takes the foreground (neither Discord nor OSL) | `:3402-3416` | Hides composer + shield, sets `composer_temporarily_hidden = true` |
| 2 | OSL's `main` window is minimized | `:3462-3472` | Same |
| 3 | **Lock is off and nothing is painted** | `:3567-3576` | Same. Because production never populates painted rows (§4.3), this fires **whenever the lock is off**. |
| 4 | Composer measurement failed (self-occlusion) | `:3650-3663` | Hides the pair and waits up to 20 × 15 ms for the hide to land, so the next measurement has line of sight |
| 5 | Native surface re-sample — geometry change, DPI change, or the 120 s backstop | `:3775-3800` | Hides the pair, waits for the hide to land, samples Discord's composer pixels, then re-reveals |

Cases 4 and 5 are the mechanisms most likely to be experienced as "flashing": each is a real hide, a bounded wait, and a real reveal on OSL's own windows. Case 5 is triggered by any geometry change.

The design comments acknowledge each of these individually and defend them — e.g. `:3560-3566` (bail out before measuring, because *"an idle session paying for that every 200 ms is exactly the load that froze Discord's UI thread once already"*) and `:3648-3651` (sampling before the hide lands would capture OSL's own surface). None of them is careless. But collectively they mean the composer's presence is a *derived* property of five different guard conditions rather than the invariant the product model requires.

### 1.11 Gap table — window / z-order / focus

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| W1 | Composer inserted immediately **above** Discord | `SetWindowPos(overlay, discord_root, …)` inserts it immediately **below** Discord | `native_discord_overlay.rs:1625-1635` vs. proof at `:1295-1327` | **Blocks core feature** |
| W2 | Drift is corrected when detected | Correction never converges; the same inverted write repeats every 200 ms indefinitely | `:1348`, `:3484-3489`, `:3928-3931` | **Blocks core feature** |
| W3 | The raise is proven to have worked | No verification after the write; the shield stack next to it has one | `:1625-1641` vs `:1318-1327` | Degrades — masks W1 |
| W4 | Composer regains focus when Discord steals it | Reclaim predicate hardcoded `false` in production builds | `:2905-2913` | **Blocks core feature** |
| W5 | Focus acquisition is proven | Production uses bare `window.set_focus()` with no `SetForegroundWindow` and no confirmation loop | `:3213-3218` vs QA `:3189-3211` | Degrades |
| W6 | Stack re-asserted only on real transitions, to protect WebView2 keyboard delivery | Re-asserted every 200 ms because drift never clears; blur-behind rewritten on the same cadence | `:1669`, `:3921-3931` | **Blocks core feature** — plausible independent cause of "no keys reach the composer" |
| W7 | Composer never hides itself | Hides in five situations, one of which (lock off) is unconditional in production | `:3402-3416`, `:3462-3472`, `:3567-3576`, `:3650-3663`, `:3775-3800` | **Blocks core feature** (model item 7) |
| W8 | No visible intermediate state | Any geometry change triggers a hide → wait → sample → reveal cycle on OSL's own windows | `:3775-3849` | Degrades (model item 6) |
| W9 | Nothing raises Discord above the composer at engage time | Production explicitly brings Discord forward immediately before revealing the composer; the QA build does not | `main.ts:3383`; `native_window_host.rs:5827-5829` | **Blocks core feature** — combined with W1 and W4 this is the full causal chain |
| W10 | Discord's other raise paths cooperate with the composer guard | Tether repair uses `HWND_TOP` on Discord when its plan fires | `native_window_host.rs:3978-3982` | Degrades |
| W11 | Drift is always either corrected or reported | Silently gives up when the two windows are in different bands, with no signal to the operator | `native_discord_overlay.rs:1400-1402`, `:1613-1621` | Degrades |
| W12 | z-order invariants are tested | The guarding tests are source-text string searches that never execute the function or check argument order | `:5703-5740` | Degrades — this is why W1 survived review |

**The causal chain for the live blocker, in order:** engage brings Discord forward (W9) → the composer is revealed behind it → the raise that should fix that inserts it lower instead (W1) → the probe correctly detects this forever (W2) → nothing ever reclaims focus (W4) → and the resulting 200 ms write cadence is exactly the one documented to break WebView2 keyboard delivery (W6). Keystrokes go to Discord, and Enter sends whatever Discord's own box holds.

---

## 2. Lock semantics — the code's stated intent contradicts the product model

This is a **design-record conflict**, not a bug in the usual sense. Three independent places in the codebase state, in prose, that the lock owns whether OSL has a composer at all. The owner's model says the lock owns encryption only and the composer is always present.

| Where | What it says |
|---|---|
| `apps/osl-hub-ui/src/overlay.ts:163-171` | "The lock is encryption only, and it governs exactly one thing on screen: who owns the message box. Lock on, OSL's composer is over Discord's… **Lock off, OSL has no composer at all** and they type into Discord normally." |
| `apps/osl-hub-ui/src/overlay.css:234-240` | "Lock off: the operator types into Discord's own message box, so OSL must not paint a composer anywhere." → `:root[data-osl-lock-engaged="false"] #write-pane { display: none; }` |
| `apps/osl-hub/src/native_discord_overlay.rs:352-358` | "Lock on: the composer rectangle, extended upward over the rows OSL paints. **Lock off: only the rows OSL paints** — the operator types into Discord's own box, so OSL must own no pixel of it." |
| `apps/osl-hub/src/main.rs:1452-1455` | "Whether what the operator types is being encrypted, and therefore whether OSL owns a composer over Discord's real message box." |

Implementation of that (contradicted) intent:

- Renderer: `applyLockEngaged` (`overlay.ts:172-175`) writes `data-osl-lock-engaged`; the CSS rule above removes `#write-pane` from layout.
- Native geometry: `protected_surface_rect` (`native_discord_overlay.rs:359-373`) returns the composer rect only when `lock_engaged`; otherwise it returns the painted-rows bounds, or `None`.
- Native visibility: `:3465-3474` hides both windows when `!lock_engaged && !shielded`.

**Consistency note in the code's favour:** the codebase is genuinely careful that the lock does not affect the *transcript*. `OverlaySessionState.lock_engaged` is deliberately a flag inside the session rather than the session's existence (`:216-221`: "Lowering the lock used to end the session, which took the decrypted display down with it — and display is the eye's business, not the lock's"), and the eye is explicitly not reset on session teardown (`overlay.ts:1511-1515`). So the *display* half of the model is honoured; the *composer presence* half is not.

| # | Intended (owner) | Actual | Evidence | Severity |
|---|---|---|---|---|
| L1 | Lock off leaves the protected composer present and active | Lock off removes `#write-pane` from the DOM layout and, natively, hides the whole composer window | `overlay.css:238-240`; `native_discord_overlay.rs:359-373`, `:3465-3474` | **Blocks core feature** |
| L2 | Lock changes nothing about display | Honoured — the transcript layer is untouched by the lock | `overlay.ts:163-171`, `:1511-1515`; `native_discord_overlay.rs:216-221` | None (correct) |
| L3 | Lock gates encryption/send | Honoured — `require_engaged_lock` refuses prepare/send with the lock down | `main.rs:1386-1395`, `:1408`; `overlay.ts:827-831` | None (correct) |
| L4 | The cyan "protection is on" ring | Only ever visible with the lock engaged, by construction — because `#write-pane` is `display:none` otherwise | `overlay.css:255-281` | Follows from L1 |

---

## 3. Send path — intended vs actual

### 3.1 The chain

| Stage | Where | Notes |
|---|---|---|
| Enter / click in the protected composer | `overlay.ts:1182-1201` (keydown), `:1203-1216` (keyup), `:1014` (button), gesture FSM `overlay-send-gesture.ts:13-52` | Three modes: `button`, `double` (two Enters within 1200 ms), `single` (Enter on keydown). |
| `sendDraft` | `overlay.ts:817-1012` | Re-reads backend state before every send (`:864-875`). |
| IPC | `native-overlay-adapter.ts:160` `prepare_native_discord_overlay_text`, `:218` `send_native_discord_overlay_carrier` | QA build has a fused `send_native_discord_qa_atomic_text` (`:253`). |
| Encrypt + commit to OSL inbox | `main.rs:1755-1809` → `broker.rs:1661-1677` → `broker.rs:1912-1921` | Lock gate at `main.rs:1408`. Encryption happens **before** Discord is touched (`main.rs:1820-1825`, `:1895-1899`). |
| Cover text ("flagtext") generation | `crates/ipc/src/prose_token.rs:181-231` → `crates/stego/src/mode1.rs:294-310` → `crates/stego/src/bigram.rs` | 64-bit blob id ‖ 32-bit HMAC = 96 bits, arithmetic-decoded through a bigram language model into ordinary English chat words. |
| Geometry admission | `discord_carrier_geometry.rs:172-247` `plan_carrier`; consumed at `native_discord_adapter.rs:962-1014` | Only `CarrierDecision::RowOverlay` yields a cover text (`:165-169`). |
| Locate Discord's composer | `native_discord_adapter.rs:8662-9116` `locate_with_timeout` | MSAA `accHitTest` first, UIA second. |
| Type the carrier | `native_discord_adapter.rs:12120-12598` `place` | `SendInput` with `KEYEVENTF_UNICODE`, 8 UTF-16 units per batch. |
| Verify what landed | `:11009-11121` `await_exact_carrier_in_composer` → `:10751-10844` `composer_exact_text_probe` | UIA subtree walk → `TextPattern.DocumentRange` → MSAA-tree fallback → `ValuePattern` (proof-only). |
| Press Enter | `:10943-10986` `send_enter` | Scan-code `0x1C`. Preceded by an autocomplete-dismiss Escape when the carrier contains `@ # : /` (`:1694-1701`, `:10361-10369`). |
| Confirm consumption | `:11761-11792` `confirm_carrier_consumed` | Re-locates and requires the composer to be **empty**. Delays `[40,80,160,240]` ms in production. |

### 3.2 Cover-text mechanism — worth stating plainly

The carrier is **not** zero-width characters, homoglyphs, or whitespace steganography. It is fully visible plain English whose *word choice* encodes the payload. A fixture example from the test corpus is `native_discord_adapter.rs:16103`:

> `"ok i will weekend again with you get what i was thinking usual"`

This matters for the product model: with lock ON and eye OFF, that sentence is exactly what the operator (and everyone else) sees in the conversation. That is the intended behaviour per model item 3.

`pro_context_cover.rs` (a phrase-table cover generator, `FREE_COVER = "🔒 OSL private message"` at `:16`) is **not** on this path; it is referenced only from `burn_scope` (`main.rs:2944`) and state registration (`main.rs:4487`).

### 3.3 Recently fixed *(verified live — stated here so the map is complete, not re-derived)*

- Probe-point unification.
- `accHitTest` window-rooted locate — no hiding of OSL windows, no flashing, during locate (`native_discord_adapter.rs:8754-8761`: "FIRST, and with no line of sight arranged at all… the Z order of OSL's protected composer… cannot affect the answer").
- Keyserver quota now evicts instead of refusing.
- Composer read-back now spans the whole Slate document via a UIA subtree walk with an MSAA-tree fallback. The old read returned a single Slate text node via `ValuePattern`/`accValue` and broke every send. The `complete` flag (`:10463-10476`) is what gates a re-drive; the doc comment at `:10466-10475` records the "appended the remainder twice" defect this fixed.
- NBSP / zero-width canonical folding — `canonical_accessible_text` (`:3821-3825`), composed of line-break folding (`:3785`), no-break-space folding (`:3798`), invisible stripping (`:3808`, set at `:3757`) and edge trimming (`:3815`). Doc at `:3839-3846` records that the previous version only trimmed the ends and "a failed send bricked protection until a human cleared the composer by hand".

### 3.4 There is no plaintext-send path *inside* OSL

Worth stating explicitly, because the live symptom is "Enter sends plaintext". The only strings OSL ever types into Discord's composer are (a) the wordbank flagtext, or (b) the operator's own previously-suspended Discord draft being typed back (`native_discord_adapter.rs:11635`, `:11749`). Every failure mode refuses and surfaces a notice.

**Therefore the observed plaintext send is not OSL sending plaintext — it is the operator typing directly into Discord because OSL's composer never received the keystrokes.** That is a §1 problem, not a §3 problem.

### 3.5 Send-path gaps

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| S1 | Enter sends the flagtext instantly and in the background, with no visible intermediate state | The carrier is *typed into Discord's visible composer character-batch by character-batch*, then verified by read-back, then Enter is injected. In Atomic mode this is one `SendInput` burst, but the carrier is still visibly present in Discord's box until Enter is confirmed. Total budget can reach seconds. | `:12278-12308` (atomic), `:1321` `CARRIER_OBSERVE_DEADLINE_MS = 2000`, `:11771-11773` | Degrades (model item 6) |
| S2 | "Compatibility / match typing" mode is an option | It types one character at a time with a per-character sleep, making the intermediate state unavoidable and long | `:12309-12324`, `:1271-1286`, `:495` | Cosmetic (Pro-gated opt-in) |
| S3 | A failed send is clearly reported | Renderer adapters swallow the backend error string: every wrapper returns `null` on throw | `native-overlay-adapter.ts:161`, `:221`, `:264-266` | Degrades — the *fact* of failure is surfaced (`overlay.ts:897-900`, `:919-922`) but the *reason* never reaches the operator |
| S4 | `markerSent` reflects whether Discord really got the row | In production the per-row proof (`visible_carrier_row`) does not exist; `markerSent` is derived from `carrier.status/placed/enterSent` only | `overlay.ts:909-918`; `native_discord_adapter.rs:754`, `:785`, `:829` are `#[cfg(any(test, feature = "discord-qa-shell"))]` | Degrades |
| S5 | A refused carrier is always cleaned out of Discord's box | When `may_continue_input` fails, the clear requires the same proof that just failed, so the carrier can be left in Discord's composer | `:12506-12512`; mitigated by `remember_typed_carrier` (`:714-726`) so the next calibration recognises OSL's own litter | Degrades |
| S6 | State bookkeeping is reliable | Several `remember_*` writers silently no-op on a poisoned mutex | `:921-934`, `:715-726`, `:755-780` | Cosmetic (fails toward viewport-only, never toward a wrong send) |
| S7 | Calibration fails closed | A stale suspended Discord draft that cannot be restored no longer blocks protection — "PROTECTION OPENS ANYWAY" | `:9392-9424` | Cosmetic (deliberate, documented) |

---

## 4. Eye / display path — intended vs actual

### 4.1 Where the eye lives

| Concern | Location |
|---|---|
| Persisted per-scope | `security.rs:200` `decrypt_display_by_scope: BTreeMap<String, bool>`; read `:1460`, written `:1498` |
| Exposed as | `security.rs:129` `ScopeSecurity.decrypt_display_enabled` |
| Native cache | `native_discord_overlay.rs:213` `protected_display_visible: AtomicBool`, doc at `:202-212` ("the ONLY thing that decides whether OSL displays anything over Discord's message rows") |
| Resolver | `native_discord_overlay.rs:766-777` |
| Refresh cadence | `:757` `PROTECTED_DISPLAY_REFRESH_INTERVAL = 500 ms`, applied at `:3431-3441`, only while OSL is foreground |
| Overlay-side toggle | `overlay.ts:1225` → `main.rs:1717` `set_native_discord_overlay_security` |
| Header-side toggle (the eye button) | `main.ts:2333` → `:3521` → `main.rs:1751` `set_active_hub_context_security`, plus an event to the overlay window (`main.ts:3543`) |
| Renderer applier | `overlay.ts:529` `applyDecryptDisplayVisibility`, doc `:515-527` |

The renderer's own doc comment matches the owner's model almost word for word (`overlay.ts:515-527`): *"The eye is the ONLY control over what the operator sees… The lock is deliberately absent from this function."*

### 4.2 Eye OFF is correct

Eye OFF genuinely leaves unmodified Discord on screen. Nothing blanks, dims, or covers Discord content.

- `messageList.hidden = true` and every row's plaintext hidden (`overlay.ts:530-537`).
- All row geometry dropped: `verifiedCarrierRows = decryptDisplayEnabled ? bindings ?? [] : []` (`overlay.ts:348`).
- `painted_message_row_rects` short-circuits to empty (`native_discord_overlay.rs:798-800`).
- `shielded = false` → the opaque shield is hidden (`:3456`, `:1273-1277`, `:1600-1604`).
- With the lock also down, the composer window leaves the screen entirely (`:3465-3474`).

The comments record this as a deliberate fix for a previous regression:

> `native_discord_overlay.rs:318-323`: "There is deliberately no fallback that spans the message band. The previous header-to-composer surface is what made the lock black out the conversation, fight Discord for geometry, and swallow the scrollback — eye off is now literally unmodified Discord because OSL has no window there at all."

> `:1594-1598`: "With the eye off there is no protected pixel anywhere on the message list — the operator is looking at Discord — so an opaque window there would hide their real conversation for no protection at all. That was the black band."

### 4.3 Eye ON does nothing in a production build

This is the second core-feature failure, independent of §1.

The renderer paints per-row overlays from `verifiedCarrierRows`, sourced from `state.visibleCarrierRows` (`overlay.ts:358-361`, `:344-356`). That backend field is **QA-only**:

```rust
// apps/osl-hub/src/main.rs:1457-1458
#[cfg(feature = "discord-qa-shell")]
visible_carrier_rows: Vec<NativeDiscordCarrierRowDto>,
```

In a shipping build the field is absent, so `state.visibleCarrierRows` is `undefined`, `verifiedCarrierRows` becomes `[]`, and `applyVerifiedCarrierRows` runs its unconditional clear loop — which sets `row.hidden = true` on every transcript row (`discord-carrier-row-binding.ts:107-126`) — and then re-shows nothing.

The native half is empty too. `painted_message_row_rects` (`native_discord_overlay.rs:793-828`) reads `rehydrated_row_rects` in production. That cache is written in exactly one place: `record_rehydrated_row_rects` at `native_discord_adapter.rs:6507`, inside `read_visible_message_rows`. The chain to reach it is:

`overlay renderer` → Tauri command `rehydrate_native_discord_overlay_history` (`main.rs:2509`) → `read_visible_message_rows` (`main.rs:2545`) → `record_rehydrated_row_rects`.

**No renderer ever invokes that command.** It is registered in the Tauri handler at `main.rs:4601`, but a search of `apps/osl-hub-ui/src` finds no `invoke("rehydrate_native_discord_overlay_history")` — the only occurrences are in a test file (`discord-headless-qa-contract.test.ts:37`, `:43`) describing what the *headless QA* harness does. `native-overlay-adapter.ts` invokes 16 commands (lines 29–366); this is not one of them.

Consequently, in production:
- `rehydrated_row_rects` is always empty.
- `painted_message_row_rects` is always empty.
- `shielded` is always `false`, so the capture shield never shows.
- `protected_surface_rect` with the lock off always returns `None`.
- The `!lock_engaged && !shielded` hide branch (`:3465-3474`) fires **every time the lock is off**, which is what makes gap L1 unconditional rather than situational.

The guard even documents its own expectation of this state, at `native_discord_overlay.rs:3446-3450`:

> "Exactly the rows OSL is painting decrypted text over… Empty with the eye off, and **empty in every production session until the bounded row reader also returns row rectangles.**"

### 4.4 The eye is keyed to rows OSL sent, not to rows OSL can decrypt

Even in the QA build where bindings exist, the matcher is:

```ts
// apps/osl-hub-ui/src/overlay.ts:344-356
verifiedCarrierRows = decryptDisplayEnabled ? bindings ?? [] : [];
for (const item of transcript.root.querySelectorAll(...)) clearCarrierRowGeometry(item);
for (const binding of verifiedCarrierRows) {
  const item = outgoingBubbles.get(binding.messageId);   // outgoing only
  if (item) applyCarrierRowGeometry(item, binding);
}
```

`outgoingBubbles` (`overlay.ts:141`) is populated only by OSL's own sends. The bindings themselves come from `refresh_verified_sent_carriers` (`native_discord_adapter.rs:850`) over `pending_sent_carrier` / `visible_sent_carriers` (`:480-482`) — a local ledger of rows OSL just typed, committed at `:786` `commit_pending_sent_carrier`.

So the architecture is **"remember what I sent and where"**, not **"read the screen and decrypt what is there"**. Model item 5 requires the latter.

### 4.5 The decode-from-screen machinery exists but is orphaned

The correct mechanism is written and looks complete:

- `read_visible_message_rows` (`native_discord_adapter.rs:6395-6512`) walks Discord's MSAA message list, keeps the visible message rows, and captures both their text and their on-screen rectangles.
- `broker::rehydrate_native_discord_overlay_history` (`broker.rs:1470-1518`) feeds each row's own text through `authenticate_oriented_prose_pointer` with **both** orientations, so the operator's own sent messages and the peer's both decode (`broker.rs:1495-1508`).
- `rehydrated_rows` (`broker.rs:1526-1539`) guarantees every row in yields exactly one row out, in order — undecodable rows keep their place carrying `None`, because "a dropped row is a hole behind an opaque capture shield exactly where the operator's history should be."

Two things stop it being the eye:

1. **Nothing calls it** (see §4.3).
2. **It discards the geometry on the way out.** `read_visible_message_rows` maps away the bounds at `native_discord_adapter.rs:6509-6511` (`.map(|(locator, line, _bounds)| (locator, line))`), and `rehydrate_native_discord_overlay_history` further drops the locator (`broker.rs:1485`). Even wired up, the renderer would receive `{flagtext, plaintext}` pairs with no rectangle to place them at. The rectangles go only into the native `REHYDRATED_ROW_RECTS` cache, which is used for *shield sizing*, not for text placement.

### 4.6 Even if wired, the trigger is a single edge

`rehydrate_native_discord_overlay_history` is gated by `begin_rehydrate` (`native_discord_adapter.rs:948-958`), which answers `false` for a repeat of the same scope binding. `main.rs:2503-2507`:

> "TRIGGER: the scope-change edge and nothing else… There is deliberately no timer, no poll, no retry and no backstop here: per-poll row reading is what froze this machine for 19,207 ms."

And `native_discord_overlay.rs:787-790`:

> "The cache answers empty until the first rehydration for the current scope and window generation completes, and **it is not refreshed by scrolling alone** — it reflects whatever that last completed read saw."

So the eye's row set would be a one-time snapshot taken when the conversation was opened. Scrolling, new inbound messages, and messages sent after that edge would never be added. This is a deliberate trade against a measured 19-second UI freeze; it is recorded here as a structural constraint on model item 5, not as carelessness.

### 4.7 Eye/display gaps

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| E1 | Eye ON draws decrypted text over Discord rows | Nothing is ever drawn in a production build: `visibleCarrierRows` does not exist in the production DTO, so all rows are cleared and hidden | `main.rs:1457-1458`; `overlay.ts:344-356`; `discord-carrier-row-binding.ts:107-126` | **Blocks core feature** |
| E2 | Eye works on ANY decryptable row | Bindings are keyed to `outgoingBubbles` — rows OSL itself sent | `overlay.ts:141`, `:352`; `native_discord_adapter.rs:480-482`, `:786` | **Blocks core feature** |
| E3 | Screen-read decryption feeds the display | `rehydrate_native_discord_overlay_history` is registered but never invoked by any renderer | `main.rs:4601` vs. absence in `native-overlay-adapter.ts` | **Blocks core feature** — never-wired, not abandoned |
| E4 | Screen-read rows carry placement geometry to the renderer | Bounds are dropped before the data leaves Rust | `native_discord_adapter.rs:6509-6511`; `broker.rs:1485` | **Blocks core feature** |
| E5 | Row set tracks the conversation | Single scope-change edge; not refreshed by scrolling or by new messages | `main.rs:2503-2507`; `native_discord_overlay.rs:787-790` | Degrades (deliberate trade) |
| E6 | Eye OFF shows plain Discord | Honoured, including no shield and no surface | `overlay.ts:530-537`; `native_discord_overlay.rs:798-800`, `:1594-1598` | None (correct) |
| E7 | The capture shield covers exactly what OSL paints | Correct by construction, but since nothing is painted in production the shield never appears at all | `:1200-1205`, `:3451-3456` | Follows from E1 |

---

## 5. Receive / drain path — intended vs actual

**Two different things are called "receive" in this codebase, and only one of them is implemented.**

### 5.1 What is implemented: an OSL key-server inbox drain

| Stage | Where |
|---|---|
| Poll scheduler | `overlay.ts:726` `scheduleReceivePoll` → `:735` `pollReceived` |
| Poll gate | `discord-qa-receive-policy.ts:8-14` — requires `overlayReady && decryptDisplayEnabled && (qaShell \|\| !document.hidden)` |
| Backoff | `overlay.ts:762` — 2 s, doubling to a 10 s ceiling |
| IPC | `native-overlay-adapter.ts:270` `open_native_discord_overlay_text` |
| Command | `main.rs:2575-2604` (overlay-label gated at `:2578`) |
| Drain | `broker.rs:2081-2093` → `:2142` `drain_peer_inbox_text` |
| Source | `broker.rs:2163` `client.get_control_inbox(&identity)` — **the OSL key server, not Discord** |
| Eye gate | `broker.rs:2156` `allow_messages = display.decrypt_display_enabled`; `:2219` skips message items when off (acks still processed) |

This path is real, wired, and authenticated (`verify_manual_v3_type` + `decrypt_direct_manual_v3_payload`, `broker.rs:2229-2244`).

### 5.2 What is not implemented: decoding a carrier row OSL did not generate

Reading a Discord row off the screen and decrypting it is `read_visible_message_rows` + `rehydrate_native_discord_overlay_history` — orphaned, see §4.5.

### 5.3 Inbound messages decrypt but cannot be displayed

An opened inbound message is appended via `appendBubble("incoming", …)` (`overlay.ts:751-753`, `:568`), which pushes a transcript row and stores the plaintext. `syncTranscript()` then calls `applyVerifiedCarrierRows`, whose first loop hides **every** row and whose second loop only re-shows rows found in `outgoingBubbles`. An incoming message therefore never gets a rectangle and is never visible.

### 5.4 Receive gaps

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| R1 | A friend's message arrives and the eye shows it in place over their Discord row | The inbox drain decrypts it, but the display layer hides it because it is not in `outgoingBubbles` | `overlay.ts:751-753` + `:344-356` | **Blocks core feature** |
| R2 | The row on screen is the thing decoded | Decoding is keyed by key-server `message_id`, not by the carrier text on screen. The screen-decoding path is orphaned. | `broker.rs:2163-2244` vs `broker.rs:1470-1518` | **Blocks core feature** |
| R3 | Receive works regardless of display state | Polling stops entirely when the eye is off (`shouldPollDiscordOverlay`), and the drain skips message items | `discord-qa-receive-policy.ts:8-14`; `broker.rs:2156`, `:2219` | Degrades — arguably correct (nothing to show), but it means the eye also gates *fetching* |

---

## 6. Lifecycle — open / adopt Discord, lock engage, calibrate

### 6.0 Two terms that mean something other than they sound

- **"Suspend"** in `suspend_and_calibrate` does **not** suspend the Discord process or its threads. It suspends the **operator's composer draft text**: saves it, clears Discord's box so OSL can measure and type into an empty composer, and restores it later. There is no `NtSuspendProcess`, `SuspendThread`, or `DebugActiveProcess` anywhere in the repo. The one `CREATE_SUSPENDED` (`native_window_host.rs:6201`) exists only so a *dedicated-mode* child can be joined to a job object before its primary thread resumes.
- **Discord is not reparented.** Shipping Discord uses `ExistingSession` mode, which is an **owner** relationship written through `GWLP_HWNDPARENT`. `SetParent` (a true child reparent) is used only by the dedicated path (Telegram/Signal, and a PTB-gated dedicated-Discord path). This is why the composer and Discord are z-order *siblings* rather than parent/child, which is the whole reason §1 exists.

### 6.1 Adopt

| Stage | Where |
|---|---|
| Public entry | `native_window_host.rs:2025` `host()`, `:2041` `host_mode()`; Windows impl `:5046` |
| Mode fork | `:112` `cold_host_action` — Discord → `ClaimExisting` |
| Locate Discord.exe | Fixed channel manifests `:2283`, `:2321`, `:2328`; install root via `FOLDERID_LocalAppData` (`:5302`); signature/publisher trust `:4978`, `:5683` |
| Find the window | `:5290` `claim_existing_host` → `EnumWindows` (`:5349`), per-candidate gates at `:5499-5545` (class `Chrome_WidgetWin_1`, `:61`), ambiguity cap `MAX_EXISTING_WINDOW_CANDIDATES = 32` (`:4975`) |
| Relaunch if needed | `:5406` `claim_or_relaunch_existing_host`, launching with `--force-renderer-accessibility=complete` and `--enable-features=UiaProvider` (`:46-49`), polling every 100 ms to `EXISTING_SESSION_DISCOVERY_TIMEOUT = 15 s` (`:2592`) |
| Adopt | `:6529` `adopt_existing_companion` |

The adoption itself, in order (`:6545-6668`): capture prior owner / styles / iconic / placement / rect → gate on `borrowed_style_is_preserved` (`:233`) → **arm the out-of-process recovery guardian** (`:6577`) → `SetWindowLongPtrW(window, GWLP_HWNDPARENT, owner)` (`:6593`) → swap `WS_EX_APPWINDOW` for `WS_EX_TOOLWINDOW` to remove the taskbar button (`:6599`) → `SetWindowPos(..., SWP_FRAMECHANGED|SWP_NOACTIVATE|SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER)` (`:6600`) → verify ex-style applied *and* `GWL_STYLE` unchanged (`:6609`) → `SW_RESTORE` → position → caption-button shield → tether worker.

**`GWL_STYLE` is never modified on a borrowed Discord window.** Only `GWL_EXSTYLE` and `GWLP_HWNDPARENT` change.

Crash safety is real and unusual: `BorrowedRecoveryGuardian` (`:3151-3202`) is a **separate child process** of the OSL binary that blocks on `WaitForSingleObject(parent, INFINITE)` and restores Discord's owner, ex-style and placement if OSL dies. If its 2 s handshake fails, adoption aborts *before any mutation* — the one cleanly fail-closed path in this subsystem.

### 6.2 Tether

Only Discord gets a tether (`:6650`). Worker `borrowed_window_tether_worker` (`:4014`), one pass `reconcile_borrowed_tether` (`:3815`).

Cadence is a backoff ladder, not a fixed timer: 16 ms active → 48 ms settling → 120 ms idle → 320 ms quiet (`:1542-1566`, selector `:1582`). The doc at `:1568-1580` records why: the previous 16 ms-while-active cadence was re-armed forever by the overlay guard's ≤400 ms call-in, giving a measured ~27–62.5 passes/s on a composite that was already aligned every time (`tether_repair_skipped_already_aligned=575`, zero corrections).

Per pass: identity check (cached 2 s, `:1404`) → parent validity → **owner repair** if `GWLP_HWNDPARENT` drifted (`:3834-3859`; Discord clears its own owner while recreating its Electron presentation, `:3838-3842`) → minimize/restore mirroring (`:3861-3887`) → bounded compositor-rebuild repaint, verified by distinct-colour sampling rather than `IsIconic` (`:3891-3918`, limit 3 at `:1486`) → read-only z-order and foreground reads → a single conditional `SetWindowPos` writing exactly the corrections the plan proved (`:3958-3989`) → post-verify.

Reconcile budgets differ by caller: 500 ms from OSL's UI thread (`:1240`), 3500 ms from the guard (`:1250`). The doc at `:1242-1249` explains: *"Discord's UI thread is measured to stall for up to 2889 ms inside one composer accessibility scan (865 ms for an MSAA row scan), and a reconcile's cross-process `SetWindowPos` queues behind exactly that work."*

### 6.3 Lock engage

`main.ts:3392-3395` → `setNativeDiscordProtectedOverlayOpen(token, true)` → `main.rs:1143` `set_native_discord_protected_overlay_open`.

Sequence (`main.rs:1207-1341`): caller must be `"main"` → resolve owner and current context → host must be Discord and match the context → `discord_overlay_target` → generation match → **`OverlaySessionState::activate`** (new epoch, `lock_engaged = true`, `native_discord_overlay.rs:487`/`:516`) → scope binding → **`calibrate`** (`main.rs:1247`) → `verified_surface_bounds` → sample Discord's composer pixels (`capture_verified_surface_guarded`, `:1271`) → apply adaptive presentation bounds → store the surface → `native_discord_overlay::show` (`:1329`).

### 6.4 Calibration

`native_discord_adapter.rs:1016` `calibrate` → `:9356` `suspend_and_calibrate`:

1. **Stale draft first** (`:9377-9424`). A draft held from a previous cycle is restored before anything else is measured. If it cannot be restored, protection **opens anyway** (`:9392-9424`) — a deliberate, documented non-fail-closed decision, surfaced through `unrestored_draft_notice`.
2. **Locate** (`:9426`) via `locate_for_calibration` (`:9325`) with a bounded retry ladder `[60, 140] ms` under a 400 ms budget (`:1950`, `:1954`), retrying only the five whitelisted transient diagnoses (`locate_failure_is_retryable`, `:1930-1941`). Doc at `:9311-9324`: *"The lock button looked dead because a single transient miss failed the whole open."*
3. **Classify what is in Discord's box** (`:9440`, `:1871-1880`): `Empty` / `OwnStrandedCarrier` / `OperatorDraft`.
4. `OwnStrandedCarrier` (`:9450-9484`): clear OSL's own litter, save nothing, re-locate, re-verify empty.
5. `OperatorDraft` (`:9485-9587`): re-verify unchanged, save into `SuspendedNativeDraft` (`Zeroizing<String>`), clear via **real synthesized input** (never `ValuePattern.SetValue`), restore on any failure. Never replaces a complete saved draft with a truncated partial (`:9497-9520`).
6. **Commit** (`:9589-9606`): write `binding` + `text_presentation`, return a receipt.

Cached at calibration (`NativeDiscordComposerState`, `:461-491`): `binding` (generation, conversation/name/automation-id/class-name hashes, bounds, display bounds), `presentation_bounds`, `text_presentation`, `suspended_draft`, `typed_carrier`, `prepared_visual`, `rehydrated_scope`, `unrestored_draft_notice`.

Invalidation is mostly by **read-time freshness gate** rather than eviction: `verified_composer_bounds` filters by `binding.display_bounds` (`:513-520`), `verified_text_presentation` by `text_presentation_inside_input` (`:554-560`), generation equality checked at `:405`, `:451`, `:749`, `:860`, `:4498`, `:5321`. Hard `clear()` (`:638-672`) runs on calibration failure (`main.rs:1256`), on disengage-to-nothing (`main.rs:1200`), and on any engage error (`main.rs:1378`).

### 6.5 Disengage

`main.rs:1166-1206`. Restore the suspended draft if held, then `disengage_lock()` (`native_discord_overlay.rs:471-474`) which sets `lock_engaged = false` and **returns `protected_display_visible()`**. If that is `true`, nothing else happens: the session, guard and surface stay alive so the eye can keep displaying. If `false`, the session is cleared and the windows hidden.

The doc comment at `main.rs:1178-1190` is the clearest statement of the owner's model anywhere in the tree:

> "The lock is ENCRYPTION ONLY. Lowering it must stop OSL owning Discord's message box and nothing else — it may not take the operator's decrypted display away, because display is the eye's business… This must hold in EVERY build: the QA shell used to hide the overlay and its shield unconditionally here, so the eye went dark whenever the lock came down."

**But the guard does not honour that.** `disengage_lock` keys the decision on the *eye flag*; the guard's hide branch keys on `!lock_engaged && !shielded`, where `shielded` comes from *painted rows*. In production, painted rows are always empty (§4.3), so `shielded` is always `false` and the guard hides the composer on lock-off **regardless of the eye**. The command layer and the guard layer disagree about what keeps the surface alive.

### 6.6 Lifecycle gaps

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| C1 | Calibration failure fails the engage cleanly with a useful reason | In production the failure is swallowed: `clear()` runs, execution continues, and the engage then dies three lines later with the unrelated message *"The verified native composer surface is unavailable"*. The `return Err(error)` is inside `#[cfg(feature = "discord-qa-shell")]`. | `main.rs:1252-1263` vs `:1266-1269` | Degrades — real cause is never reported |
| C2 | A failed engage tells the operator why | `adapters.ts:315-317` catches and returns `false`, discarding the Rust error string in non-QA builds; the UI shows a generic message | `adapters.ts:315-317`, `main.ts:3399-3401` | Degrades |
| C3 | Lock-off keeps the surface alive while the eye is on | `disengage_lock` keys on the eye; the guard keys on painted rows, which are always empty in production, so the composer is hidden on every lock-off | `main.rs:1191`; `native_discord_overlay.rs:3465-3474` | **Blocks core feature** (same root as L1/E1) |
| C4 | z-order repair always converges or reports | `borrowed_tether_owner_is_above_target` treats an exhausted 128-window walk as "no drift" | `native_window_host.rs:1287-1305` | Cosmetic (rare) |
| C5 | A failed compositor repaint is surfaced | After 3 attempts the tether abandons the repaint and disarms; Discord's surface can stay a flat colour while the composite reports "aligned" | `native_window_host.rs:3916-3921`, `:1486` | Degrades |
| C6 | A relaunched Discord OSL could not claim is cleaned up | On final discovery timeout only `child.try_wait()` is called; the spawned process is left running | `native_window_host.rs:5493-5497` | Degrades |
| C7 | A partially-adopted window is always rolled back | If the ex-style verification fails *and* `restore_guardian_snapshot` also fails, the window is left owner-linked with a live guardian child process until OSL exits | `native_window_host.rs:6616-6662` | Degrades (rare) |
| C8 | Engage is atomic | The session is activated (`lock_engaged = true`) before calibration; a calibration failure leaves that state set until the error handler at `main.rs:1348-1377` unwinds it | `main.rs:1241` vs `:1252` | Cosmetic |
| C9 | An engage failure preserves the operator's draft and the real error | If the draft cannot be restored, the original error is replaced by a generic *"OSL could not open protection or restore the saved Discord draft"* | `main.rs:1350-1370` | Degrades |
| C10 | The first guard pass is patient | After `FIRST_GUARD_GRACE = 3 s`, a foreground window that is neither Discord nor OSL closes the whole engage | `native_discord_overlay.rs:2675-2686`, `:3628-3632`, `:37` | Degrades |

---

## 7. Whitelist and burn

### 7.1 There is no server/channel whitelist on this path

The written design describes seven whitelist scopes — DM, group-chat full/per-user, server-channel full/per-user, entire-server full/per-user (`docs/phase-7-design.md:25-35`) — with "most-permissive wins" and "no blacklist concept" (`:39-41`).

**The Hub Discord path implements exactly one of them.** `broker::activate_manual_peer` (`broker.rs:200-226`) builds the only scope OSL ever uses for Discord:

```rust
let scope = ScopeInput { kind: ScopeKind::Dm, id: manual_peer_scope_id(...)?, server_id: None, channel_id: Some(channel_binding) };
```

`manual_peer_scope_id` (`security.rs:1017-1040`) is `SHA256("OSL-MANUAL-LOCAL-SCOPE-v2" ‖ service_id ‖ account_id ‖ person_id)` — **it contains no Discord guild or channel id at all**. And `manual_peer_scope_approved_for_binding` (`security.rs:983-990`) hard-rejects anything that is not `ScopeKind::Dm`.

So "which servers and channels OSL will operate in" is not a question this code can answer. It operates in exactly one place: the conversation with one verified friend on one linked service account. The other `ScopeKind` variants exist (`crates/ipc/src/scope.rs:46-51`) and are used by the person-roster DTOs, but are unreachable from the Discord adapter.

### 7.2 Enforcement

Single chokepoint: `security::require_manual_peer_scope_approved` (`security.rs:1003-1014`), whose predicate is `prefs.manual_approved_scopes.contains(key) && !prefs.burned_manual_scopes.contains(key)` (`:997-1001`).

Eleven call sites, all in `broker.rs`: send (`:1192`), decrypt/authenticate (`:1583`), inbox prepare (`:1781`), drain (`:2159`), history (`:2574`), attachments (`:2635`, `:2745`, `:2918`, `:3052`, `:4173`, `:4280`). The last two are inside dead functions (§8.4).

Not approved → send and decrypt both fail closed with *"Approve encryption for this friend before continuing"*; decrypt additionally masks to *"This encrypted message could not be opened"* (`broker.rs:1576`, `:1590`). Burned scopes can never be re-approved (`security.rs:1069-1072`) and their eye/TTL cannot be edited (`security.rs:1487-1490`).

### 7.3 The whitelist and eye controls do not exist in a production UI

`nativeDiscordHeaderControls` (`main.ts:2273-2278`): when `!discordQaShell` the Discord header renders **only** `Burn`, `Covertext`, and a disabled `AI Covertext`. The whitelist `+`/`−` buttons, the revoked-scope warning chip, and the eye toggle are all inside the `discordQaShell` branch.

The eye therefore has no visible control in a shipping build's Discord header. The only other surfaces that can write it are `set_native_discord_overlay_security` from the overlay window — whose controls live in `.overlay-runtime-controls`, which is `hidden` in the markup (`overlay.html:41-50`) — and burn (§7.5). **I did not find a reachable production UI path that turns the eye on or off.** Flagged as uncertain in §11.

### 7.4 What burn does

| Kind | Entry | What it destroys |
|---|---|---|
| Chat | `main.rs:4228-4262` → `security::burn_scope` (`security.rs:1512-1620`) or `burn_manual_peer_scope` (`:1637`) | Local SQLite rows for the channel, key material via `cmd_osl_apply_burn`, matching `outgoing_whitelists` entries, the burned-scope ledger, and remote cipher-store blobs (`:1606-1613`) and attachments (`:1721-1733`) |
| App ("Discord") | `main.rs:4159-4216` | Every pending scope in the service manifest, then `clear_and_hide` + broker clear. Asserts `login_profile_untouched: true, native_history_untouched: true` (`:4213-4214`) |
| Account | `main.rs:4036` → `cleanup::execute_full_hub_cleanup` (`cleanup.rs:112`) | A fixed compiled target list (`cleanup.rs:332-419`); symlinks refused (`:479`), roots validated (`:313`) |

### 7.5 Burn forces the eye off

`revoke_manual_scope_state` (`security.rs:1751-1763`):

```rust
prefs.manual_approved_scopes.remove(storage_key);
prefs.burned_manual_scopes.insert(storage_key.to_owned());
prefs.decrypt_display_by_scope.insert(storage_key.to_owned(), false);   // <- writes the eye
ttl.entries.remove(storage_key);
```

This is a write to display state from something that is not the eye. It is defensible (a burned scope has no keys, so there is nothing to display), and it is permanent because the scope can never be re-approved — but under the product model's "the eye is the only control over display" it is a violation worth knowing about.

### 7.6 What burn does not do

- **It never touches Discord.** No Discord message is edited or deleted on any burn path. This matches `docs/design/burn-contract.md:14-17` and contradicts nothing in the product model, but it does contradict `README.md:80-88`.
- **It sends no burn notice to peers**, despite `README.md:82-83` promising *"a burn notice goes to the other members so their copies go dark too."* The UI checkbox is disabled with the label *"The consent-and-acknowledgment workflow is unavailable in this build"* (`main.ts:2738`), and `docs/design/burn-contract.md:29-30` confirms the transport is unwired.

### 7.7 Whitelist / burn gaps

| # | Intended | Actual | Evidence | Severity |
|---|---|---|---|---|
| B1 | Per-server / per-channel whitelist | Only a synthetic per-friend DM scope exists; guild and channel ids are not part of the scope identity | `broker.rs:200-226`; `security.rs:983-990`, `:1017-1040` | Degrades (documented feature absent, not a model item) |
| B2 | The operator can see and change the whitelist | The whole whitelist UI is QA-shell only | `main.ts:2273-2278` | Degrades |
| B3 | The eye is the only writer of display state | Burn writes `decrypt_display_by_scope = false` | `security.rs:1758-1761` | Cosmetic (defensible, permanent) |
| B4 | Burning notifies peers so their copies go dark | Not implemented; the control is disabled | `main.ts:2738`; `docs/design/burn-contract.md:29-30` vs `README.md:82-83` | Degrades (doc/product mismatch) |
| B5 | The roster reflects the live whitelist | Roster DTOs read `peer.outgoing_whitelists`, which nothing on the Discord path writes any more (§8.5) | `security.rs:1871`, `:1899`, `:1908`, `:2046` | Degrades |

---

## 8. Dead and unreachable code on the Discord path

Nothing here is marked `#[allow(dead_code)]` — a repo-wide grep over the Discord files returns zero hits. Everything below is dead by **reachability**, and none of it warns at compile time because `pub` items in a `pub mod` of a lib crate are never flagged as unused.

### 8.1 `composer_semantic_identity_matches` — **NOT dead** (correcting the brief's premise)

`native_discord_adapter.rs:404-411`. It has one production call site at `:10057`, inside `windows::refresh_bounds` (`:10022`), which is reached from `refresh_verified_bounds` (`:1104`) and is called from the guard at `native_discord_overlay.rs:3510` on the composer-refresh path.

It is the **runtime-id drift tolerance gate**: Chromium re-mounts the composer element on ordinary re-renders, so requiring runtime-id equality would make a stored calibration permanently unmatchable (`:396-403`). It is live and load-bearing. The similarly-named `saved_draft_belongs_to_composer` (`:440`) is a separate, deliberately looser gate for draft restore, not a replacement.

One real inconsistency: the function carries no `#[cfg]` while its only caller is inside `#[cfg(target_os = "windows")] mod windows` (`:4840`), so on non-Windows builds it is genuinely unreachable. That is a build-configuration wrinkle, not a logic defect.

### 8.2 `OVERLAY_OCCLUDES_COMPOSER` — **never-wired consumer**

`native_discord_adapter.rs:43-44`, with a doc comment at `:40-42` promising a specific response:

> "Reported when OSL's own protected surface covers the Discord composer… **The overlay guard responds by hiding that surface and re-measuring instead of failing permanently.**"

Producers: `:8931` (inside `locate_with_timeout`) and `:9469` (inside `suspend_and_calibrate`).
**Consumers: none.** The constant and its literal text appear nowhere in `native_discord_overlay.rs`, `native_window_host.rs`, `broker.rs`, `main.rs`, or any of `apps/osl-hub-ui/src`.

The behaviour the comment promises *does* happen, but generically: the guard's `Err(_)` arm (`native_discord_overlay.rs:3650-3663`) hides and re-measures on **any** refresh error. The named sentinel is never distinguished from an identity failure, a permission failure, or a timeout.

**Classification: never-wired.** The intended path — "this specific failure means OSL itself is the cause, so hide and retry rather than fail" — was written on the producing side and never implemented on the consuming side. It survives only because it happens to be a `String` error caught by a blanket arm. Because it is never distinguished, a genuine self-occlusion and a genuine identity change produce identical handling.

### 8.3 `apps/osl-hub/src/external_overlay.rs` — **entire module never wired** (~740 lines)

Declared through `pub mod external_overlay;` and referenced from **nowhere else
in production Rust**. All 16 public prototype types are unconsumed:
`ScreenRect`, `ExternalContextBinding`, `VerifiedFieldKind`,
`ComposerCalibration`, `WindowObservation`, `OverlayHiddenReason`,
`ComposerOverlayDecision`, `ComposerOverlayGuard`, `EncryptedCarrierBinding`,
`DecryptedHitTarget`, `DecryptionOverlayGuard`, `OverlayCacheTier`,
`VisibleCacheLimits`, `VisiblePlaintextCache`,
`EncryptedLocalOverlayCacheRecord`, and `EncryptedLocalCacheLimits`.

**Classification: implemented-unwired source prototype — production-unreachable
and superseded by `native_discord_overlay.rs` + `native_discord_adapter.rs`.**
The current `docs/design/external-overlay-security-contract.md` states that the
generic external overlay is not enabled for any service, has no production
consumer, Tauri handler, registered command or UI adapter, and is distinct from
the shipping native Discord surface.

Relevant to this map because `EncryptedCarrierBinding` and `DecryptionOverlayGuard` are the *decrypt-and-paint-over-any-row* abstraction the product model needs (§4.4). It exists, in a module nothing calls.

### 8.4 `burn_contract.rs` + `control_contract.rs` — **never wired** (~1,585 lines combined)

Declared at `lib.rs:2-3`. Every public entry point has zero production callers:

| Item | Definition | Only references |
|---|---|---|
| `plan_local_burn` | `burn_contract.rs:150` | `:300` (internal), `:612`, `:645` (tests) |
| `plan_remote_friend_burn` | `:271` | `:704`, `:713`, `:719`, `:733`, `:788` (tests) |
| `apply_remote_consent_revocation` | `:206` | `:753`, `:757`, `:764`, `:770` (tests) |
| `local_effects_digest` | `:140` | `:163` (internal), `:570` (test) |
| `BurnReplayJournal` | `:382` | `:795` (test) |

`control_contract.rs` is referenced only by `lib.rs:3`, and its import of six `burn_contract` types (`control_contract.rs:12-15`) is the only thing keeping those types nominally "used".

**Classification: never-wired.** These are the intended authorization/consent layer for burn. The actual burn executors (§7.4) run **without** any of it — `security::burn_scope` performs irreversible destruction with no `BurnConfirmation`, no `local_effects_digest` check, and no signature verification. `docs/design/burn-contract.md:29-30` concedes this.

### 8.5 `security::set_friend_scope_permission` — abandoned, and it left a stale reader

Defined at `security.rs:515` with a 12-line doc comment at `:503-514`. **Zero call sites.**

**Classification: abandoned — superseded by `set_manual_peer_scope_permission` (`security.rs:1043`)**, which is what the live Tauri command calls (`main.rs:3665-3674`).

The two have *different semantics*: the dead one wrote `peer.outgoing_whitelists` entries into `peer_map.json`; the live one only flips a key in `prefs.manual_approved_scopes`. But the roster DTOs still **read** `outgoing_whitelists` (`security.rs:1871`, `:1899`, `:1908`, `:2046`). So the roster UI reads a store that nothing on the Discord path writes any more — gap B5.

### 8.6 Attachment path — built on both ends, never connected in the middle

| Item | Definition | Status |
|---|---|---|
| `broker::prepare_peer_attachment` | `broker.rs:4143` | Zero callers, including tests |
| `broker::open_peer_attachment` | `broker.rs:4259` | Zero callers, including tests |
| `apps/osl-hub-ui/src/discord-media-staging.ts` | whole module | Imported only by its own test |

The two Rust functions each contain a live whitelist gate (`:4173`, `:4280`) that can never fire. **Classification: abandoned** — superseded by `prepare_peer_attachment_at` (`:4162`) and the `begin_peer_attachment` / `deliver_peer_attachment` / `commit_peer_attachment_open` trio (`:2619`, `:2726`, `:3038`). `discord-media-staging.ts` is **never-wired**: the Discord media path was built end-to-end on both sides and never joined.

### 8.7 Commands registered but never invoked

| Command | Registered | Invoked from UI |
|---|---|---|
| `rehydrate_native_discord_overlay_history` | `main.rs:4601` | **No** — see §4.3. This is the single most consequential never-wired item in the map. |
| `discover_mass_cleanup_targets` | `main.rs:4547` | No — and the backend is an unconditional `Err` (`mass_cleanup.rs:106-114`) |
| `execute_mass_cleanup_batch` | `main.rs:4548` | No — likewise `Err` (`mass_cleanup.rs:117-127`) |
| `list_core_features` | `main.rs:4544` | No |

### 8.8 Never-constructed enum variants

| Variant | Definition | Verdict |
|---|---|---|
| `DiscordSnapshotReason::NodeOrTimeLimitExceeded` | `native_discord_adapter.rs:3280` | **Never-wired** — the node/time-budget exhaustion case exists in the enum but nothing ever reports it, so a budget-exhausted snapshot is indistinguishable from other failures |
| `FallbackReason::DiscordCharacterCap` | `discord_carrier_geometry.rs:118` | **Abandoned** — the cap is enforced as an inline boolean guard at `native_discord_adapter.rs:1168` instead, so the operator never learns that length was the reason |

### 8.9 QA-only surfaces that the production build has no equivalent for

Not dead code, but structurally the same problem: the capability exists, and the shipping build has no version of it. Consolidated from §0.

- `visible_carrier_rows` DTO field and `NativeDiscordCarrierRowDto` (`main.rs:1457-1458`, `:1462-1500`).
- `PendingSentCarrierRow` / `VerifiedSentCarrierRow` / `commit_pending_sent_carrier` / `verified_sent_carriers` / `refresh_verified_sent_carriers` (`native_discord_adapter.rs:254-291`, `:783-870`).
- Focus reclaim (`native_discord_overlay.rs:2905-2913`).
- Verified focus acquisition (`native_discord_overlay.rs:3213-3218`).
- The entire whitelist + eye header UI (`main.ts:2273-2278`).
- `verify_native_draft_round_trip_for_qa` (`native_discord_adapter.rs:1057`) — QA-gated **and** never called even in QA.

---

## 9. Untested surfaces

"Untested" here means: no evidence in the tree that it has been exercised against real Discord, and in most cases no mechanism by which it could have been.

| Surface | Why it is untested | Evidence |
|---|---|---|
| **Eye ON rendering over Discord rows** | Cannot run in a production build at all — `visibleCarrierRows` does not exist and `rehydrated_row_rects` is never populated. In QA it can only paint rows OSL itself just sent. | §4.3, §4.4 |
| **Receive: decoding a carrier row OSL did not generate** | The only code that does this (`read_visible_message_rows` → `rehydrate_native_discord_overlay_history`) is never invoked from any renderer. | §4.5, §8.7 |
| **Inbound message display** | The drain decrypts, but the display layer hides every row not in `outgoingBubbles`. No inbound message has ever been drawn. | §5.3 |
| **P2P pairing / safety-number verification** | Not exercised in the Discord flow surveyed here. The whitelist `+`/`−` requires `verifiedPeer` (`main.ts:2281-2285`), and the whole control is QA-only, so the verified-peer branch of the production Discord path has no reachable UI. | `main.ts:2273-2285` |
| **The production z-order path** | The QA build raises to `HWND_TOPMOST` and the new same-band guard makes the newly-added QA call to the shared raise a permanent no-op. So no QA run has ever exercised the production reorder. | §1.4a |
| **Production focus acquisition** | QA verifies focus with `SetForegroundWindow` + a 10 × 20 ms confirmation poll; production does neither. The verified path is the only one ever observed. | §1.5 |
| **Composer read-back MSAA fallback** | Runs only when *both* UIA routes return `None` (`native_discord_adapter.rs:10796-10798`). Recently added; whether it has ever actually fired is not determinable from the tree. | §3.3 |
| **Multi-chunk / geometry-fallback sends** | The `flagtext = None` branch (OSL inbox only, no Discord row) has no visible test evidence. | `broker.rs:1892-1893`; `discord_carrier_geometry.rs:176-246` |
| **Burn's remote blob deletion** | `remote_blob_deletions_failed` reporting exists (`security.rs:1614-1619`) but there is no sign of live exercise. | `security.rs:1606-1619` |
| **Every z-order and stack invariant** | Guarded by *source-text* tests that string-match function bodies rather than executing them. They cannot detect a wrong argument, a wrong direction, or a wrong order. | `native_discord_overlay.rs:5703-5740` and the surrounding test module |

---

## 10. Prioritised gap list

### Tier 1 — blocks the core feature

| Rank | ID | Gap | Primary evidence |
|---|---|---|---|
| 1 | **W1** | `raise_protected_composer_above_discord` inserts the composer **below** Discord (`SetWindowPos` `hWndInsertAfter` inverted) | `native_discord_overlay.rs:1625-1635`, proven against `:1295-1327` |
| 2 | **W9** | Engaging the lock deliberately brings Discord forward first, in production only | `main.ts:3383`; `native_window_host.rs:5827-5829` |
| 3 | **W4** | Focus reclaim is compiled to `false` in production, so focus is never recovered | `native_discord_overlay.rs:2905-2913` |
| 4 | **W2 / W6** | Because W1 never converges, the guard re-writes the stack and the blur-behind region every 200 ms — the exact cadence documented to break WebView2 keyboard delivery | `:1348`, `:1669`, `:3921-3931` |
| 5 | **E1** | Eye ON paints nothing in a production build: `visibleCarrierRows` does not exist, so every transcript row is cleared and hidden | `main.rs:1457-1458`; `overlay.ts:344-356` |
| 6 | **E3 / E4** | The decode-from-screen path is written but never invoked, and discards row geometry before the data leaves Rust | `main.rs:4601`; `native_discord_adapter.rs:6509-6511`; `broker.rs:1485` |
| 7 | **E2 / R1** | Row bindings are keyed to `outgoingBubbles` — rows OSL itself sent — so the eye can never work on an inbound or historical row | `overlay.ts:141`, `:352` |
| 8 | **L1 / C3** | Lock OFF removes the composer: `#write-pane` goes `display:none`, and the native guard hides the whole window | `overlay.css:238-240`; `native_discord_overlay.rs:359-373`, `:3567-3576` |
| 9 | **W7** | The composer hides itself in five distinct situations | `:3402-3416`, `:3462-3472`, `:3567-3576`, `:3650-3663`, `:3775-3800` |

### Tier 2 — degrades the feature

| ID | Gap | Evidence |
|---|---|---|
| W3 | The raise has no post-write verification, unlike the shield stack beside it | `:1625-1641` vs `:1318-1327` |
| W5 | Production focus acquisition is unverified | `:3213-3218` |
| W11 | Cross-band drift is a silent surrender with no operator signal | `:1400-1402`, `:1613-1621` |
| W12 | z-order invariants are guarded by source-text tests that cannot catch W1 | `:5703-5740` |
| C1 | Calibration failure is swallowed in production; the engage dies later with an unrelated message | `main.rs:1252-1263` vs `:1266-1269` |
| C2 / S3 | Renderer adapters discard backend error strings, so the operator never learns why | `adapters.ts:315-317`; `native-overlay-adapter.ts:161`, `:221`, `:264-266` |
| S1 | The carrier is visibly typed into Discord's box and verified before Enter — there is a real intermediate state | `native_discord_adapter.rs:12278-12308`, `:1321` |
| S4 | `markerSent` has no per-row proof in production | `overlay.ts:909-918` |
| S5 | A refused carrier can be left stranded in Discord's composer | `native_discord_adapter.rs:12506-12512` |
| E5 | The row set is a single scope-change snapshot; scrolling and new messages never refresh it | `main.rs:2503-2507` |
| R3 | The eye gates *fetching*, not just display | `discord-qa-receive-policy.ts:8-14`; `broker.rs:2156`, `:2219` |
| W8 | Geometry changes cause a visible hide → sample → reveal cycle | `native_discord_overlay.rs:3775-3849` |
| W10 | Tether repair raises Discord to `HWND_TOP` | `native_window_host.rs:3978-3982` |
| B1 | No server/channel whitelist exists; only a synthetic per-friend DM scope | `broker.rs:200-226`; `security.rs:983-990` |
| B2 | Whitelist and eye controls are absent from the production UI | `main.ts:2273-2278` |
| B4 | Burn sends no peer notice, contradicting the README | `main.ts:2738` vs `README.md:82-83` |
| B5 | Roster reads `outgoing_whitelists`, which nothing writes any more | `security.rs:1871`, `:1899`, `:2046` |
| C5–C10 | Assorted lifecycle failure paths (see §6.6) | §6.6 |
| 8.2 | `OVERLAY_OCCLUDES_COMPOSER` is never distinguished from any other error | `native_discord_adapter.rs:43-44` |

### Tier 3 — cosmetic or deliberate

`L4` (cyan ring visibility follows from L1) · `S2` (compatibility typing mode) · `S6` (poisoned-mutex no-ops) · `S7` (calibration opens holding an unrestored draft) · `B3` (burn writes the eye off) · `C4` (128-window walk limit) · `C8` (engage is not atomic) · `8.8` (never-constructed enum variants) · `.composer-box::after` ring traces the window, not Discord's composer rect, and is a few pixels off (`overlay.css:266-271`).

---

## 11. Explicit uncertainties

Recorded rather than guessed.

1. **Whether a leaked `WS_EX_TRANSPARENT` is contributing to the blocker.** The mechanism is real and documented (`native_discord_adapter.rs:5586-5593`), the mitigation is real (`:5608`, `:5721`), and I cannot tell statically whether it fires. It produces the same symptom as W1 and should be ruled out by observation. §1.8.

2. **Whether the eye can be toggled at all in a production build.** The Discord header omits the control (`main.ts:2273-2278`) and the overlay's own runtime controls are `hidden` (`overlay.html:41-50`). I did not find a reachable production path that writes `decrypt_display_enabled`. There may be one I did not locate — this should be confirmed by trying it, not by reading.

3. **Whether the MSAA composer-read fallback has ever fired.** It is gated behind both UIA routes returning `None` (`native_discord_adapter.rs:10796-10798`) and was added recently.

4. **The exact live state of `native_discord_overlay.rs`.** It changed by 120 lines during this survey. §1 was re-verified against the 5,816-line version, but another edit may have landed since. Re-check before acting.

5. **Whether the tether's `HWND_TOP` repair actually fires in practice.** It requires `plan.zorder`, which requires a proven owner-above-target inversion *and* an active composite (`native_window_host.rs:1362-1366`). I could not determine its real-world frequency.

6. **Whether `rehydrate_native_discord_overlay_history` was deliberately unwired or the wiring was lost.** The command, its gate, its bounds, its privacy contract, and its every-row-in-one-row-out invariant are all fully built and carefully commented. Nothing in the tree explains why nothing calls it. This is the highest-value question in the whole map: it is the difference between "the eye was never built" and "the eye was built and one `invoke` is missing."

7. **Whether the production build has ever been run end-to-end at all**, as opposed to the QA shell. Every capability that differs between the two flavours is broken on the production side, which is more consistent with "production was never exercised" than with "production regressed." I have no direct evidence either way.
