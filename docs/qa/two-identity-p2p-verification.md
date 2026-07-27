# Two-identity P2P verification — operator procedure

Wave C1 of `docs/design/osl-completion-plan-2026-07-26.md`. Covers requirements
21 (verified both ways) and 23 (second OSL identity).

Everything below runs on Windows. The `osl-hub` bin cannot build on Linux
(`rfd` backend), so both the build and the harness are PowerShell.

---

## 1. What the second instance actually needs

| Thing | Keyed by | Consequence |
|---|---|---|
| single-instance mutex `<id>-sim` and marker window class `<id>-sic` | the Tauri **identifier**, embedded at compile time | two processes with one identifier cannot coexist; the second hands its argv to the first over `WM_COPYDATA` and exits |
| identity root `%APPDATA%\<id>\[discord-qa-shell-v1\]osl-core` | the same identifier | two builds with one identifier share one identity |
| WebView2 user-data folder `%LOCALAPPDATA%\<id>` | the same identifier | — |
| **every QA artefact** (`osl-startup-trace.txt`, `osl-qa-selftest.*`, `osl-discord-qa-*.txt`) | `std::env::temp_dir()`, **not** the identifier | two instances sharing one `%TEMP%` interleave their trails, clobber each other's verdicts, and **race for the self-test trigger** |

So there are **two** levers, not one:

1. **A distinct bundle identifier** — which requires a **separate build**. There
   is no env var and no CLI flag; that is deliberate
   (`apps/osl-hub/Cargo.toml:49-55`).
2. **A distinct `TEMP`/`TMP`** — set on the child process at launch.

`%APPDATA%` is deliberately **not** overridden. Tauri resolves `app_data_dir`
through `SHGetKnownFolderPath` while `crates/keystore/src/recipients.rs:191`
reads the env var, so overriding it would move one and not the other.

### How the two instances are told apart

By the **single-instance marker window class `<identifier>-sic`**, and by
nothing else. Every OSL build is titled `OSL Privacy`, which has already caused
the harness to grade a stale instance. The marker window is a top-level
`WS_VISIBLE | WS_POPUP` window at 0,0 with `WS_EX_TOOLWINDOW`, so `EnumWindows`
returns it; `Get-P2PBundleMap` in `scripts/qa/osl-p2p-win32.ps1` reads pid →
identifier from it and that is the only identity any script here consults.

The launcher additionally anchors on **the pid it started** plus its full
descendant closure, and excludes every window that existed before the launch by
hwnd. If it ever has to fall back to title matching it says so on the console
and in the JSON and grades the run `blocked` — it will not hand back a rig it
cannot distinguish.

---

## 2. What the operator must do by hand

The scripts deliberately stop short of these. **Nothing in `scripts/qa/` creates
an OSL identity or registers anything on the keyserver on its own initiative** —
the keyserver is live production serving the owner's real identity.

1. ~~**A second Discord account.** Create or use the Deckard alt.~~
   **Superseded 2026-07-26 — this is no longer a manual step and treating it as
   one cost most of an evening.** Three disposable Discord accounts already exist
   in Key Vault `osl-test-secrets-a7d5d9` as `osl-test-discord-01`, `-02` and
   `-03`, alongside `osl-test-account-manifest` describing which is which.
   Retrieve them just in time on the VM per `docs/testing/test-account-secrets.md`;
   no value may enter a prompt, report, event, log or screenshot. Instance A still
   stays on the account it is already adopted to.
2. **Decide that a second OSL identity may exist.** Starting a
   `discord-qa-shell` build creates a disposable device-bound identity
   (`apps/osl-hub/src/main.rs:5905` → `discord_qa_identity.rs:247`) and then
   blocks up to 30 s waiting for it to be **registered on the keyserver**
   (`discord_qa_identity.rs:30,134`). That registration is a real write to
   production. The launcher refuses to run without
   `-ConfirmCreatesIdentity` for exactly this reason.
