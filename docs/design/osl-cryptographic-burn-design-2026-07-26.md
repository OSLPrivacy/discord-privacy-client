# Proposed OSL cryptographic burn: deferred design

> **READ THIS FIRST.** The phrase "cryptographic burn" appears in this document
> ONLY as the name of a property that **does not exist and is not being built**.
> `docs/THREAT_MODEL.md` bans that phrase and "permanently undecryptable" from
> every user-facing surface, and this document does not license their use
> anywhere else. Nothing here describes shipping behaviour.
>
> **What burn does today:** it destroys the local cached plaintext on this
> device — zeroes the stored ciphertext and nonce, marks the row burned, deletes
> the cached attachments, truncates the WAL. That is real. It does **not** and
> cannot revoke anyone else's ability to decrypt the ciphertext Discord still
> holds, because the send path wraps to long-term recipient identity keys.

This document specifies a deferred construction. It does not describe a feature
that exists, and it does not authorize implementation. The current product
truth remains the one in `docs/THREAT_MODEL.md:148-190`: burn destroys OSL's
local cached copy, but it does not revoke the ability to decrypt ciphertext
that Discord still holds.

The current local destruction is real. `crates/store` zeroes the cached
ciphertext and nonce, marks the row burned, deletes cached attachments, and
truncates the SQLite WAL; see `crates/store/src/lib.rs:126-168`,
`:397-415`, and `:677-728`. The `messages.wrapped_key` column exists, but
`MessageStore::put` does not write it and a repository-wide call-site sweep
found no path that has ever written it non-NULL. Every burn path sets an
already-NULL value to NULL. Nothing in this design diminishes the value of the
local destruction or changes what may be claimed about it today.

## 1. What the proposed “cryptographic burn” property would mean

For a message created under the new protocol, define:

- `M` as the message plaintext;
- `K` as a fresh, uniformly random per-message body key;
- `C = AEAD_Encrypt(K, M, authenticated_header)` as the ciphertext carried by
  Discord;
- `R` as the service-side record or distributed capability without which an
  authorized client cannot recover `K`; and
- `T_burn` as the linearization point at which `R` becomes irrevocably
  unavailable to fetch.

The proposed property is:

> After `T_burn`, possession of the complete Discord carrier, all sender and
> recipient long-term identity keys, and all non-secret protocol metadata is
> insufficient to recover `M`, unless the adversary obtained `K`, plaintext, or
> an equivalent decryption capability before `T_burn`, or breaks an explicitly
> named cryptographic or service-trust assumption.

This is deliberately a future-access property, not remote erasure. It does not
erase plaintext that a recipient already read, copied, screenshotted, exported,
backed up, or retained with a modified client. It does not stop a recipient
from fetching and archiving `K` while access is valid. It does not promise
memory-forensic erasure from an endpoint. It does not remove Discord's copy of
`C`.

The property is falsifiable. A conforming implementation must pass, at minimum,
this experiment:

1. Send and successfully open a new-protocol message.
2. Archive its exact Discord carrier and every participant's long-term identity
   secrets, but do not archive `K` or a live client cache.
3. Complete burn and obtain the evidence required by the selected trust model.
4. Start a fresh authorized client from the archived identity material.
5. Confirm that the key service refuses recovery and that the archived carrier
   cannot be decrypted.

The negative control must show that step 5 succeeds before burn. A mutation
that leaves a recipient-identity wrapping of `K` in the carrier must make the
post-burn test fail. Tests with an intentionally retained `K` must continue to
decrypt and must be reported as an explicit limitation, not treated as a
protocol failure.

No current message meets this definition. The current v3 send path generates a
fresh body key but places a copy of that key, wrapped to each recipient's
long-term X25519 and ML-KEM identity material, in the Discord carrier
(`crates/ipc/src/wire_v2.rs:673-760`). Anyone who later obtains the recipient
identity secrets can recover the body key. Local burn cannot revoke that.

## 2. Design

### 2.1 Key custody and message flow

`K` is generated on the sender and encrypts the body exactly once. The new
carrier must not contain any copy of `K` that can be opened using participant
long-term identity secrets alone. This is the decisive difference from v3.

For as long as the message remains available:

