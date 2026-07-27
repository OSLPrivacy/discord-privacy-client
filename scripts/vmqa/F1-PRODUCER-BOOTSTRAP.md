# F1 producer bootstrap and staging

This workflow prepares, but does not perform, the F1 Azure runtime probe. It
does not start a VM, upload a build, weaken a Windows ACL, or create evidence.

The trust boundary is fixed:

- source commit `1f745c85bb23cf79a956aa87d623905e20f83cf1`
- source tree `1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a`
- producer identity `osl-vmqa-producer` with a non-login shell
- host key `/var/lib/osl-qa/private/f1-native-witness.key`
- staging root `/var/lib/osl-qa/f1-staging`
- installed programs under `/opt/osl-vmqa/bin`

Production accepts no key path, executable path, expected commit, expected
tree, expected executable hash, output path, or staging destination. The sole
build input is a complete VMQA build bundle. That bundle must pass the strict
bundle verifier and its identity digest must have the detached producer seal
in `/var/lib/osl-vmqa/producer-seals`. The immutable source pin, sealed build
identity, and rehashed bundle bytes jointly establish the executable hash.

## One-time privileged provisioning

A human administrator must create the non-login account and install the
reviewed committed scripts as root-owned, mode `0555` files:

```text
/opt/osl-vmqa/bin/vmqa_f1_producer.py
/opt/osl-vmqa/bin/vmqa_build_evidence.py
/opt/osl-vmqa/bin/vmqa_f1_provisioning_preflight.py
/opt/osl-vmqa/bin/vmqa-f1-windows-provisioning-preflight.ps1
/opt/osl-vmqa/bin/vmqa-run.sh
```

The administrator must create `/var/lib/osl-qa` as root-owned mode `0755`,
then create `private` and `f1-staging` beneath it as
`osl-vmqa-producer`-owned mode `0700`. The workflow deliberately does not
repair a wrong owner, mode, account, shell, or symlink.

The administrator preflight is read-only, takes no arguments, and must run as
uid 0 from that fixed installation:

```bash
/opt/osl-vmqa/bin/vmqa_f1_producer.py provisioning-preflight
```

The fixed producer hashes the provisioning module against its immutable source
pin before importing it; direct execution of the module refuses. It then
refuses unless the dedicated account is non-root and non-login; the fixed
installation and program bytes match their immutable hashes; every production
tool pin resolves beneath `/opt/osl-vmqa/toolchain` through root-owned `0755`
directories to a root-owned regular `0555` file with the pinned hash; the
authority, private, staging, seal, lock, and monotonic-state objects have their
exact owners and modes; and exactly one current protected staging receipt binds
the current production seal to the pinned commit/tree, identity, executable,
loader, and terminal descriptor snapshot. It performs no repair and reports
`writesPerformed: 0` only after every check succeeds. There are intentionally
no path, expected-hash, identity, tool-pin, seal, bundle, or fixture options.

The existing production build-tool pins still name the reviewed Liam-owned
toolchain. That toolchain is not traversable by the non-login producer account
and must not be opened up. Before a real build, an administrator must install
equivalent root-owned, non-producer-writable tools under a protected fixed
toolchain directory; the code pins must then be updated to their independently
measured hashes and accepted as a separate exact commit. Until then,
production build creation and the administrator preflight must remain
fail-closed. Copying the current symlink wrappers without their exact protected
runtime tree is not sufficient.

The producer then creates a new key without printing it:

```bash
sudo -u osl-vmqa-producer \
  /opt/osl-vmqa/bin/vmqa_f1_producer.py bootstrap-key --generate
```

Alternatively, an already generated 64-character lowercase hexadecimal key
may be delivered on standard input with `--import-stdin`. There is
intentionally no import-path option. Do not place the key in an argument,
environment variable, repository, rendezvous share, artifact directory, or
log.

## Exact build admission

After the producer-owned build command has created and sealed the exact
commit bundle, a dry run checks every prerequisite without writing:

```bash
sudo -u osl-vmqa-producer \
  /opt/osl-vmqa/bin/vmqa_f1_producer.py preflight \
  --bundle /path/to/new-producer-bundle
```

Staging copies the complete verified bundle into the fixed protected store,
verifies the copy, publishes it without replacement, and verifies the final
published bytes:

```bash
sudo -u osl-vmqa-producer \
  /opt/osl-vmqa/bin/vmqa_f1_producer.py stage \
  --bundle /path/to/new-producer-bundle
```