3. **Decide that A may post into its live conversation.** Step P1 posts a real,
   human-paced message into whatever conversation instance A is adopted to. The
   harness refuses without `-ConfirmDriveLiveConversation`.
4. **Drive the receive side.** There is no file trigger for drain, reveal, burn
   or the eye — see §6. With `-OperatorDrivesReceiveSide` the harness prints
   what to click on instance B and watches while you do it.
5. **Never run either instance against the real conversation as part of
   development.** The harness is *written* to be able to; running it is your
   call.

---

## 3. What the scripts do

All under `scripts/qa/`. Every one writes a JSON verdict and exits non-zero on
anything that is not green.

| Script | Does | Output |
|---|---|---|
| `osl-p2p-win32.ps1` | shared Win32 layer. Enumeration lives in C# so a callback exception cannot silently truncate a list. No `SendInput`, no `mouse_event`, no `SetCursorPos`. | (dot-sourced) |
| `osl-instance-b-build.ps1` | builds a second hub with a different identifier via a `tauri build --config` overlay, `--features desktop,discord-qa-shell`; stages the exe **and `WebView2Loader.dll`** to `C:\OSL-QA-B` and records the sha256. Does **not** run it. | `%TEMP%\osl-instance-b-build.json` |
| `osl-p2p-pair.ps1` | copies each identity's `discord-qa-offer.v1.json` into the other's `discord-qa-peer-offer.v1.json`. Two file copies, nothing else. Refuses while either instance is running, and refuses if both offers carry the same `osl_user_id`. | `%TEMP%\osl-p2p-pair.json` |
| `osl-launch-instance-b.ps1` | starts B with its own `TMP`/`TEMP`, reads back **which bundle actually started**, proves A was not touched, and relocates only B's window. | `%TEMP%\osl-launch-b.json` |
| `osl-p2p-loop.ps1` | the six-step verification. | `%TEMP%\osl-p2p.json` |

### Order of operations

```powershell
# once, per build of B
scripts\qa\osl-instance-b-build.ps1 -Identifier org.oslprivacy.hubqab

# once, to create B's identity — this REGISTERS on the live keyserver
scripts\qa\osl-launch-instance-b.ps1 `
  -ExeB C:\OSL-QA-B\osl-privacy-hub.exe -BundleB org.oslprivacy.hubqab `
  -ConfirmCreatesIdentity
# ... let it finish setup, then close BOTH instances

# once, to pair them
scripts\qa\osl-p2p-pair.ps1 -BundleB org.oslprivacy.hubqab

# start A by hand, adopt it to the QA conversation, then:
scripts\qa\osl-launch-instance-b.ps1 `
  -ExeB C:\OSL-QA-B\osl-privacy-hub.exe -BundleB org.oslprivacy.hubqab `
  -ConfirmCreatesIdentity

# every run
scripts\qa\osl-p2p-loop.ps1 -BundleB org.oslprivacy.hubqab `
  -TempRootB "<the tempRoot the launcher printed>" `
  -ConfirmDriveLiveConversation -OperatorDrivesReceiveSide
```

**Parse-check before invoking anything.** The in-progress marker cannot protect
you from a parse error, because nothing runs at all:

```powershell
$e=$null;[void][System.Management.Automation.Language.Parser]::ParseFile($p,[ref]$null,[ref]$e);$e
```

A dangling `else` has silently killed two runs.

---

## 4. The step vocabulary

`osl-p2p.json` → `steps[]`, each with an `id`, a `status` and a `detail`.

| Step | Claim |
|---|---|
| **P1** | A sends an encrypted message to the peer |
| **P2** | B drains, decrypts and renders it |
| **P3a** | An acknowledgement flowed back from B to A |
| **P3b** | The receipts were *Received* first, then *Opened* |
| **P4a** | A view-once message is listed but not opened (phase 1) |
| **P4b** | It reveals exactly once (phase 2) |
| **P4c** | A second reveal attempt is refused |
| **P5** | A burn issued by A is applied on B, and B acknowledges |
| **P6** | The eye on B paints a row **B did not send** |