- The sender holds `K` only long enough to encrypt the body and establish the
  durable key record.
- The service-side record `R` holds `K`, or an equivalent capability that can
  recover or release `K`, under the custody architecture selected below.
- An authorized recipient obtains `K` only through the service. Delivery should
  be encrypted to a fresh request/device key as well as protected by transport
  security, but that does not remove the service from the trust model.
- A client may cache `K` in memory for a short, fixed interval. It must evict it
  on a valid burn notice, lock, account removal, and cache expiry. Persistent
  caching of `K`, or of any equivalent that permits offline recovery of `K`, is
  prohibited for a message carrying the new lifecycle label.
- The sender's other devices need explicit fetch authorization if sent history
  must remain reopenable. They must not receive a durable identity-key wrapping
  as a shortcut.

The availability consequence is intentional and unavoidable: a cold client
cannot decrypt while offline, and a key-service outage blocks opening after the
memory cache expires. A design that permits indefinite offline decryption by
long-term identity keys also permits indefinite post-burn decryption by those
keys.

Attachments are part of the same claim boundary. Either each attachment key is
service-mediated, or attachment keys are wrapped under `K` so destruction of
the capability for `K` also blocks a cold attachment decrypt. Leaving an
independent long-term-recipient wrapping for an attachment would make burning
the text while retaining the file a false success.

### 2.2 Who may fetch

Authorization is a snapshot fixed at send time, not whatever Discord later
reports as the current channel membership. At minimum, `R` must be bound to:

- an opaque content/key-record identifier;
- the authenticated sender identity and sending device;
- the exact authorized recipient device set, including the sender's authorized
  devices if sent-history recovery is supported;
- the protocol and authorization-policy versions;
- a commitment to the authenticated wire header/body; and
- an active, revoked, or terminal lifecycle state.

Recipients may fetch while the record is active. The authenticated sender may
burn. Administrative service credentials must not silently become message
recipients. Multi-device addition, device loss, reinstall, identity rotation,
group membership change, and account removal all need explicit rules; none may
be inferred from a mutable server-returned key without identity verification.

The current audit finding is a precondition, not a side issue. The keyserver can
substitute recipient X25519 and ML-KEM encryption keys while leaving the
Ed25519 key used by TOFU unchanged
(`docs/security/osl-audit-2026-07-26-codex.md:25-79`). Until every fetched
encryption/device key and capability is bound to the already-trusted complete
identity bundle, the service can arrange access for its own key while the user
sees no identity change. A server with that power is not a credible deletion
authority.

### 2.3 Create, fetch, and burn ordering

The required lifecycle is a state machine, not a collection of unrelated
endpoints:

1. **Create pending record.** The sender creates `R` with an idempotency token,
   recipient authorization, a commitment to the message, and the escrowed or
   distributed capability for `K`.
2. **Durable acknowledgement.** The key service confirms that the record is
   durable before the client posts the carrier to Discord.
3. **Finalize association.** After Discord assigns the message identifier, the
   sender binds it to the pending record without changing the authorized
   principals or ciphertext commitment.
4. **Fetch.** An authenticated authorized device requests the exact record.
   Active records may release `K`; pending, revoked, expired, mismatched, and
   unknown records fail closed.
5. **Burn.** An authenticated sender request changes the record to a terminal
   revoked state, prevents future fetches, destroys the held capability, and
   creates the evidence defined by the chosen trust model.
6. **Retain a non-secret tombstone.** The tombstone prevents record-id replay or
   accidental recreation. It must contain no material from which `K` can be
   recovered.

Create/finalize/burn must be idempotent. The design must specify cleanup for
create-success/send-failure, send-success/finalize-failure, lost
acknowledgements, retries, and partial group burns. It must also specify one
linearizable order between fetch and burn. A fetch linearized before `T_burn`
may receive `K`; every fetch linearized after it must fail. Merely checking a
revoked flag in one transaction and reading key material in another is not
sufficient.

The repository contains some wrapped-key client/server primitives, but the
required end-to-end per-message lifecycle is not wired into send, receive, or
storage, and there is no deletion proof. Existing primitives therefore do not
satisfy this design.

### 2.4 What “proven deleted” can mean

