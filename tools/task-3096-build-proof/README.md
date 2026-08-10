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
output. This is the unsigned payload: task 3097 applies the OSL offline
signature chosen by task 3095, and task 3098 verifies it. Keeping signing out
of this command prevents the build machine from pretending to be the offline
OSL signing key.

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