### Per-step status

| Status | Means | Green? |
|---|---|---|
| `pass` | measured, and the measurement satisfies the step | **yes** |
| `fail` | measured, and the measurement violates the step | no |
| `unmeasurable` | the measurement was *possible* but did not happen this run — usually no stimulus, or a stale artefact | **never** |
| `blocked` | **no run of this harness against this build can measure it.** Carries `productChangeRequired` with a file:line | **never** |

`unmeasurable` and `blocked` are both non-green and neither is ever counted
toward `passed`. The distinction matters: `unmeasurable` is a rerun, `blocked`
is a code change.

### Overall verdict

| Verdict | Means |
|---|---|
| `pass` | every step is `pass` |
| `fail` | at least one step was **measured** and violated |
| `not-passing` | a mix of `pass` and `unmeasurable`/`blocked`, no measured violation |
| `unmeasurable` | nothing could be measured, but the rig was sane |
| `blocked` | a **precondition gate** failed. The six steps were not run, so **no statement is made about the product**. |

"We could not test" and "the product is broken" are different facts. A `blocked`
run emits one diagnosis plus a remedy and no step rows at all.

`diffKey` is a one-line summary (`P1=pass;P2=unmeasurable;...`) for diffing runs.

### The precondition gates

`consent` → `distinct-bundles` → `instance-a` → `instance-b` → `two-processes` →
`separate-temp-roots` → `qa-profile-a` → `qa-profile-b` → `separate-profiles` →
`mutual-pairing` → `discord-present`.

`separate-profiles` and `mutual-pairing` are the two that matter most:
`separate-profiles` catches the rig where both "identities" resolve to one root,
and `mutual-pairing` proves each side has verified the *other* — both are
checked **before any send**, because "the message never arrived" and "they were
never paired" are different facts and only one is a product defect.

---

## 5. Freshness rules the harness enforces

- **Run start is captured once.** Every file-backed artefact is graded against
  it, with **no fixed staleness window** — a fixed window is what once let a
  4158-second-old receipt be reported as a live failure. An artefact predating
  run start is `unmeasurable` with its age stated: never pass, never fail.
- **Append trails are sliced by byte offset**, not by mtime. `mtime` says
  "fresh" the moment *anything* writes, including the other instance. The
  baseline length of every trail is taken before any step runs.
- **Overwrite output is claimed first.** Each JSON is stamped `in-progress`
  before anything else, so a run that dies cannot leave the previous verdict
  looking current.
- **Executables are identified by sha256, not mtime.** `cargo` hardlinks its
  output, so a rebuild can leave the mtime unchanged.
- Both instances are asserted alive at every step boundary, and Discord's
  process set is compared as a sorted pid list before and after.

---

## 6. What remains unprovable even with two identities

These are `blocked`, not `unmeasurable`. Each names the change that would fix it.

1. ~~**The drain cannot be driven.**~~
   **Superseded 2026-07-26.** The rendezvous no longer drives a single verb. Six
   exist — `status`, `send`, `drain`, `rehydrate`, `reveal-view-once` and the
   attachment verb — enumerated at `apps/osl-hub/src/qa_selftest_request.rs:74-89`,
   and `scripts/qa/osl-p2p-loop.ps1` now drives `drain` on instance B through the
   instance-addressed rendezvous rather than printing "operator, please click".
   That flips a silent drain from `unmeasurable` to a real `fail`.

   The lesson worth keeping is *where* the gap actually was. This entry blamed
   `main.rs` and sent at least one lane looking there; the verbs had existed for
   some time and the stale component was the PowerShell harness. When a
   capability appears missing, check the caller before the callee.

   Two traps recorded while wiring it, both live: the trigger dispatches on the
   first non-whitespace byte being `{`, so a UTF-8 BOM — which PowerShell's
   default `Set-Content` emits — fails that test and falls through to the legacy
   **send** verb, the one with an irreversible side effect. And
   `--features core` does not compile `main.rs`, so only the Windows desktop
   build exercises the dispatch at all.
