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
```

The administrator must create `/var/lib/osl-qa` as root-owned mode `0755`,
then create `private` and `f1-staging` beneath it as
`osl-vmqa-producer`-owned mode `0700`. The workflow deliberately does not
repair a wrong owner, mode, account, shell, or symlink.

The existing production build-tool pins still name the reviewed Liam-owned
toolchain. That toolchain is not traversable by the non-login producer account
and must not be opened up. Before a real build, an administrator must install
equivalent root-owned, non-producer-writable tools under a protected fixed
toolchain directory; the code pins must then be updated to their independently
measured hashes and accepted as a separate exact commit. Until then,
production build creation must remain fail-closed.

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
is refused. The retained staging receipt binds commit, tree, build identity,
producer seal, executable, loader, key identifier, sizes, hashes, and final
paths. It contains no key bytes.

## Remaining live step

Once the fixed installation, producer account/directories, real witness key,
and real sealed release bundle exist, the human VM owner must provision the
same key into `C:\ProgramData\OSL-QA\private\f1-native-witness.key` with a
protected SYSTEM-only ACL. The exact host runner must itself execute as
`osl-vmqa-producer`, because its key check requires the key owner to equal the
runner UID; install that exact runner and its dependencies as root-owned,
non-producer-writable bytes rather than making the repository traversable.
Only then may the owner start the Azure VM, verify the dedicated standard
evidence session and guest agent, run the SYSTEM witness, execute the rights
negative control, retain all raw artifacts, and grade the strict verifier. A
green bootstrap or staging receipt is not runtime evidence and earns no F1
point.
