# Storage portability: custom-server implementation spec

This is the delivery specification for replacing the Cloudflare-hosted
undelivered payload store. A custom deployment conforms only if it preserves
the authorization, deletion, and unlinkability rules below. Cloudflare is an
implementation tenant, not a protocol dependency.

The authoritative protocol is
[`storage.md`](/home/liamw/osl-plan/plan/03-CONTRACTS/storage.md), especially
sections 1b, 2, and 3. This document turns those rules into a deployable
custom-server boundary; it does not introduce alternate wire semantics.

## Deployment boundary

Run two separately operated services:

1. The **keyserver** authenticates users, enforces issuance budgets, and mints
   short-lived, single-use, claim-free Ed25519 storage grants. A grant contains
   only `{aud, exp, jti}`: never a user, device, tier, or quota field.
2. The **cipher store** accepts a valid grant at upload and handles opaque
   ciphertext, capability-authorized read, acknowledgement, burn, expiry, and
   delivery-tag matching. GET, ACK, and BURN never require an account, device,
   or session credential.

Use independently administered hosts, preferably with different providers and
jurisdictions. They may share only the grant-verification key and documented
APIs; neither receives the other's identity or payload database. This turns the
same-account Worker split into an adversarial separation.

The custom store may use an S3-compatible object store or encrypted local
files for bytes, and SQLite or Postgres for the index. Do not back up live
payload objects or index rows into a recoverable retention system if claiming
instant wipe. Moving off Cloudflare makes two properties strictly better:

- D1's mandatory Time Travel disappears for both bytes and metadata.
- The keyserver/store split becomes independently operated rather than two
  Workers in one account.

## Data and storage interfaces

Keep payload bytes and lifecycle metadata separate. The byte-store portability
seam is:

```ts
interface PayloadStore {
  put(fetchCap: string, bytes: Uint8Array): Promise<void>
  get(fetchCap: string): Promise<Uint8Array | null>
  delete(fetchCap: string): Promise<void>
  putByDigest(fetchDigest: string, bytes: Uint8Array): Promise<void>
  deleteByDigest(fetchDigest: string): Promise<void>
}
```

Its object key is lowercase-hex `SHA-256(fetch_cap)`, never `fetch_cap`
itself. The index stores no payload bytes and no capability preimages. Each
undelivered row contains:

| field | requirement |
|---|---|
| `blob_id` | 32 lowercase hex characters; path-only opaque identifier |
| `fetch_digest_sha256_hex`, `ack_digest_sha256_hex`, `manage_digest_sha256_hex` | 64 lowercase hex characters |
| `object_class` | exactly `single-ack` or `multi-fetch` |
| `pool` | exactly `undelivered` |
| `delivery_tag` | 32 lowercase hex characters; indexed but never attached to identity |
| `size_bytes`, `expires_at`, `created_at` | positive size and UTC Unix-second lifecycle fields |

`blob_id` names; it does not authorize. `fetch_cap`, `ack_cap`, and
`manage_cap` are bearer capabilities. They travel in headers, never paths or
queries, and are never logged. Store and compare only their SHA-256 digests,
using constant-time comparison. The server never receives seed `P`.

On upload, write the object first and remove it if the index transaction fails.
On ACK, BURN, or sweep, delete the object and remove the index row; retries
after partial failure must converge on absence. A successful delete must be
strongly consistent: a following read in the same operation observes no object.

## Required blob API

For GET and ACK, malformed identifiers, missing/malformed/wrong capabilities,
unknown objects, expiry, ACKed objects, and burned objects all return the same
`404` status and body. This prevents an existence and delivery oracle.

| operation | request | success | mandatory behavior |
|---|---|---|---|
| Upload | `POST /v1/blob`, valid storage grant, blob id/digest/class/tag headers, Padmé-valid ciphertext | `201` with id and expiry | authenticate only this write end; enforce single-use grant, size/capacity, class, 7-day TTL, and padding before persistence |
| Fetch | `GET /v1/blob/:blob_id`, `X-OSL-Fetch-Cap` | `200` opaque bytes, `Cache-Control: no-store` | no identity check and no mutation; all negatives are uniform `404` |
| Acknowledge | `POST /v1/blob/:blob_id/ack`, `X-OSL-Ack-Cap` | `204` | `single-ack` deletes exactly that copy; `multi-fetch` is an indistinguishable no-op; repeat ACK is idempotent |
| Burn | `DELETE /v1/blob/:blob_id`, `X-OSL-Manage-Cap` | `204`, always | delete only for matching manage capability; wrong/missing cap, absent object, and repeats disclose nothing |