2. **Received vs Opened is indistinguishable from outside.** The drain batch
   carries the acknowledgements (`broker.rs:3082`) but the QA receipt records
   only a single `acknowledgmentCount`, and the sender-side ledger
   `hub_native_overlay_receipts.json` is AEAD-encrypted at rest
   (`broker.rs:3869`). *Received* (`broker.rs:2834`) and *Opened*
   (`broker.rs:2905`) collapse into one number, and their order is lost. **P3b.**
3. **The second view-once refusal leaves no trace.** It is correct by
   construction — already-consumed at `broker.rs:2872`, `continue` at `:2884`,
   empty batch trips the `Err` at `broker.rs:2536-2542` — but it is returned
   only to the renderer as a string. No file, no counter, no server call. **P4c.**
4. **Bilateral burn is inert — and the drain now destroys the notice.**
   *Corrected 2026-07-26. The previous wording said the drain "never checks
   `is_revocation_bundle`, so an inbound `0x0A` is silently skipped by the
   `continue`." That is stale. The conclusion — burn is inert — is unchanged;
   the mechanism is different and worse.*

   The drain **does** check it now, at `apps/osl-hub/src/broker.rs:2635-2640`:

   ```rust
   if ipc::wire_v2::is_revocation_bundle(&bundle)
       || ipc::wire_v2::is_revocation_ack_bundle(&bundle)
   {
       let _ = client.delete_control_inbox(&identity, &item.id);
       continue;
   }
   ```

   It recognises the revocation frame, **DELETEs it from the control inbox, and
   applies nothing.** The in-source rationale is inbox hygiene: these frames
   "are not accepted by this text/receipt drain", and leaving them would consume
   the keyserver's bounded per-pair capacity. That reasoning is sound for a frame
   nobody will ever process — but no other consumer exists, so the effect is that
   an authenticated burn notice is *consumed and discarded* rather than merely
   passed over.

   Why this is worse than the old skip. A skipped row survives in the inbox, so
   any future drain that learns to apply it recovers the burn. A deleted row is
   gone: the notice cannot be replayed, the recipient never revokes, and the
   sender's UI still shows a burn that was sent. Wiring `apply_peer_revocation`
   later will not heal burns issued in the meantime. The failure is also silent
   at both ends — `delete_control_inbox`'s result is discarded into `let _`.

   Everything else in the original finding still holds, with line anchors
   re-verified 2026-07-26 (the old ones had drifted):

   - `apply_peer_revocation` (`apps/osl-hub/src/security.rs:2431`, was cited as
     `:2124`), `due_revocations` (`:2668`, was `:2366`),
     `record_revocation_attempt` (`:2698`, was `:2396`) and
     `record_revocation_ack` (`:2723`, was `:2421`) still have **zero callers
     outside `security.rs`** — the only call site is `security.rs:2534`, itself
     internal.
   - Burn commands still only *queue*, and nothing ever posts the queue.
   - `is_native_overlay_ack_bundle` is now at `broker.rs:2641` (was `:2620`) and
     `is_native_overlay_relay_bundle` at `:2702` (was `:2676`).
   - Whether migration `0027` is deployed is **`unknown-recheck-required`**, not
     "NOT DEPLOYED". The migration header
     (`keyserver-cf/migrations/0027_control_inbox_revocation_lane.sql:4`) says
     NOT DEPLOYED, but `keyserver-cf/DEPLOY.md:619` records that approval for the
     0026/0027 deploy was given, and remote D1 state cannot be read from this
     checkout. See Conflict C2 in
     `docs/design/osl-completion-plan-2026-07-26.md`.

   **P5 remains blocked.** No rig can fix this; Tab 5 owns the code fix. The fix
   is not simply "stop deleting" — the row must be authenticated and applied via
   `apply_peer_revocation` before any DELETE, on the same
   ledger-write-then-cleanup ordering the acknowledgement branch already uses at
   `broker.rs:2686-2695`.
