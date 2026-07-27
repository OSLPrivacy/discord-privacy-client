# Scheme-1 prekey owner-proof server handoff

Status: `test-proven-only`. Nothing in this change was deployed or applied to
live D1. Client integration, independent audit, and runtime proof remain open.

## Exact contract

The normative descriptor is
`SCHEME1_PREKEY_CONTRACT_DESCRIPTOR` in
`src/lib/prekey-owner-proof.ts`. Its UTF-8 SHA-256 is:

```text
2febeb312152c66a3c79aae75b0ac3d0cd6d4c77b4b779de49123b0c9b839048
```

The Worker implements the byte framing documented by the active keystore
handoff without copying client source:

- replenish domain
  `discord-privacy-client/prekey-replenish/v2`;
- batch domain `discord-privacy-client/opk-owner-batch/v1`;
- owner-proof domain `discord-privacy-client/opk-owner-proof/v1`;
- owner-proof version `1`, identity-blob version `3`, lifecycle version `2`;
- big-endian u32/u64 integers and u32-length-prefixed UTF-8 strings;
- canonical padded RFC 4648 base64 at every binary field;
- full identity commitment over the exact `OSL-REGISTER-v1` field form with
  the scheme-1 capability bitmap present.

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

Migration `0034_scheme1_prekey_owner_proofs.sql` adds:

- immutable proof JSON, generation, and batch commitment to each scheme-1 OPK
  pool row;
- one non-deletable per-owner lifecycle authority;
- generation-1-only genesis and exact `old + 1` D1 CAS;
- D1 triggers that keep scheme-0 and scheme-1 rows separated and prevent
  duplicate public-key reuse or proof/context mismatch.

The replay receipt, identity CAS, lifecycle CAS, SPK/pool transition, and OPK
inserts run in one D1 batch. SPK or identity-context rotation removes the old
pool before the replacement proof batch is inserted. The consuming GET pops
by `(lifecycle_generation, opk_id)` and returns the exact persisted
`owner_proof`, `registration_sig`, `rn_capabilities`, root key/proof, identity
scheme, and revision.

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
- Independent server/client contract audit and exact-byte cross-language
  fixtures remain required.
- Migration 0033, migration 0034, matching Worker activation, trusted evidence
  producer enrollment, live D1 readback, and live runtime proof are all
  unperformed.
