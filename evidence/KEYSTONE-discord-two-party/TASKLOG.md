# KEYSTONE — the live two-party Discord carrier proof. **STILL NOT PRODUCED.**
# Blocker A is CLOSED. A new blocker, A′, replaced it, and it needs ~4 minutes of owner time.

**Branch `proof/keystone-discord-two-party`, worktree `/home/liamw/osl-keystone2`, from
`integrate/first-usable @ 2646513c2`. No product code was changed — the branch adds evidence and this
log only. Nothing was typed into any Discord composer. No message was sent. The owner's machine was
never touched.** Evidence: `evidence/KEYSTONE-discord-two-party/`.

**Run date 2026-08-05, on the Azure QA VMs `osl-client-1` and `osl-client-2` (RG `OSL-VMQA`,
spaincentral). Neither VM was shut down.**

---

## 0 · The verdict in one paragraph

**None of the four clauses ran.** The cover was not placed, nothing was posted, no peer decoded, and no
peer's Discord was looked at — because **neither Discord client could be signed in.** D-233's blocker A
("there is no second Discord party") is genuinely closed: the burner accounts exist, both sets of
credentials are **correct**, and the login form accepted them. What stopped the run is one layer later:
**Discord serves an image-based hCaptcha to both accounts from both Azure public IPs, and the "I am
human" checkbox escalates to a picture challenge rather than passing.** Reproduced on two VMs, two
accounts, two IPs, four attempts. **Solving that challenge programmatically would be circumventing an
anti-automation control, so the lane refused it and stopped.** The unblock is an owner RDP session of
about two minutes per VM; RDP is already open on both.

**Independently of the accounts, a second structural blocker was verified at source and it is new:
the keystone's artifact list is unsatisfiable by any single run.** See §3. It has to be re-specified as
two runs before anyone tries again, or the next lane will burn a day discovering it the hard way.

---

## 1 · Blocker A is CLOSED — the accounts work, and this is worth recording precisely

`/tmp/osl-creds.csv` carries three Discord rows. Two were used, referred to here by username or row
index only, per the credential rule; **no credential value appears in this log, in the evidence, in any
screenshot, or in any commit.**

| VM | account | outcome |
|---|---|---|
| `osl-client-1` | **`oslpriv1`** | credentials accepted by the form, **hCaptcha image challenge** |
| `osl-client-2` | row 34 (the row whose note records a changed password) | credentials accepted, **hCaptcha drag challenge** |

Row 19 (the not-phone-verified row) was deliberately left alone, per the brief.

**The credentials are demonstrably right.** The login form's own read-back proves the values arrived
intact and the button was live:

```
READBACK email length=25 (expected 25)
READBACK password length=29 (expected 29)
INVOKE Log In -> Button name="Log In" ... en=True pat=Invoke,ScrollItem
INVOKE-OK
```

and the same on `osl-client-2` at lengths 20/29. **`VERDICT=CAPTCHA` on both**, from the tree, not from a
screenshot. Nothing about this is a credential problem, an e-mail-verification problem or a
phone-verification problem — none of those screens was ever reached, so **SMSPool and the mailbox rows
were never needed and were not touched.**

## 2 · Blocker A′ — hCaptcha, and why the lane stopped rather than solved it