Against the current untrusted keyserver, remote deletion cannot be proven. The
server can copy `K`, a wrapping secret, a database page, a snapshot, or a
recipient-targeted wrapped value before returning success. A server signature,
deletion count, tombstone, replay receipt, transparency-log entry, or audit log
can prove that the server made a statement or recorded a state transition. It
cannot prove that no usable copy exists.

Accordingly, the current architecture can provide only an attestation:

> The named service reports that it accepted the burn, made the record
> unavailable through its normal fetch path, and ran its configured deletion
> procedure.

That is operational evidence, not cryptographic proof against a dishonest
operator. The UI and protocol status must use that distinction.

A stronger result requires choosing and documenting additional trust:

- A hardware-backed design could keep a per-message recovery secret
  non-exportable and produce remote attestation over approved code, rollback
  protection, and key-handle destruction. The result still trusts hardware,
  firmware, attestation roots, provisioning, side-channel resistance, and the
  claim that no alternate export path existed.
- A threshold design could split recovery capability among independently
  administered services and make reconstruction impossible after enough shares
  are destroyed. The result still assumes the required number of operators did
  not collude or copy shares, and deletion remains supported by their
  attestations or audits.
- A combination of threshold custody, non-exportable hardware, an append-only
  public log, independently reproduced builds, and monitored rollback counters
  can narrow the trusted base. It still proves the property only relative to
  those assumptions.

Ordinary client-only encryption is not a fourth option: if the recipient can
recover `K` forever without a service-held capability, burn cannot revoke that
recovery. A cryptographic accumulator or signed tombstone also does not prove
absence of a copied secret.

The implementation may not proceed until the owner chooses which trust model
is acceptable and the threat model names it. If a malicious keyserver remains
in scope with no hardware, threshold, or independently trusted deletion
boundary, the honest answer is that deletion is attested, not proven.

## 3. Blast radius

This is primarily a protocol and service change. The local store is the last,
smallest part.

### Send path

The live content send path currently resolves long-term recipient identity keys
and calls v3 encryption (`crates/ipc/src/commands.rs:2780-2824`,
`:2964-2980`). It must instead:

- perform authenticated, fail-closed capability negotiation for every intended
  recipient/device;
- generate `K`, create the service record, and wait for durable acknowledgement;
- emit only the new carrier when every required peer supports it;
- authenticate and bind the key reference, recipient snapshot, message type,
  ciphertext, and policy version;
- finalize the Discord-message association and handle orphan cleanup; and
- zero transient copies of `K` on all success and error paths as far as the
  language and operating system permit.

All other content-producing paths, including native-hub/manual-peer sends,
attachments, edits, replies, group sends, control paths that carry content, and
retries, need an explicit decision. No path may silently fall back while
preserving a “revocable” label.

### Receive path

The receive router must recognize the new authenticated wire version, verify
the sender and negotiated capability, extract the opaque key reference, fetch
through an authorized device identity, decrypt in memory, and persist the
message's actual lifecycle classification. It needs bounded cache eviction,
burn-notice handling, offline and service-outage behavior, retry safety, and
clear errors for revoked versus temporarily unavailable records.

Reprocessing Discord history must not revive a burned local row or refetch a
revoked key. Existing local terminal-burn behavior remains mandatory.

### Wire format

A new, implementation-time-assigned wire version is required. Reusing v3 would
make revocable and non-revocable carriers indistinguishable. The authenticated
format must carry, at minimum, the opaque record reference, body nonce and
ciphertext, sender/device binding, authorization-policy version, and enough
commitment data to prevent key-reference substitution, cross-message replay,
recipient-set substitution, and type confusion.

It must not carry a participant-identity wrapping of `K`. Exact byte layout,
algorithm selection, domain-separation labels, size limits, padding, and
attachment binding require a separate cryptographic protocol specification and
external review. They are intentionally not invented here.

### Store schema: v5

The store is schema v4 today. It uses keyed blind indexes for identifiers,
sealed per-row metadata, and opaque `seq` ordering
(`crates/store/src/schema.rs:16-54`). A new column or incompatible sealed
metadata shape is a v5 migration. Because v4 already stamps
`SCHEMA_VERSION = 4` and refuses files from a newer schema
(`crates/store/src/schema.rs:131-137`), a v5 stamp makes v4 binaries refuse the
upgraded file rather than silently mishandle new rows.

