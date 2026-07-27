# Scheme-1 prekey owner-proof server handoff

Status: `test-proven-only`. Nothing in this change was deployed or applied to
live D1. Client integration, independent audit, and runtime proof remain open.

## Exact contract

The normative descriptor is
`SCHEME1_PREKEY_CONTRACT_DESCRIPTOR` in
`src/lib/prekey-owner-proof.ts`. Its UTF-8 SHA-256 is:

```text
8041c9c14f841935c6b42e74829e39747e6b8915bf4c929c80ff1d3e189dffaa
```

The Worker implements the byte framing documented by the active keystore
handoff without copying client source:

- replenish domain
  `discord-privacy-client/prekey-replenish/v2`;
- batch domain `discord-privacy-client/opk-owner-batch/v1`;
- owner-proof domain `discord-privacy-client/opk-owner-proof/v1`;
- identity scheme `1`, public identity-bundle version `1`, prekey protocol
  version `2`, owner-proof version `1`, and lifecycle version `2`;
- big-endian u32/u64 integers and u32-length-prefixed UTF-8 strings;
- canonical padded RFC 4648 base64 at every binary field;
- full identity commitment over the exact binary
  `OSL-FULL-IDENTITY-BUNDLE-v1\0` field form, including the root key, scheme,
  public bundle version, identity revision, every encryption key, and the RN
  capability bitmap.

`identity_bundle_version` is a server wire-protocol field. It is deliberately
not the Rust keystore's `IDENTITY_BLOB_VERSION`: that Rust value versions a
private sealed on-disk serialization that the server neither receives nor
interprets. The server contract therefore does not guess or pin a client
storage format.

For scheme 1, the current Ed25519 key signs the SPK, every OPK owner proof, and
the outer replenish request. That key is not treated as free-standing trust:
the authoritative `osl1_...` identity is root-derived, and D1 retains the
immutable root key, root signature over the complete scheme-1 identity bundle,
current-key signature, identity revision, and capability bitmap. A proof is
therefore bound to the root-derived owner, current full-bundle commitment,
SPK, complete OPK batch, lifecycle generation, and capability/version fields.

## Shipping routes and durable state

The real `POST /v1/prekey-bundle/replenish` route selects the durable identity
scheme. Scheme 0 retains the separately named v1 compatibility path. Scheme 1
requires `protocol_version: 2`, a nonempty complete proof batch, and exact
canonical fields; v1/proofless downgrade is refused.

Scheme 0 remains the historical tagless registration shape. If
`identity_scheme` is present on `/v1/register`, it must be the exact numeric
value `1`; `0`, strings, unknown versions, extra fields, and stripped
scheme/root proofs are refused instead of being reclassified by the legacy
parser. Scheme-1 register, pubkey, replenish, and consuming-fetch responses
carry explicit scheme/bundle/protocol tags. The signed full bundle and each
OPK proof bind those values.

Migration `0034_scheme1_prekey_owner_proofs.sql` adds:

- immutable proof JSON, generation, and batch commitment to each scheme-1 OPK
  pool row;
- one non-deletable per-owner lifecycle authority;
- scheme-tagged replenish receipts containing the original identity revision,
  identity commitment, lifecycle generation, batch commitment, and batch
  count;
- rejection of legacy-shaped receipts for scheme-1 owners, with every
  scheme-1 lifecycle transition requiring the receipt's exact scheme,
  protocol, identity, generation, batch, and count fields;
- generation-1-only genesis and exact `old + 1` D1 CAS;
- D1 triggers that keep scheme-0 and scheme-1 rows separated and prevent
  duplicate public-key reuse or proof/context mismatch.

The replay receipt, identity CAS, lifecycle CAS, SPK/pool transition, and OPK
inserts run in one D1 batch. SPK or identity-context rotation removes the old
pool before the replacement proof batch is inserted. The consuming GET pops
by `(lifecycle_generation, opk_id)` and returns the exact persisted
`owner_proof`, `registration_sig`, `rn_capabilities`, root key/proof, identity
scheme/bundle version, prekey/lifecycle version, exact selected generation and
batch commitment, and revision.

The original successful replenish, an exact signed-request replay, and a
fresh authenticated request for the already-current generation and identical
batch all return the same `scheme1_replenish_committed` JSON result. An exact
request receipt keeps its original generation/batch result even if the
lifecycle later advances. A fresh request for a lower generation or a
different batch is refused. This separates lost-response recovery from caller
rollback.

Deterministic cross-language registration, root/current proof, pubkey,
owner-proof, replenish, fetch, and replay vectors are frozen at
`test/fixtures/scheme1-contract-vectors.json`. Workerd recomputes the
production canonicalizers and signature verification against those exact
bytes, while separate Workerd tests exercise the shipping routes and real D1
migrations with equivalent fresh signed requests.
`scripts/scheme1-contract-mutants.mjs` must kill both the legacy-classifier
and permissive-owner-proof mutants through their designated negative
assertions, rather than accepting an unrelated compile or setup failure.

## Rollout gate

Migration 0033 and then migration 0034 must be observed on the exact configured
D1 database before the matching Worker is activated. Admission requires a
signature from the committed trusted-producer registry, an exact provider
observation of the activated Worker version/deployment at 100% traffic, and
source hashes for both migrations. The producer registry remains deliberately
empty, so production provisioning is blocked.

Genesis recovery authority is the fixed owner-only path
`/var/lib/oslprivacy/keyserver/canonical-rollout-genesis.json`, never
`--output`. The file is exclusively created, file-synced, inode/link checked,
then directory-synced before remote mutation. Ambiguous retry reconstructs the
same SQL from the retained nonce. D1 mutation is followed by a separate full
row SELECT binding the nonce/receipt and exact commit/repository/keyserver
trees; caller-selected output receives only a derived receipt without the raw
nonce.

## Remaining blockers

- The active keystore worktree is only an inspected contract source. No
  uncommitted client code was copied, changed, or claimed wired.
- Client response parsing must explicitly understand scheme 1 and verify the
  root/current full-bundle proofs before using these fields.
- A client must persist the peer OPK generation/batch pin before returning a
  fetched OPK to a caller.
- The Rust client must admit the exact checked-in vector object and contract
  digest, then persist scheme/generation/batch pins across restart, before
  migrations 0033/0034 are eligible for any deployment decision.
- Migration 0033, migration 0034, matching Worker activation, trusted evidence
  producer enrollment, live D1 readback, and live runtime proof are all
  unperformed.