5. **Inbox eviction is invisible to the client.** `evictOldestPending`
   (`keyserver-cf/src/endpoints/control-inbox.ts:197`) deletes silently: no error,
   no response field, no log. The client's `deferredRows` counts *retryable
   pointer failures*, not evictions. The only observable is an HTTP 429 carrying
   `recipient_inbox_full` once the D1 backstop trips at 512 rows. Proving
   eviction under real traffic would mean either reading D1 directly or having
   the worker report an eviction count.
6. **Orientation is discarded in the renderer.** The decoder proves both
   `PeerToSelf` and `SelfToPeer` (`broker.rs:1788-1791`), but
   `apps/osl-hub-ui/src/overlay.ts:688-689` stamps `direction: "incoming"` and
   `author: verifiedFriendIdentity` on **every** decoded row. So the eye cannot
   tell you *from the row* whether OSL sent it. P6 works around this by proving
   B sent nothing during the run (outbound receipt sha256 unchanged **and** the
   send-stage trail did not grow), which makes every decoded row on B a row B did
   not send **by construction** — but the product still cannot label it.
7. **A production binary is unobservable.** Every artefact except
   `osl-startup-trace.txt` is behind `--features discord-qa-shell`, and the eye's
   `visibleCarrierRows` field does not exist without it. This rig measures the QA
   shell, not the shipping build.

---

## 7. Resetting between runs

Between ordinary runs — nothing. The harness re-baselines every trail at run
start and grades only what was written afterwards, so leftovers cannot be graded.

To reset **instance B's identity** (a fresh unpaired identity):

1. Close instance B. Confirm it is gone — no `<id>-sic` marker window.
2. Delete `%APPDATA%\<B-identifier>` and `%LOCALAPPDATA%\<B-identifier>`.
3. Delete B's temp root (the launcher prints it; default
   `%LOCALAPPDATA%\Temp\osl-qa-b-<identifier>`).
4. Start B once via the launcher. It creates a new identity and **registers it
   on the live keyserver** — that is a real write, and the old registration is
   left behind.
5. Close both instances, re-run `osl-p2p-pair.ps1`, restart both.

To reset the **artefact surface only** (keep both identities):

- Delete `osl-qa-selftest.json`, `osl-qa-selftest.request`,
  `osl-discord-qa-*.txt` from *both* temp roots.
- Delete `discord-qa-*-receipt.json` from both profile roots. Do **not** delete
  `discord-qa-offer.v1.json`, `discord-qa-peer-offer.v1.json`,
  `discord-qa-pairing-status.v1.json` or `discord-qa-device-key.v1.json` — that
  un-pairs the rig, and deleting the device key makes the identity unopenable.

**Never** delete anything under instance A's profile root as part of a reset.

---

## 8. Safety invariants these scripts hold

- **No pointer input and no keyboard injection anywhere.** There is no
  `SendInput`, `keybd_event`, `mouse_event` or `SetCursorPos` in any script here.
  The only actuator the Win32 layer exposes is `PostMessage`, and where a click
  is ever needed it goes to `Chrome_RenderWidgetHostHWND` after
  `WindowFromPoint`→`GA_ROOT` has confirmed ownership. The current harness needs
  none: it drives files and reads files.
- **Instance A is never touched.** Its hwnds and pid are captured before the
  launch and excluded from every operation, including the window move. Its
  identity file is compared by sha256 across the launch and a change is a `fail`.
- **Discord's process set is asserted unchanged** across every run, as a sorted
  pid list so a same-count restart is still caught. Nothing here starts, stops,
  signals or closes a Discord process.
- **No plaintext, cover text, key material or conversation name is ever logged.**
  Only counts, fixed labels, hashes, booleans and opaque IDs are read out of
  artefacts, and only fixed strings are written into them.