The v5 logical record must distinguish at least:

- legacy/non-revocable message;
- new-protocol active message with an opaque key-record reference;
- burn pending or failed;
- burn terminal with no local key/cache material; and
- corrupt or unverifiable lifecycle metadata.

The classification and key reference belong inside authenticated sealed
metadata, or must be equivalently authenticated; they must not weaken v4's
metadata protection. Existing v4 rows migrate explicitly to legacy with no key
reference. Migration cannot manufacture one.

The existing `wrapped_key` column must not be treated as evidence of a
capability merely because it exists. Its final v5 treatment—remove it, or store
only a reviewed service-mediated escrow object—depends on the custody protocol.
Any retained object must be unusable for offline recovery of `K` without a live,
authorized service fetch. The column must never hold a long-term-recipient
wrapping that survives burn or a persistent plaintext `K` cache.

Migration tests must cover transactionality, crash recovery, wrong-key refusal,
v4-to-v5 legacy classification, v5 newer-file refusal by v4, and preservation
of the current v4 blind-index/sealed-metadata invariants.

### `StoredMessage` and construction sites

`StoredMessage` currently has no key reference or lifecycle classification
(`crates/store/src/lib.rs:59-98`). Adding the required field changes every
construction. The established call-site sweep identified:

- `crates/ipc/src/commands.rs:1948`, `:2069`, `:2120`, `:2355`, and `:2383`;
- `crates/ipc/tests/phase6a_edit.rs:51` and `:187`; and
- `crates/ipc/tests/whitelist_repair.rs:204`.

The five production anchors had drifted to `:1952`, `:2073`, `:2124`, `:2359`,
and `:2387` in the worktree when this document was written; they are the same
five constructions. A future implementer must repeat the repository-wide
search rather than assume this list remains complete.

The field must be explicit, not an `Option` whose absence is interpreted using
the running client's feature version. Legacy, active service-mediated, pending,
and terminal states have different security meaning and must survive restart.

### Keyserver

The keyserver needs the complete create/finalize/fetch/burn/tombstone lifecycle,
principal and device authorization, idempotency, expiry, quotas, abuse controls,
linearizable fetch-versus-burn behavior, backup and disaster-recovery rules,
key-material-safe logging, and the selected evidence mechanism. Restore from
backup must not resurrect burned capabilities. Replication lag must not permit
a fetch after the acknowledged burn point.

Before any of that is security-relevant, the full recipient encryption/device
bundle and capability advertisement must be authenticated against the trusted
identity. The existing key-substitution finding must be closed and
independently tested.

### Burn command

The burn command must stop reporting a single undifferentiated success. It must:

1. enumerate the exact service key references covered by the requested message
   or scope;
2. authenticate and submit idempotent burns;
3. retain retry state for service failures without claiming completion;
4. record and expose the level of evidence actually obtained;
5. distribute authenticated peer cache-eviction notices;
6. perform the existing local shred and attachment deletion; and
7. reach terminal success only when the selected service-side condition and
   local destruction both complete.

Current entry points that require review include
`crates/ipc/src/commands.rs:2417` (`cmd_osl_burn_message`), `:6520`
(`cmd_osl_apply_burn`), `:9598` (`cmd_osl_burn_scope_data`), and the broader
engage flow at `:10524`, plus `apps/osl-hub/src/security.rs:1938`
(`burn_scope`) and its broker dispatch. Exact anchors may drift.

A peer burn notice improves prompt cache eviction but is not the authority for
revocation. An offline, malicious, or modified peer may ignore it. The
service-side state is what blocks a later cold fetch.

### Tests

At minimum, the implementation changes or adds tests for:

- crypto/wire: randomness, authenticated-field tampering, wrong key reference,
  recipient-set substitution, replay, attachment-key binding, and proof that
  archived identity keys alone cannot decrypt a new carrier;
- send/receive integration: real two-identity create/fetch/open/burn/refetch,
  offline behavior, service outage, restart, multi-device authorization, and
  sender-history reopening;
