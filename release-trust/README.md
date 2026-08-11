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

Run the two release gates from the repository root:

```sh
cargo run -p release-trust --bin verify-release-trust -- release-trust
cargo run -p release-trust --bin scan-release-private-keys -- .
```
