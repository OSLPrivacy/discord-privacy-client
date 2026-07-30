# Cross-VM pairing (B6): running the offer/response exchange over blob

Status as of this writing: **code and spec only. No Azure VM was started to
produce this document, and no run of `osl-p2p-pair-blob.ps1` has ever
executed.** Everything below the line "What remains unproven" is a claim
about what the code is designed to do, not a measurement of what it did.

This complements, and does not replace,
[`docs/qa/two-identity-p2p-verification.md`](./two-identity-p2p-verification.md)
(same-host, one filesystem, `osl-p2p-pair.ps1`) and
[`docs/testing/azure-vm-qa-workflow.md`](../testing/azure-vm-qa-workflow.md)
(the fleet's general blob-rendezvous design). Read those first if the "why"
behind a given check here isn't obvious — this doc only covers what's new for
the two-VM case.

## Why a new script instead of reusing `osl-p2p-pair.ps1`

`osl-p2p-pair.ps1` cross-installs two identities' offers with `Copy-Item`.
That only works because both identities' `%APPDATA%` roots are visible to one
PowerShell process. The crypto pair (`OSL-Azure-Client-1` /
`OSL-Azure-Client-2`, resource group `OSL-TWO-CLIENT-LAB`, per
`scripts/vmqa/SETUP-CRYPTO-PAIR.md`) is two separate VMs with two separate
filesystems and no shared drive (shared-key auth being off rules out an SMB
mount, same reasoning as `azure-vm-qa-workflow.md` gives for why blob and not
a file share is the rendezvous at all). `osl-p2p-pair-blob.ps1`
(`scripts/qa/osl-p2p-pair-blob.ps1`) does the one new thing this needs — move
bytes between the two machines — and nothing else. It calls the same
`osl-p2p-win32.ps1` for the "is this instance running" check and produces the
same step/verdict JSON shape as its sibling.

## The sequence

Both VMs must have already been started once (each writes its own
`discord-qa-offer.v1.json` on first boot — the product does that, not this
script) and both must be **closed** before either command below runs, for the
same reason `osl-p2p-pair.ps1` insists on it: the peer offer is only consumed
at startup (`publish_and_consume_pairing`,
`apps/osl-hub/src/main.rs:5913` → `discord_qa_identity.rs:261`), so writing it
under a live process is a silent no-op until some later, unpredictable
restart.

```
On OSL-Azure-Client-1 (identity "A"):
    scripts\qa\osl-p2p-pair-blob.ps1 -Side A -Push -RunId exchange

On OSL-Azure-Client-2 (identity "B"):
    scripts\qa\osl-p2p-pair-blob.ps1 -Side B -Pull -PeerSide A -RunId exchange
    scripts\qa\osl-p2p-pair-blob.ps1 -Side B -Push -RunId exchange

On OSL-Azure-Client-1 (identity "A"):
    scripts\qa\osl-p2p-pair-blob.ps1 -Side A -Pull -PeerSide B -RunId exchange

Then, on both VMs:
    start the instance (consumes the peer offer, writes
    discord-qa-pairing-status.v1.json if it verifies)
    scripts\qa\osl-p2p-loop.ps1  (or whatever same-VM harness step follows,
    per two-identity-p2p-verification.md's mutual-pairing gate)
```

Four commands total, two per VM, because the exchange is one-directional per
call by design (mirrors `-Push`/`-Pull` explicitly rather than a single
"sync" verb, so each side's operator can see exactly what left and what
arrived). `-RunId` defaults to `exchange`, a single well-known slot, since the
fleet runs one crypto pair at a time; pass an explicit different value to
start over cleanly without deleting the prior attempt's blobs.

## Where the bytes live

```
vmqa/pairing/<RunId>/A-offer.json
vmqa/pairing/<RunId>/A-offer.json.ready   (ASCII sha256 of the payload above)
vmqa/pairing/<RunId>/B-offer.json
vmqa/pairing/<RunId>/B-offer.json.ready
```

Same container (`vmqa`) and same storage account
(`osltestartifactsa7d5`) the rest of `scripts/vmqa/**` already uses — no new
cloud dependency, per the task constraint. The `put-atomic`/`get-atomic`/
`.ready`-sentinel pattern is copied from `scripts/vmqa/vmqa-share.sh` (host
side) and `scripts/vmqa/vmqa-agent.ps1` (VM side): payload written first,
`.ready` sentinel (containing the payload's own sha256) written second, and a
reader treats "no `.ready`" as "not there yet," never touching a
possibly-half-written payload.

## What authorizes a blob write — read this, it's the loud finding the task asked for

Shared-key auth is off on this storage account
(`azure-vm-qa-workflow.md`, "Drive it by blob rendezvous, never by RDP"), so
there is no account key or SAS in play anywhere in this script — none is
generated, stored, or logged. What actually authorizes each write is **Entra
ID RBAC**: each crypto VM carries a system-assigned managed identity;
`osl-p2p-pair-blob.ps1` exchanges that identity for an OAuth token over IMDS
(`http://169.254.169.254/...`, link-local, not reachable off the VM) and
presents it as a bearer token on every blob REST call — the identical
mechanism already proven in `vmqa-agent.ps1`'s `Get-StorageToken` /
`Invoke-BlobRequest`, duplicated here in miniature rather than dot-sourced
(that file is a daemon with its own `param()` block and main loop, not a
library).

The role granted is `Storage Blob Data Contributor`, and per
`azure-vm-qa-workflow.md:118-119` it is scoped to the **whole `vmqa`
container**, not to a path prefix inside it. Azure RBAC role assignments
don't narrow to a blob-name prefix without an explicit ABAC condition, and
none is documented for this fleet. **This means either crypto VM's identity
can read or write any path under `vmqa/` — `runs/`, `builds/`, `agent/`, and
both sides of `pairing/` — not just the one it "should" touch.** Nothing in
Azure stops Client-1 from overwriting the object Client-2 is about to push,
or reading a path that "belongs" to the peer by convention only.

So the honest answer is not "nothing authorizes a write" — something does,
and it's real (Entra RBAC, no static secret, unspoofable-off-Azure IMDS) —
but it is coarser than the path-naming convention suggests, and it is
**identity-of-the-writer** authorization, not **content** authorization. That
gap is exactly why the transport must never be trusted for what it carries:
a VM with write access to `pairing/` cannot be stopped by Azure from writing
a garbage or adversarial offer to either side's slot. The receiving side's
`-Pull` verifies transport integrity (downloaded bytes match the `.ready`
sentinel's sha256) which proves the copy wasn't torn — it proves nothing
about whether the offer is a real, honestly-derived identity. That check is
`publish_and_consume_pairing`'s alone, at the next start, on the exact same
code path it already uses for a same-host `Copy-Item`-delivered offer. The
blob transport is not, and must never become, an authority path.

## How replay and staleness are refused

The product's own artifact is the ground truth, not a sidecar file this
script invents. `discord-qa-pairing-status.v1.json`
(`discord_qa_identity.rs:54-61`) records `peer_offer_sha256` — the sha256 of
the exact peer-offer bytes the product consumed and verified. `-Pull`:

1. Downloads the peer's payload, verifies it against the `.ready` sentinel's
   sha256 (transport integrity — proves the copy is intact, not that the
   offer is genuine).
2. If a local `discord-qa-pairing-status.v1.json` already exists **and**
   `verified == true` **and** its `peer_offer_sha256` equals the just-downloaded
   payload's sha256 → **refused**. This exact offer was already consumed and
   the resulting pairing already verified; the script exits non-zero and
   writes nothing. A re-run cannot silently re-pair against a stale offer.
3. If a local `discord-qa-peer-offer.v1.json` already exists with the same
   sha256 but no verified pairing yet → treated as an idempotent no-op (already
   installed, nothing to do), reported distinctly from a fresh install so the
   operator isn't misled into thinking a new pull happened.
4. Otherwise (new sha256 — e.g. the peer regenerated its identity after a
   reset) → proceeds and installs, and the receipt says so.

This is why a re-run is restart-safe: it can't double-pair (case 2 refuses),
and it can't silently reuse a stale offer under a different guise (the check
is keyed on content hash, not on file mtime or presence alone).

## Other refusal gates (same shape as `osl-p2p-pair.ps1`)

- **Not a fleet VM**: IMDS instance-name lookup against the closed VM
  allow-list; if IMDS doesn't answer or the name isn't listed, refuses before
  attempting any blob call.
- **Instance still running**: `Get-P2PBundleMap` (via `osl-p2p-win32.ps1`,
  the `<bundle-id>-sic` marker-window mechanism) — refuses `-Push` and
  `-Pull` alike while the local instance is up. This script has no way to see
  whether the *peer's* instance is closed on the other VM; that stays the
  operator's job (see "what remains unproven" below).
- **`-Pull` without `-PeerSide`**, or **`-PeerSide` equal to `-Side`**:
  refused before any network call — the latter catches the real footgun of
  pulling your own just-pushed offer back as if it were the peer's.
- **Same `osl_user_id` on both sides**: refused (failed, not blocked) — two
  offers with the same opaque ID means one identity was cloned onto both
  VMs, not two real identities to pair.
- **Local offer missing or unreadable**: refused with a pointer to start the
  instance once so the bootstrap writes it.

No friend code or safety number is ever read, compared, or logged by this
script — only the opaque `osl_user_id` and sha256 hashes, matching the
invariant already followed by `osl-p2p-pair.ps1` and
`two-identity-p2p-verification.md` §8.

## Deviation from the runbook worth flagging explicitly

`two-identity-proof-runbook.md` §3 sketches the VM-side write as
`az storage blob upload --auth-mode login`. That's the **host** pattern
(`scripts/vmqa/vmqa-share.sh`, run from WSL under the operator's own
interactive `az login`). It doesn't apply on the crypto VMs: nothing
provisions or configures `az` CLI login there, and the entire point of the
fleet's IMDS design is that the VM side needs no interactive credential at
all. `osl-p2p-pair-blob.ps1` therefore uses IMDS + raw REST
(`vmqa-agent.ps1`'s proven pattern), not `az`. This is called out in the
script's own header comment as well, so it isn't a silent deviation from the
brief.

## What remains unproven pending a real two-VM run

Nothing here has executed. Specifically unverified:

- **PowerShell syntax/parse correctness.** No `pwsh` is available in this
  Linux/WSL work environment, so
  `[System.Management.Automation.Language.Parser]::ParseFile` (the check
  `two-identity-p2p-verification.md` recommends) has not been run against
  `osl-p2p-pair-blob.ps1`. The script was hand-written by close pattern match
  against two already-proven scripts (`osl-p2p-pair.ps1`,
  `vmqa-agent.ps1`), but a typo or type error would not be caught until it
  runs on Windows.
- **The IMDS token fetch and blob REST calls actually succeeding** from
  `OSL-Azure-Client-1`/`-2` specifically — `vmqa-agent.ps1` proves the
  pattern works from *some* fleet VM in *some* other context, not that this
  script's particular duplication of it is bug-free.
  `Get-HttpStatusCode`'s reflection-based status-code extraction (written to
  tolerate either Windows PowerShell 5.1's `WebException` or PowerShell 7's
  `HttpResponseException` shape) is the single riskiest piece of unverified
  code in the script — if it doesn't extract a 404 correctly, `-Pull`'s
  polling loop degrades to treating every error as a hard failure or every
  failure as "not ready yet," and only a real run would surface which.
- **RBAC actually reaching these two VMs' identities as documented.**
  `azure-vm-qa-workflow.md` and `SETUP-CRYPTO-PAIR.md` say `Storage Blob Data
  Contributor` is granted on the `vmqa` container to both crypto VMs — this
  was read, not re-verified against the live role assignments, and no `az
  role assignment list` was run (that alone would have required Azure
  access this task was explicitly told not to use).
- **The full four-command sequence end to end**, including whether operator
  timing (closing an instance, running `-Push`, switching RDP sessions,
  running `-Pull` before the other side's `.ready` sentinel exists) behaves
  as the `-TimeoutSec` polling loop intends.
- **Whether `publish_and_consume_pairing` actually accepts an offer that
  arrived via this path.** The design claim is that consumption doesn't care
  how the bytes arrived — same file, same schema, same
  `#[serde(deny_unknown_fields)]` struct, same verification code. That claim
  was checked by reading `discord_qa_identity.rs`'s struct definitions, not
  by producing a real `discord-qa-pairing-status.v1.json` with
  `verified: true` from a blob-delivered offer.
- **The staleness-refusal path itself**, i.e. that case 2 above actually
  fires on a genuine second `-Pull` attempt after a real successful pairing,
  has not been exercised.

A real two-VM run — start both instances once, run the four-command
sequence, restart both, and check for `verified: true` on both sides' — is
the only thing that would close these gaps, and per this task's explicit
constraint, no VM was started to do that here.