- keyserver: unauthorized create/fetch/burn, cross-recipient access, idempotent
  retries, quotas, expiry, backup restore, replica behavior, and races proving
  the chosen fetch/burn linearization;
- failure injection: orphaned pending records, Discord send failure, finalize
  failure, lost acknowledgements, partial scope burn, retry after restart, and
  service deletion failure without a false success state;
- store: all v5 migration cases, terminal local shredding, WAL truncation,
  attachment deletion, no resurrection on history reprocessing, and every
  `StoredMessage` constructor;
- compatibility: every old/new sender-recipient combination, missing,
  unverified, stale, and downgraded capability advertisements, plus a mutation
  that makes a new sender emit v3 and proves the UI does not label that message
  revocable; and
- trust evidence: verification of whatever attestation/threshold mechanism is
  selected, along with explicit tests showing that an ordinary signed server
  response proves only receipt/state reporting and cannot establish absence of
  a secretly copied key.

Existing suites in `crates/store/tests`, burn and wire tests in
`crates/ipc/tests`, keyserver wrapped-key integration tests, and hub burn
orchestration tests all lie in the blast radius. Passing unit tests is not
enough; the acceptance evidence needs a real keyserver deployment or
production-equivalent local service and negative controls that fail under
deliberately broken behavior.

## 4. What cannot be retrofitted

Messages sent before the cutover remain decryptable forever by any holder of
the relevant recipient long-term identity keys. This includes every current v3
carrier because it already contains the body key wrapped to those identities.

A database migration cannot remove those recipient slots from ciphertext that
Discord already holds. Uploading a new service record later cannot make the old
identity-key wrapping disappear. Deleting local `wrapped_key`, adding a v5
field, receiving a burn notice, or upgrading the recipient does not change the
old carrier.

Therefore:

- v4-to-v5 migration marks all existing messages legacy/non-revocable;
- only messages created after successful capability negotiation and encoded in
  the new wire format are eligible for the proposed property;
- forwarding, editing, or re-saving an old plaintext does not upgrade the
  original message; a separately sent new-protocol message is a new object;
  and
- product copy and UI must describe the state per message, not per account,
  contact, conversation, or currently installed client version.

There is no honest backfill operation.

## 5. Mixed-version peers

A new recipient can understand the feature while an old sender still emits v3.
If the recipient UI infers revocability from its own version, a local toggle, or
the contact's current capability, it will label that v3 message as revocable
even though the carrier contains a long-term-recipient wrapping. That message
will remain decryptable from archived identity keys after any burn.

This mismatch is worse than the current honest absence. Today the product says
that burn is local destruction. A silent upgrade would create a security
promise whose truth depends on which binary sent each individual message, while
showing the recipient a stronger state that the bytes do not have. Users would
make retention and safety decisions based on a false premise.

The feature therefore requires authenticated, downgrade-resistant capability
negotiation:

- the sender selects the new format only when every required recipient/device
  has a current, verified capability bound to its trusted complete key bundle;
- absent, unknown, stale, or unverified capability is not support;
- capability state supplied by the keyserver is not trusted until its binding
  to the accepted identity is verified;
- the authenticated wire version, not the receiving app version, determines
  the per-message lifecycle label;
- a user who requires revocability gets a failed send rather than silent v3;
- a user who explicitly permits a legacy send sees it labeled as legacy before
  sending and after receiving; and
- once a peer/conversation is pinned to the new capability under a defined
  policy, an unexpected downgrade is refused or requires an explicit,
  high-friction reset with an identity/capability warning.

Groups require the intersection of all intended recipient-device capabilities.
Splitting one logical message into new-format copies for some members and v3
copies for others does not give the message one coherent burn state and is not
the default design.

## 6. Preconditions before implementation

No implementation line should be written until these are resolved in order:

1. **Freeze the property and non-goals.** Approve the definition in section 1,
   including retained-key/plaintext limits, offline behavior, attachment scope,
   and the exact user-visible distinction between local destruction, service
   revocation, and remote erasure.
2. **Close the key-substitution finding.** Bind every X25519, ML-KEM, device,
   and capability key to the already-trusted identity; require and verify the
   complete signed bundle on every fetch; define first-use and rotation
   ceremony; and independently test substitution under unchanged TOFU.
