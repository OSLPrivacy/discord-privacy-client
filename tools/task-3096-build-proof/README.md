# OSL build-proof binding

`osl-build-proof` defines the payload covered by one unmodified-build proof.
One proof binds exactly one value in each of these five roles:

1. `buildFingerprint` — the lowercase SHA-256 fingerprint of one build;
2. `deviceId` — the stable identifier of the one device on which it was proved;
3. `personId` — the stable identifier of the one person for whom it was proved;
4. `madeAtUnixSeconds` — the Unix-second instant when the proof was made; and
5. `stopsCountingAtUnixSeconds` — the later Unix-second instant after which
   the proof no longer counts.

The command emits one JSON object with exactly those five fields on standard
output. This is the unsigned payload. Keeping signing out of this command
prevents the build machine from pretending to be the offline OSL signing key.

```sh
cargo run --manifest-path tools/task-3096-build-proof/Cargo.toml --bin osl-build-proof -- \
  --build-fingerprint <64-lowercase-hex> \
  --device-id <device-id> \
  --person-id <person-id> \
  --made-at-unix-seconds <integer> \
  --stops-counting-at-unix-seconds <integer>
```

All five flags are mandatory. Empty identifiers, malformed fingerprints,
invalid integer instants, and a stop time that is not later than the made time
are refused without printing a proof.

## Sign and verify

`osl-sign-build-proof` turns the same five values into a signed proof. It uses
Ed25519 with a domain-separated, length-prefixed canonical encoding; the
signature is not over incidental JSON whitespace. The signing-key file must
contain the base64 encoding of exactly one 32-byte Ed25519 secret seed.

```sh
cargo run --manifest-path tools/task-3096-build-proof/Cargo.toml \
  --bin osl-sign-build-proof -- \
  --signing-key-file <offline-key-file> \
  --build-fingerprint <64-lowercase-hex> \
  --device-id <device-id> \
  --person-id <person-id> \
  --made-at-unix-seconds <integer> \
  --stops-counting-at-unix-seconds <integer> > build-proof.json
```

The signing key belongs on the offline hardware selected by task 3095, never
on a build machine or in CI. The signed proof does not carry a public key:
accepting a key supplied by the proof would let anyone mint a trusted proof.
Instead, the check command requires an independently trusted public-key file,
also base64-encoded and exactly 32 bytes.

```sh
cargo run --manifest-path tools/task-3096-build-proof/Cargo.toml \
  --bin osl-check-build-proof-signature -- \
  --proof-file build-proof.json \
  --trusted-public-key-file <osl-public-key-file>
```

A valid proof prints `signature valid`. A proof signed by any other key is
refused with `bad-signature` and exit status 1.

## Classify the current build

`osl-check-build-proof` reads the signed proof, checks it against the
independently trusted public key and its counting window, and compares its
fingerprint with the observed build fingerprint:

```sh
cargo run --manifest-path tools/task-3096-build-proof/Cargo.toml \
  --bin osl-check-build-proof -- \
  --proof-file build-proof.json \
  --trusted-public-key-file <osl-public-key-file> \
  --build-fingerprint <64-lowercase-hex>
```

It prints `unmodified` or `modified` only when the proof supports that answer.
Otherwise it prints plain wording beginning `OSL cannot check this person's
app`. A missing proof, an expired proof, and an old proof schema say why, but
none is presented as a clean-build claim. Unreadable or malformed proofs,
invalid signatures, and invalid observed fingerprints use the general
cannot-check wording. `--at-unix-seconds` is available for deterministic
checking and otherwise the current system time is used.
