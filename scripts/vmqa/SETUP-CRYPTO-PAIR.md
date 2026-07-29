# Setting up the crypto pair — `OSL-Azure-Client-1` / `-2`

Brings the two-identity P2P pair from deallocated to a snapshotted known-good state, so the
two-identity proof stops re-doing setup on every run.

Facts you need, verified 2026-07-26:

| | |
|---|---|
| Resource group | `OSL-TWO-CLIENT-LAB` |
| Region | **`centralus`** — *not* `northcentralus`; that is the Independent pair's region |
| OS disks | `osl-client-1-osdisk`, `osl-client-2-osdisk` |
| Admin / interactive user | `osltest` |
| Size | `Standard_D2s_v3` |
| Vault | `osl-test-secrets-a7d5d9`, same resource group |

Pin `--location centralus` on every snapshot here. Azure region policy refuses a snapshot that
inherits a default region instead of the source disk's, and the correct value differs per fleet.

**Verifying the fleet discipline above without spending anything:**
`scripts/vmqa/test-vmqa-fleet.sh` drives `vmqa-fleet.sh`'s real `fleet_rg`, `expand_target`,
`cmd_snapshot`, `cmd_restore`, `cmd_stop_all` and `cmd_leak_check` against a stubbed `az`
(`scripts/vmqa/fixtures/fake-az/az`) that costs nothing and touches no subscription. It asserts,
per refusal path (unknown VM, malformed `WARM-`/`COLD-` label, VM not deallocated, missing
`--yes`/`--yes-destroy-current-disk`), that **zero** `az` calls were made — not just that the exit
code was non-zero — and separately proves the crypto pair's real success path pins
`--location centralus` and tags `lineage=warm-iteration`/`lineage=cold-release-gate` correctly.
This is regression coverage the lifecycle script did not have before (the prior lane's own report,
`docs/reports/vmqa-lane-2026-07-26.md`, lists `vmqa-fleet.sh` as proven only by manual live runs).
It does **not** prove anything about a real VM, snapshot, or identity — that stays `unproven` here
by design; run it with `bash scripts/vmqa/test-vmqa-fleet.sh`.

**A gap this inventory surfaced, not fixed here:** step 9 below says "pair them per
`docs/qa/two-identity-p2p-verification.md` §3", but that section's pairing tool
(`osl-p2p-pair.ps1`, itself not present in `scripts/qa/` under that name — see
`scripts/qa/osl-p2p-loop.ps1:677`) copies `discord-qa-offer.v1.json` between two profiles **on one
machine**. On this fleet the two identities live on two separate VMs, so nothing here moves an
offer file from `OSL-Azure-Client-1` to `OSL-Azure-Client-2` or back. Closing that is out of this
lane's file ownership (`scripts/qa/**`) and out of scope for a unit that may not create identities;
it is recorded here so the next take does not assume §3 already covers the cross-VM case.

---

## Two WARM tiers, split at the consent boundary

Setup is deliberately cut into two snapshots rather than one:

| Tier | Contains | Production side effect |
|---|---|---|
| `WARM-agent` | WebView2 runtime, Discord signed in, agent registered as a logon task | **none** |
| `WARM-identity` | the above **plus** an OSL identity | **writes to the live keyserver** |

Creating an OSL identity is not a local act. A `discord-qa-shell` build creates a device-bound
identity and then blocks up to 30 s waiting for it to be **registered on the keyserver**
(`discord_qa_identity.rs:30,134`) — a real write to production, against the same keyserver serving
the owner's real identity. The registration is also not cleanly reversible: resetting an identity
leaves the old registration behind.

So `WARM-agent` is built without asking anyone, and `WARM-identity` is a one-step finish that
requires the crypto lane's explicit go-ahead. Do not collapse them. A single snapshot would force
every future re-take to redo a production write.

---

## Sequence

Steps marked **HUMAN** need an interactive login or 2FA and cannot be automated away.

### 1. Start the VM

```bash
scripts/vmqa/vmqa-fleet.sh start OSL-Azure-Client-1
```

### 2. **HUMAN** — first interactive logon

The agent runs as a **logon task in the interactive session**, so a session must exist. RDP in once
as `osltest`; the password is `azure-client-1-admin-password` in the vault.