3. **Choose the custody and trust model.** Decide between single-service
   escrow, attested non-exportable hardware, independently administered
   threshold custody, or another reviewed construction. State plainly who can
   obtain content-decryption capability.
4. **Define deletion evidence.** Decide whether the accepted result is an
   honest-service attestation or a stronger assumption-backed result. Specify
   backups, replicas, rollback, logs, disaster recovery, operator access, build
   identity, attestation roots, and auditability. Do not call an ordinary
   service receipt proof.
5. **Specify authenticated capability negotiation.** Define the signed
   advertisement, binding to the trusted bundle and device set, freshness,
   pinning, downgrade handling, group policy, and legacy-send UX.
6. **Specify and review the cryptographic protocol.** Freeze algorithms, exact
   bytes, domain separation, associated data, key reference and commitment
   binding, attachment treatment, size limits, retry/idempotency inputs, and
   key zeroization expectations. Obtain external cryptographic review before
   uncontrolled traffic.
7. **Specify the keyserver state machine.** Freeze create, finalize, fetch,
   burn, tombstone, expiry, authorization, concurrency, restore, replication,
   quota, and abuse behavior, including the exact fetch/burn linearization
   point.
8. **Specify devices and caching.** Define authorization for current and future
   devices, reinstall, device loss, identity rotation, sender-history access,
   memory-cache lifetime, eviction triggers, offline errors, and outage UX.
9. **Approve the wire cutover and store v5 migration.** Assign the wire version,
   freeze legacy classification, define v4-to-v5 data transformation and
   downgrade refusal, decide the fate of `wrapped_key`, and repeat the complete
   `StoredMessage` call-site sweep.
10. **Approve failure states and claims.** Define pending, partial, attested,
    failed, and terminal UI states. A timeout, queued retry, peer notice, local
    shred, or deletion count must not be presented as stronger completion.
11. **Write the acceptance and mutation plan.** Every claimed property needs a
    negative control, mixed-version matrix, real two-identity service test, and
    intentionally broken implementation that the test rejects.

If precondition 3 retains an untrusted single keyserver and precondition 4
requires proof against that server, the requirements are inconsistent. Stop
and change the trust model or the claim; do not paper over the contradiction in
an API design.

## 7. Why this is deferred

The owner decision on 2026-07-26 was **NOT NOW; revisit after the deadline**
(`docs/design/osl-internal-build-checklist.md:80-83`). The repository-wide
truth correction and deliberate deferral are also recorded in
`docs/design/osl-master-decision-2026-07-26.md:97-109` and
`docs/reports/store-lane-2026-07-26.md:162-191`.

Deferral is the correct engineering state because:

- the required create/fetch/delete/evidence lifecycle is not wired into
  messages;
- the current keyserver is not a trustworthy deletion authority while it can
  substitute encryption keys without changing the TOFU identity;
- a service-mediated body key changes the privacy and availability model by
  placing content-decryption capability behind an online service;
- deletion against a dishonest remote operator cannot be proven by the normal
  server mechanisms currently available;
- the change spans cryptography, wire compatibility, every send/receive path,
  keyserver operations, store v5, all `StoredMessage` construction sites, burn
  orchestration, attachments, multi-device behavior, UI claims, and a large
  test matrix;
- pre-cutover history cannot benefit, so rushing the migration would create a
  mixed corpus whose security state is easy to misrepresent; and
- the current honest absence is safer than a partially wired feature that
  invites users to believe a legacy carrier is revocable.

The decision should be revisited only when the owner accepts the service
custody/availability tradeoff, the key-substitution finding is closed and
independently verified, a deletion-evidence trust model is approved, the
capability and wire protocols have external review, the v5 migration and
mixed-version UX are specified, and the project is willing to fund the
keyserver operational and end-to-end test burden.

## Status

**DESIGNED, NOT STARTED**

Owner decision, 2026-07-26: **do not start this migration.** Design it, document
the blast radius, do not execute.

That is the whole of the recorded decision. No revisit trigger, date or
condition was attached to it, and this document does not invent one — resuming
is an owner call, not a consequence of any milestone passing. Section 6 lists
the preconditions that would have to be true first; the keyserver trust problem
in §2.4 is the one that cannot be engineered around from inside this crate.