The upload headers are `X-OSL-Blob-Id` (32 hex), the SHA-256 digest headers
for fetch/ack/manage (64 hex each), `X-OSL-Object-Class`, and
`X-OSL-Delivery-Tag` (32 hex). There is **no** `STATUS` endpoint, status
field, or equivalent sender-visible query.

GET is never destructive. The client fetches, decrypts, and persists locally
when the pointer arrives, then ACKs only after the local write succeeds. The
server must never delete on fetch or transmission.

BURN has no timestamp, signature-freshness rule, or clock window. Its
non-expiring `manage_cap` permits a queued offline burn to be sent unchanged
after reconnect. The client marks server burn complete only after `204`; it is
pending while the queue is undrained.

## Lifecycle, quota, and notification obligations

The undelivered pool is standalone. It must never evict a live object for any
capacity need, including a retained-history pool. Refuse writes at the global
cap with `503`; clients queue and retry. For v1, the undelivered window is
exactly seven days (604800 seconds), as both floor and ceiling. Expiry is
checked on reads and physically swept, so scheduler delay cannot make expired
ciphertext fetchable.

Run an idempotent, crash-tolerant periodic sweeper over an `expires_at` index.
It must tolerate at-least-once execution, concurrent ACK/BURN, and a crash
between byte and index deletion. Retained history is a separate authenticated,
per-owner store; it may evict oldest-first under its own budget, but never
shares capacity or eviction paths with undelivered blobs.

The cipher store indexes `delivery_tag` but retains no account or conversation
association. A realtime service subscribes connections to opaque rotating tags
and emits fixed-size `{tag, blob_id}` wakeup frames on a match. A frame grants
no fetch authority and proves no delivery: the recipient still needs the
separately delivered pointer seed. The transport owns connection cadence and
frame padding; the store preserves the opaque-tag boundary.

## Replacing Cloudflare mechanics

| Cloudflare facility | custom-server obligation |
|---|---|
| R2 payload bucket | byte store with strongly consistent delete and no cache that serves deleted payloads |
| D1 metadata index | SQLite/Postgres transactionally maintaining the fields above, with expiry and delivery-tag indexes |
| Cron Trigger | supervised scheduler for idempotent expiry sweep |
| Durable Object alarm | `next_wake_at`/`expires_at` index plus a sweeper scheduling the earliest retained record |
| Durable Object serialisation | database transaction or per-owner advisory lock |
| Workers rate-limit/KV binding | Redis or equivalent token bucket; fetch-side limiting is cost control and fails open |

Platform SDK calls belong only in the byte-store adapter and retained-pool
scheduling wrapper. Capability checks, digest comparison, Padmé validation,
TTL bounds, quota arithmetic, and sweep policy must run in plain Node or the
chosen ordinary runtime with no Cloudflare bindings. Maintain a plain-runtime
test gate that exercises those policies and fails on platform dependencies.

## Migration acceptance checklist

- [ ] A post-delete read cannot retrieve payload bytes or metadata.
- [ ] A database/object-store dump exposes only ciphertext and capability
  digests, never bearer-capability preimages.
- [ ] GET and ACK negatives are indistinguishable `404`; BURN is unconditional
  `204`; no STATUS-like surface exists.
- [ ] Wrong fetch, ack, and manage capabilities do not authorize their effects;
  wrong BURN changes no stored object.
- [ ] `single-ack` deletes only its own persisted copy; `multi-fetch` returns
  the same `204` without deletion.
- [ ] Expired, ACKed, and burned objects remain inaccessible after sweeper
  retries and process restarts.
- [ ] A BURN queued for days completes idempotently with no freshness check.
- [ ] Filling retained storage changes no undelivered row; a full undelivered
  store refuses writes instead of evicting.
- [ ] The plain-runtime gate covers padding, TTL, digest comparison, and quota
  arithmetic without Worker bindings.