The destination is derived from the independently sealed executable SHA-256.
An existing file, directory, or symlink at that destination is a replay and
is refused. Producer seals carry a protected monotonically increasing
generation, the preceding seal hash, and an exact initial/successor
transition. The global seal state is advanced under a producer-owned lock.
The staging store separately records the last accepted generation and accepts
only a strictly newer producer seal, so restoring an older otherwise-valid
bundle cannot reopen admission.

The retained staging receipt binds commit, tree, build identity, producer
seal generation and predecessor, executable, loader, key identifier, sizes,
hashes, and final paths. It also records the device, inode, mode, link count,
size, and SHA-256 of the final identity, executable, loader, and retained seal.
Immediately before returning, staging reopens and rehashes that complete
destination snapshot and requires exact equality with the receipt. It
contains no key bytes.

## Offline provisioning plan manifest

`vmqa_f1_provisioning_plan.py` converts a closed JSON plan input into a
deterministic machine-readable manifest. It does not inspect or modify the
host, guest, Azure, key, build, or staging store. It emits canonical JSON on
standard output only; it has no output-path or mutation option:

```bash
python3 scripts/vmqa/vmqa_f1_provisioning_plan.py plan \
  --input /absolute/path/to/non-secret-plan-input.json

python3 scripts/vmqa/vmqa_f1_provisioning_plan.py verify \
  --manifest /absolute/path/to/retained-plan.json
```

The `execute` operation always refuses. A valid result says
`offline-plan-only`, `planned`, `writesPerformed: 0`, and
`executionPermitted: false`; it never says that an account, tool, key, build,
ACL, VM, or witness is ready, installed, verified, or executed.

The manifest contains exactly seven ordered state-transition descriptions,
not an executable checklist. It binds the accepted `2279be3` predecessor
contract snapshot, exact `1f745c85` release commit/tree, fixed account and
installation paths, reviewed program hashes, one canonical independent hash
for each fixed-root tool, the derived tool inventory hash, non-secret key
presence metadata and key ID, the sealed release hashes and derived staging
path, every planned SYSTEM-only guest ACL entry, and the exact tokenized
runtime argv. The runtime transition requires the separate
`separate-live-f1-runtime` authorization and cannot be invoked by the
generator or either validator.

The checked-in `f1-provisioning-plan-safe-input.json` contains synthetic
hashes solely for deterministic refusal fixtures. Because every output is
plan-only, neither that fixture nor a caller-authored production input is
provisioning evidence. Actual independent hashes and presence metadata remain
external human inputs until the host and guest preflights validate real state.

## Remaining live step

The fixed Windows preflight also takes no arguments. It must be copied from the
hash-pinned host installation and run locally on the guest as SID `S-1-5-18`
(`NT AUTHORITY\SYSTEM`):

```powershell
C:\ProgramData\OSL-QA\bin\vmqa-f1-windows-provisioning-preflight.ps1
```

It reads only `C:\ProgramData\OSL-QA\f1-staging`, requires exactly one
executable-hash directory, rejects reparse points and layout extras, and checks
the staging root and every descendant for owner SYSTEM plus a protected,
non-inherited DACL containing exactly one allow ACE: SYSTEM FullControl. It
then rehashes the pinned-source build identity, executable, loader, producer
seal, and receipt twice before returning a non-secret `writesPerformed: 0`
record. It never reads the witness key. A host-side transcript or caller JSON
cannot substitute for this local SYSTEM execution.

The remaining human actions are therefore:

1. Create the dedicated non-root, non-login producer account and exact
   host directories without granting it a general shell or arbitrary-command
   sudo rule.
2. Install the five reviewed programs at the fixed root-owned `0555` paths.
3. Install a complete usable root-owned toolchain beneath
   `/opt/osl-vmqa/toolchain`, independently measure every resolved tool, and
   land a separate reviewed pin update replacing the current Liam-owned paths.
4. As the producer, create or import the real witness key without putting it in
   arguments, environment, repository, share, artifacts, or logs.
5. Produce and seal the exact `1f745c85` / `1b9bbbc` release bundle, stage it
   through the accepted producer transaction, and obtain a green host
   administrator preflight.
6. While the VM remains off, arrange the fixed guest files and SYSTEM-only ACL
   through the authorized provisioning channel, including provisioning the
   same witness key at
   `C:\ProgramData\OSL-QA\private\f1-native-witness.key` without exposing it to
   the staging tree or logs. After the VM is started under separate
   authorization, run the fixed Windows preflight locally as SYSTEM and retain
   its raw output.
7. Only then verify the standard evidence session and guest agent, execute the
   SYSTEM witness and intended rights negative control, retain all raw
   artifacts, and grade the strict verifier.

Neither preflight is runtime evidence. Until those live steps occur, F1 stays
unchanged.