Discord's login is behind hCaptcha. From an Azure datacentre IP the anchor checkbox does **not** pass:
clicking it with a **real** `SendInput` mouse event (not a UIA `Invoke` — the click was a genuine input
event, at the checkbox's own bounding rectangle) escalates to a picture challenge every time.

The tree names it itself:

```
Window     name="Wait! Are you human?"
CheckBox   name="'I am human', Select in order to trigger the challenge, or to bypass it
                 if you have an accessibility cookie"   aid="checkbox"
Document   name="hCaptcha challenge"
Image      name="Image-based CAPTCHA challenge. Please use the 'Accessibility: Retrieve Cookie'
                 or 'Accessibility: Text Challenge' options if available in the challenge menu."
Button     name="Skip Challenge, page 1 of 2"
```

Screenshots (e-mail field black-boxed before the files were kept):

| file | what it shows |
|---|---|
| `evidence/…/shots/osl-client-1.c1-after-captcha-click.png` | *"Select the animal that emerges from an egg"*, 6 images, `Skip` |
| `evidence/…/shots/osl-client-2.c2-after-captcha-click.png` | *"Please drag the icon to the place where it fits"* |
| `evidence/…/shots/osl-client-{1,2}.dclogin-2-after-login.png` | the modal over a filled, accepted login form |

**Four attempts, two VMs, two accounts, two public IPs, both challenge families. It is the IP class, not
the account.**

**The refusal.** hCaptcha is an anti-automation control. Defeating it — by a solver, by a paid solving
service, by an accessibility cookie obtained under false pretences, or by proxying the VM through a
residential IP — is circumvention, not testing. That is the same class of move as widening an allowlist
to make a gate pass, and the answer is the same. **The lane stopped.**

### What actually unblocks it — ~2 minutes of owner time per VM

**RDP is already open on both VMs** (`osl-client-{1,2}NSG` each carry an `Allow` rule on 3389 from `*`),
and both ports answer:

| VM | public IP | 3389 |
|---|---|---|
| `osl-client-1` | `68.221.69.0` | open |
| `osl-client-2` | `158.158.2.3` | open |

Admin user is **`osladmin`** on both. Discord is **already installed and already sitting on the login
screen with the right account typed in** — the owner only has to click the pictures. Discord then
persists the session token in the Electron profile, so **this is a one-time cost, not a per-run one.**

If the `osladmin` password is not to hand:
`az vm user update -g osl-vmqa -n osl-client-1 -u osladmin -p '<chosen>'` (and again for `-2`). **The
owner should choose and set it; this lane deliberately did not.**

*A QR-code login is the only other door Discord offers, and it needs the mobile app already signed into
the burner account. There is no third door.*

## 3 · NEW, and it blocks the keystone independently of every account:
## **the artifact list cannot be satisfied by one run.** Verified at source.

D-233 established that `place()` cannot place without sending. **The other half of that vice has now
been verified, and it points the opposite way:** the carry receipt cannot be earned by a run that sends.

| | |
|---|---|
| `native_discord_adapter.rs:18019` | `pub(super) fn place(` — the shipping placement |
| `native_discord_adapter.rs:18579` | `if !send_enter(target.window, …)` — **inside `place`** |
| `native_discord_adapter.rs:18491` | success return via Discord's own Send action → **`enter_sent: true`** |
| `native_discord_adapter.rs:18659` | success return via the Enter key → **`enter_sent: true`** |
| `native_discord_adapter.rs:255` | `DiscordProtectedSendOutcome::Sent` requires `placed && enter_sent` |
| **`native_apps.rs:4253-4255`** | **`verify_receipt` → `bad!("enter_sent is not false")`** |
| `native_apps.rs:4256-4258` | and `composer_empty_after_clear` must be `true` |

**Both success returns of `place()` set `enter_sent: true`. `verify_receipt` rejects any receipt whose
`enter_sent` is not `false`.** Therefore:

> **A Discord run that POSTS can never produce a valid `LiveCarryReceipt`. A run that produces a valid
> `LiveCarryReceipt` never posted.**

And the posting half is not optional: the pointer travels only inside the cover prose, and the receive
leg decodes **posted** accessibility transcript rows (`broker.rs`, `rehydrate_native_discord_*`). So
clauses 2–4 of the keystone (post · peer decodes · peer sees only the cover) **require** `enter_sent ==
true`, which is exactly what the receipt forbids.

**This is not a defect in either mechanism.** The receipt is a *placement* proof by construction and is
right to be; `place()` is a *send* and is right to be. It is a defect in the **specification**, which
asks one run to be both.

**The re-spec, and it costs nothing:**

1. **Run A — the receipt run.** The oracle probe borrows `shipping_type_text` / `shipping_clear_composer`
   (`landing_oracle/live.rs`), `send_enter` deliberately out of reach. Judges landing by the document,
   clears, proves empty. Earns `carry-receipts/discord.json`. **Must be labelled the oracle probe, not
   `place()`.**
2. **Run B — the keystone run.** The real `send_native_discord_overlay_carrier` → `place()` → Enter.
   Posts. Produces the three screenshots and the peer decode. **Earns no receipt, and must not be made
   to.**

Two runs, same two accounts, same session. **Nobody should weaken `verify_receipt` to merge them —
`enter_sent == false` is what makes the receipt mean "this was proven without touching anyone's chat".**

## 4 · D-228 — its fix HAS merged, and that does not mean the composer can be emptied

Re-tested at source, as the brief asked, rather than assumed either way.

`913452084` *"D-228: the carrier reclaim must PROVE the composer is empty, not that a key was accepted"*
is on this branch, and `SelectAllDeleteOutcome` (`native_discord_adapter.rs:16827`) now distinguishes
**`AcceptedUnverified`** from **`Cleared`**, with `Cleared` gated on the landing oracle judging the
document.

**Read that carefully: the fix changes the REPORT, not the MECHANISM.** D-228's measured fact was that
*no delete keystroke reaches Slate's document*. If that is still true live, the reclaim now returns
`AcceptedUnverified` — truthfully — and `composer_empty_after_clear` stays **unobtainable**, which keeps
`verify_receipt` refusing, which keeps Discord on `PUBLISHED_WITHOUT_A_LIVE_RECEIPT`. **Whether the
composer can now actually be emptied is an open LIVE question, it sits on the receipt's critical path,
and it cannot be answered until a Discord client is signed in.** It is the first thing run A should
measure.

## 5 · Corrections to the brief, both verified

1. **`carry-receipts/` DOES exist.** The brief says it "does not even exist as a directory". It is at
   **`apps/osl-hub/carry-receipts/`** — `receipt_path()` resolves through `crate_path`, not the repo
   root — and it is tracked, holding `telegram.json`, `debt-baseline.json` and
   `seam-ledger-baseline.json`. What is true is the narrower statement: **`discord.json` does not
   exist**, and `debt-baseline.json` records `ids: ["discord"], ceiling: 1` against D-203.
2. **`telegram.json`'s missing `seam_contract_sha256` is NOT a live defect.** It is
   `osl-live-carry-receipt-v1`, and `a_receipt_earned_under_the_superseded_v1_binding_is_stale`
   (`native_apps.rs:3234`) asserts a v1 receipt **must** verify as `Stale`. The previous lane flagged it
   as "looks likely to fail its own gate" — it does, deliberately and under test. **Retracted; do not
   open a defect for it.**

## 6 · Two defects in the QA rig itself, both of which made it silently do nothing

Found because every VM command returned success and no output. Both fixed in `osl-plan/vmqa/`; neither
touches the product.

**R-1 · `runps.sh` emitted a PowerShell array literal with a trailing comma, so EVERY run was a silent
no-op.** The library-pinning loop builds `@(@{…},@{…},)`. PowerShell rejects a trailing comma in an
array literal, and **`az vm run-command invoke` reports a parse error as empty output with exit 0** —
so the runner uploaded the script, "ran" it, downloaded nothing, and looked like a machine with no
results. Reproduced in isolation: `@(@{Name="x"},)` → no output; `@(@{Name="x"})` → `count=1`.
Fix: `@(${LIBRARY_SPECS%,})`.

**R-2 · `lease.sh` could never acquire a lease, so the whole VM rig was closed to every lane.**
Two independent faults: (a) its default storage account `osltestartifactsa7d5` is in the unreachable
tenant and answers **`AccountIsDisabled`** — default re-pointed at `oslvmqa47866`, which is the account
these VMs actually use; (b) az CLI 2.88 takes `--blob-name/-b` for the `lease` verbs but `--name/-n` for
`metadata`, and one shared argument array meant every acquire died at `VM-LEASE-ERROR … could not
record holder` **after taking an infinite lease**. Split into two arrays.

**R-3 · `vmqa/test-runps-namespace.sh` has been FAILING since `rc.sh` grew its lease step, and nobody
re-ran it.** Measured exit **2**. Its mock `az` has no `storage blob lease` or `blob metadata` handler,
so every run died at `VM-LEASE-DENIED` before a driver template was ever produced, and the three
namespace assertions at the end were **unreachable** — the *"two VMs' driver blobs cannot collide"*
guarantee was unproven. **Not caused by this lane's R-2 change:** the mock's catch-all rejects the lease
call before the account name is ever read. Fixed by teaching the mock an exclusive lease.
**Proven able to fail:** mutate `DRIVER_BLOB="$VM.$NAME.ps1"` → `"$NAME.ps1"` gives exit **1**; restore
gives exit **0**.

**R-4 (method note, not a fix).** A fresh worktree with no `npm ci` makes ledgers **1, 3, 4 and 7 RED**
with `ledger-input-missing` / `Cannot find module 'vite/package.json'` and `all.mjs` exit **1**. The
ledgers are right to refuse rather than infer, but it reads exactly like a four-ledger regression.
`npm ci` in `apps/osl-hub-ui`, then re-run.

## 7 · What was measured, with real exit codes

| command | result |
|---|---|
| `node scripts/ledger/all.mjs --no-cache` (before `npm ci`) | **exit 1** — ledgers 1/3/4/7 RED, `ledger-input-missing` (see R-4) |
| `node scripts/ledger/all.mjs --no-cache` (after `npm ci`) | **exit 0** — 1-7 GREEN, **8 RED at baseline 14** (by design), 10 GREEN at 4722 pins |
| `npx vite build` | **exit 0** |
| `flock -o /tmp/osl-cargo.lock cargo test --manifest-path apps/osl-hub/Cargo.toml --lib -j 3 -- --test-threads=1` | **exit 0** — **1326 passed / 0 failed / 2 ignored**, 112.36 s |
| `vmqa/test-vm-lease.sh` | **exit 0** |
| `vmqa/test-task-isolation.sh` | **exit 0** |
| `vmqa/test-runps-namespace.sh` before / after R-3 | **exit 2** → **exit 0**; mutant **exit 1** |
| `curl https://keyserver.oslprivacy.com/v1/healthz` | **200** |
| `curl https://ciphers.oslprivacy.com/v1/healthz` | **200** |
| Discord install, both VMs | **exit 0**, `app-1.0.9251` on both |
| `dclogin.ps1` ×2 | `VERDICT=CAPTCHA` both |
| `dccaptcha.ps1` ×2 | `VERDICT=STILL-CAPTCHA`, `SIGNAL IMAGE-CHALLENGE` both |

**Ledger 9 was not exercised against a Discord receipt, and no tampered-receipt mutation was run,
because no receipt was earned.** Both remain owed by run A.

## 8 · The four clauses, scored honestly

| clause | verdict |
|---|---|
| the cover **PLACED** (oracle verdict) | **NOT ATTEMPTED** — no signed-in composer existed to bind |
| the cover **POSTED** | **NOT ATTEMPTED** |
| the peer **DECODED** the plaintext | **NOT ATTEMPTED** |
| the peer's Discord showed **ONLY the cover** | **NOT ATTEMPTED** |
| direction | **neither** |
| receipt earned / survives `verify_receipt` | **none earned.** `apps/osl-hub/carry-receipts/discord.json` does not exist, and nothing was written |

**The carrier model remains proven on zero surfaces. D-224 is unchanged.**

## 9 · The order that actually unblocks this

1. **Owner: RDP to `68.221.69.0` and `158.158.2.3` as `osladmin` and click through the two hCaptchas.**
   ~2 minutes each. Everything below is blocked on it and nothing else is.
2. **Re-spec the artifact as run A + run B (§3)** before the next lane starts, or it will rediscover the
   vice from the inside.
3. **Run A first**, because it answers D-228 live (§4) and D-228 gates the receipt.
4. Then two OSL instances via the compile-time `identifier` lever, paired against the deployed keyserver,
   each bound to its own Discord person, and run B both ways.
