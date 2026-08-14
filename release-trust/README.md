# OSL release metadata trust

`metadata/root.json` is the shipped trust anchor. Its root role has exactly
three Ed25519 keys and requires two signatures. The targets, snapshot, and
timestamp roles each have one distinct key. Targets delegates three terminating
path namespaces to distinct update, build-proof, and carrier-table keys.

The repository contains public keys, signatures, metadata, and sample release
artifacts only. There is deliberately no signing API in the production crate.
The nine role keys are created and held separately from this repository:

- each root key is held by a different offline custodian or hardware device;
- root ceremonies collect any two of the three signatures outside CI;
- online role metadata is signed outside CI and delivered as a complete public
  metadata set; and
- CI, builders, website storage, and application packages receive only that
  public set and run the verifier and private-material scanner before release.

Verification starts from `root.json`, verifies timestamp and its snapshot
descriptor, verifies snapshot and every metadata descriptor, verifies targets
and the named delegation, and finally checks the selected artifact's length and
SHA-256 digest. A key ID authorized for any other role is rejected, even when
the presented signature set would otherwise meet a threshold.

Runtime consumers use `SequentialTrustClient`, bootstrapped from the root that
ships with the application rather than from a downloaded repository root. A
repository rotation is placed at `metadata/roots/<version>.root.json`; every
version must be exactly one greater than the durable pin and must meet both the
old root threshold and the new root threshold. The client durably pins the
highest root and each online role's version plus canonical signed-body digest,
so rollback and a second, different same-version view are refused. Update,
unmodified-build proof, and carrier-table calls all enter that client and
receive owned authenticated bytes, never a path to reopen after verification.

A root-key compromise cannot be distinguished cryptographically from a valid
threshold signature. A separately authenticated incident decision therefore
sets the client's local terminal compromise bit. Once set, all in-band roots
and targets are ignored and the error names
`offline/OSL-Recovery-Installer.exe`; only that separately authenticated
offline installer can establish a new trust anchor.

Run the two release gates from the repository root:

```sh
cargo run -p release-trust --bin verify-release-trust -- release-trust
cargo run -p release-trust --bin scan-release-private-keys -- .
cargo run -p release-trust --bin task-5170b
```