Retrieve it in your RDP client or on the VM. It must not be pasted into a prompt, a report, an
event, a commit message or Telegram, and no lane's transcript may ever contain its value.

### 3. WebView2 runtime

Install the Evergreen runtime. Without it the hub cannot render at all.

Separately, and this is the trap that costs the most time: `WebView2Loader.dll` must sit **beside
the staged exe**. Without it the process hangs before `main` with no trace file whatsoever and looks
exactly like a corrupt build. The `stage` verb copies both and verifies both hashes; do not
hand-stage an exe on its own.

### 4. Discord, signed in to a disposable account

The Discord test accounts already exist in `osl-test-secrets-a7d5d9` as
`osl-test-discord-01`, `osl-test-discord-02` and `osl-test-discord-03`. Read
`osl-test-account-manifest` first; it is the authority for which account belongs where. Then
retrieve the chosen credential **just in time on the VM**. Never a real personal account, and never
put a credential value into a prompt, a report, an event, a commit message or Telegram. Sign in,
dismiss first-run dialogs, and leave it on the QA conversation so a run does not start with a modal
in the way.

### 5. Install the agent

Copy `scripts/vmqa/vmqa-agent.ps1`, `vmqa-agent-bootstrap.ps1` and `vmqa-win32.ps1` to the VM, then
from an **elevated** PowerShell:

```powershell
.\vmqa-agent-bootstrap.ps1
```

It refuses to run on any machine whose name does not start with `OSL-`. That check is the whole
safety story for a module that synthesises real mouse and keyboard input, so there is no override
switch — if it refuses, you are on the wrong machine.

### 6. Verify the agent from the host, without RDP

```bash
scripts/vmqa/vmqa-run.sh agent-alive --vm OSL-Azure-Client-1
```

This must report `isInteractiveSession: true`. A heartbeat from session 0 is worthless: nothing
renders there, so the eye cannot paint and every visual verdict would be vacuous. That is also why
`az vm run-command` is not the transport — it runs as SYSTEM in session 0. Use it only for
GUI-free orchestration: is the agent alive, fetch a verdict, pull a log.

### 7. Log out, deallocate, snapshot `WARM-agent`

Snapshot only a deallocated VM; a snapshot of a running Windows box is crash-consistent at best.

```bash
scripts/vmqa/vmqa-fleet.sh stop     OSL-Azure-Client-1
scripts/vmqa/vmqa-fleet.sh snapshot OSL-Azure-Client-1 WARM-agent
```

Expected name: `OSL-Azure-Client-1-WARM-agent-<yyyymmddHHmm>`, tagged `lineage=warm-iteration`.

### 8. Repeat for `OSL-Azure-Client-2`

Use the account that `osl-test-account-manifest` assigns to client 2; do not infer it from secret
numbering. Everything else identical.

### 9. `WARM-identity` — **requires crypto lane go-ahead**

Only after step 8, and only once someone has decided a second OSL identity may exist: start both,
create the identity on each, pair them per `docs/qa/two-identity-p2p-verification.md` §3, close
both, then snapshot each as `WARM-identity`.

Never delete anything under instance A's profile root as part of a reset.

---

## Restoring, and re-taking after a change

Restore is a disk swap, not a button:

```bash
scripts/vmqa/vmqa-fleet.sh restore OSL-Azure-Client-1 <snapshot-name> --yes-destroy-current-disk
```

It creates a managed disk from the snapshot, swaps it in on the deallocated VM, and prints the name
of the **old** disk without deleting it. Delete that yourself once the restore is proven — an
automatic delete there is unrecoverable.

Re-take a tier after any setup change: a Discord update, a WebView2 update, an agent edit. A
snapshot that has silently drifted from what its name claims is worse than no snapshot, because
runs graded against it look clean. The agent records its own sha256 in every heartbeat precisely so
this drift is visible from the host.

## When you are done

```bash
scripts/vmqa/vmqa-fleet.sh leak-check
```

Exits non-zero if anything in the subscription is still running. Azure for Students is a fixed
credit pool, and a forgotten `D2s_v3` is the only way this workflow costs real money.
