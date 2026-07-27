# Azure VM QA workflow

Every OSL test that opens a window runs here, not on the owner's desktop.

**Why this exists.** A virtual monitor is visual separation only. Windows scopes the input queue and
the foreground window to a *desktop object*, not a monitor, so a process on `DISPLAY5` can still call
`SetForegroundWindow` and take the owner's screen mid-sentence — which it has, twice. Master §12.4 is
explicit: a VM is required for a genuinely separate focus, cursor, desktop and host-app session. A VM
is the only arrangement where "it cannot touch his screen" is structurally true rather than
best-effort.

`CreateDesktop` is the one native primitive that would give a real second input queue, and it is
deliberately **not** used: a non-visible desktop is capture-only, `PrintWindow` returns black on
Chromium (`osl-visual-verification-traps`), and OSL's whole visual claim is the eye painting over
Discord rows. It trades focus theft for unverifiable tests.

## The fleet

All **ten** are Windows and start **deallocated** — disk cost only, no compute. (This table said
eight until 2026-07-26 and omitted the Signal pair entirely; a fleet doc that under-reports the
fleet is how a VM gets left running.)

| Resource group | VMs | Size | Use for | Lane |
|---|---|---|---|---|
| `OSL-TWO-CLIENT-LAB` | `OSL-Azure-Client-1`, `-2` | `D2s_v3` | Two-identity P2P (B6). The A/B pair. | crypto |
| `OSL-TWO-CLIENT-LAB-INDEPENDENT` | `OSL-Independent-Client-1`, `-2` | `B2as_v2` | Second pair when the first is mid-run | scrub |
| `OSL-WHATSAPP-TWO-CLIENT-LAB` | `OSL-WhatsApp-Client-1` (`D2s_v4`), `-2` (`D2s_v3`) | mixed | WhatsApp adapter | — |
| `OSL-TELEGRAM-QA` | `OSL-Telegram-QA-1`, `-2` | `D2s_v3` | Telegram viability | — |
| `OSL-SIGNAL-QA-SCUS` | `OSL-Signal-Client-1`, `-2` | `B2als_v2` | Signal adapter | — |

All ten carry a **system-assigned managed identity** and read secrets from
`osl-test-secrets-a7d5d9` over IMDS. That is the only credential path: no key, password or SAS
token is ever passed to a VM from the host, and none may appear in a prompt, report or event.

### Lane ownership, so two lanes do not build the same rig twice

| Thing | Owner |
|---|---|
| `scripts/vmqa/**`, the Azure Files share, the in-VM agent | vmqa |
| Setup + WARM snapshot of `OSL-Azure-Client-1`/`-2` | vmqa |
| Setup + WARM snapshot of `OSL-Independent-Client-1`/`-2`, scrub's host driver | scrub |
| The COLD release-gate lineage | release |
| App-side QA verbs in `qa_selftest_request.rs` | crypto |

`scripts/qa/**` belongs to the other lanes. vmqa does not edit `apps/**` or `crates/**`; an
app-side change it needs is written into its report as an exact diff and routed.

Subscription is **Azure for Students** — a fixed credit pool, not a billing account. Deallocate when
you finish. A running `D2s_v3` you forgot about is the only way this workflow costs real money.

```bash
az vm start      -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1   # ~30-60 s
az vm deallocate -g OSL-TWO-CLIENT-LAB -n OSL-Azure-Client-1   # do this when done
az vm list -d --query "[?powerState=='VM running'].name" -o tsv # leak check
```

If `az` reports an expired refresh token, re-auth is a human step:
`az login --use-device-code --tenant 79981b01-1944-4da0-aa9a-fb9f63bddb5e`.

## The four rules that make this fast

**1. Build on the host. Never on the VM.** These are 2-vCPU boxes; a Rust build there costs more than
the whole rest of the loop. Cross-compile in WSL and ship the artifact.

**2. Set up once, then snapshot.** Discord installed and signed in, WebView2 runtime present, the OSL
identity created, the agent registered as a logon task. Snapshot the disk after that. Every later run
starts from known-good instead of re-doing setup, which is where VM QA usually dies.

### Two lineages, and they must never be confused

There is no single "the snapshot". There are two, they answer different questions, and a run graded
against the wrong one is worthless:

| Lineage | Contains | Answers | Use for |
|---|---|---|---|
| **WARM** | Discord signed in, OSL identity created, agent registered | "does this build behave?" | fast iteration |
| **COLD** | a clean Windows box, nothing installed | "does this install and run on a machine that has never seen OSL?" | the release gate |

A WARM snapshot cannot satisfy a release gate — it has already been taught everything the gate is
trying to discover, so a first-run installer defect is invisible on it. A COLD snapshot cannot
support fast iteration; you would re-do setup every run.

**Naming makes the mistake unexpressible**, because a comment would not:

```
<vm>-WARM-<purpose>-<yyyymmdd>      OSL-Azure-Client-1-WARM-iteration-20260726
<vm>-COLD-<purpose>-<yyyymmdd>      OSL-Azure-Client-1-COLD-release-gate-20260726
```

Also tag `lineage=warm-iteration` / `lineage=cold-release-gate`. Any snapshot whose name carries
neither `-WARM-` nor `-COLD-` is untrusted: delete it and retake, do not guess what is on it.

State as of 2026-07-26: the subscription held **zero** snapshots until tonight; rule 2 had never
actually happened for anyone. The first is scrub's
`OSL-Independent-Client-1-WARM-iteration-20260726`.

**Trap, already paid for: pin `--location` explicitly.** Azure region policy refuses a snapshot that
inherits a default region rather than the source disk's. Read each disk's own location and pass it —
do not assume a fleet-wide value (`northcentralus` is right for the Independent pair, not
necessarily for yours). Snapshot only a **deallocated** VM; a snapshot of a running Windows box is
crash-consistent at best.

**Restoring is a disk swap, not a button.** Create a managed disk from the snapshot, then
`az vm update --os-disk <newDiskId>` on the deallocated VM. Keep the old disk until the restore is
proven; deleting it automatically is unrecoverable. Retake a lineage after any setup change — a
snapshot that silently drifts from what it claims to contain is worse than none.

**3. Drive it by file rendezvous, never by RDP.** OSL already has this pattern
(`osl-qa-selftest.request` → `osl-qa-selftest.json`). Put that directory on an Azure Files share
mounted on both sides. The host writes a request; the agent already running inside the VM's
interactive session executes it and writes a verdict; the host reads it back. No RDP session to hold
open and nothing to click by hand — the agent does the clicking, on the VM, where that is allowed.

**4. Ship the binary only when its hash changed.** `cargo` hardlinks its output, so mtime is a lie —
identify builds by `sha256` (`osl-build-and-test-gotchas`). Copying a 40 MB exe you already sent is
pure latency.

## Why an in-VM agent and not `az vm run-command`

`az vm run-command invoke` is the obvious answer and it is wrong here. It executes as SYSTEM in
session 0, where nothing renders. OSL's eye has to actually paint over a real Discord window to be
verifiable, so the work must happen in a logged-in interactive session. `run-command` is still the
right tool for orchestration that needs no GUI: checking the agent is alive, reading a verdict,
collecting a log.

## The loop

Target is under three minutes per iteration once setup is snapshotted.

1. **Host** — `npm run build` first. The frontend `dist` is embedded at *compile* time and there are
   **two** embeds (`webview/dist` as well), so a Cargo build before the frontend build ships a stale
   UI that looks like a code bug. Then cross-compile:
   `cargo build --features desktop --bin osl-privacy-hub --target x86_64-pc-windows-gnu`.
2. **Host** — stamp `sha256` of the exe and the dist, plus branch, HEAD and dirty fingerprint. Copy
   to the share only if the hash moved.
3. **Host** — write `run.request` naming the run id and the verbs
   (`status` / `send` / `drain` / `rehydrate` / `reveal-view-once`).
4. **VM agent** — stages the exe **next to `WebView2Loader.dll`**, launches, drives the verbs, writes
   `run.verdict.json` plus screenshots to the share.
5. **Host** — polls for the verdict, grades every artifact against run start, pulls the evidence into
   `docs/reports/`.

## Traps that have already cost time

- **`WebView2Loader.dll` must sit beside the exe.** Without it the process hangs before `main` with
  no trace file at all and looks exactly like a corrupt build (`osl-staging-webview2loader`).
- **WSL does not forward environment variables to Windows processes.** Use `WSLENV`, or the test
  panics claiming its inputs are unset.
- **`--features core` does not compile `main.rs`.** The bin is gated behind
  `required-features = ["desktop"]`, so a green core run proves nothing about the binary.
- **The osl-hub bin cannot build on Linux** (`rfd` backend). Windows target only.
- **Reject artifacts older than run start.** A stale receipt that survives a restart will happily
  impersonate the current run; grade it `unmeasurable`, never pass or fail.
- **Identify instances by the single-instance marker window class `<identifier>-sic`**, never by
  title — every OSL build is titled `OSL Privacy` and that has already graded a stale instance.

## Input injection: where it is banned and where it is required

The PostMessage-only rule exists because the owner is sitting at his desktop and `SendInput` steals
his cursor and keystrokes mid-sentence. That rationale does not survive the trip to a VM, and
applying it there would ban the click-through testing the VM exists to make possible.

**On the owner's desktop — banned, absolutely.** No `SendInput`, `keybd_event`, `mouse_event` or
`SetCursorPos`. `PostMessage` only, and only after `WindowFromPoint`→`GA_ROOT` has confirmed the
window belongs to your own staged build. Window *placement* (`SetWindowPos` with `SWP_NOACTIVATE`)
is not injection and stays allowed.

**On an isolated VM — allowed, and expected.** `SendInput`, `mouse_event`, `SetCursorPos` and real
keyboard driving are all fine. Nobody is sitting at that desktop, the machine is disposable, and a
consent flow that must actually be clicked cannot be proven any other way. A harness that refuses to
click on a VM is not being careful, it is being useless.

What stays forbidden on the VM, because it is about consequence rather than focus:

- No real personal account. Disposable test identities and test conversations only.
- No real user data — no live mailbox, no production profile, no unseeded browser tree.
- Nothing that reaches back to the host: no writes outside the share, no host process control.
- Never a destructive action against a target you did not seed yourself.

**Identify the target by the single-instance marker window class `<identifier>-sic`, never by title
and never by "first process with a window".** Every OSL build is titled `OSL Privacy`. Selecting by
name has already driven the wrong lane's application through six UI steps and graded a stale
instance. A harness that guesses its subject confirms whatever it happened to find — the same defect
family as a default-deny assertion that passes because it read nothing.

## Safety, unchanged


Input-injection rules are in the section above and depend on where you are running.
Instance A's identity file `sha256` must be unchanged across a run or the run is a fail. Discord's
process set is compared as a sorted pid list. No plaintext, cover text, key material or conversation
name in any artifact. Credentials come from Key Vault just in time on the VM
(`docs/testing/test-account-secrets.md`) and never enter a prompt, report, event or log.
