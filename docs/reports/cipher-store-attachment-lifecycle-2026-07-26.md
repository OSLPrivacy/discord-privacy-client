# Cipher-store attachment lifecycle

This document describes the attachment implementation that exists in source. It
does not promote comments, test names, or constant names to contracts when the
executed code says something else.

## Stored model

Migration `0003` is not the current attachment schema. It created a direct-upload
table capped at 26 MiB and stored the bearer token itself
(`cipher-store-cf/migrations/0003_r2_attachments.sql:5-11`). Migration `0004`
drops that table rather than copying it, then creates the multipart schema
(`cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:5-22`).
Migration `0006` adds `content_expires_at` and backfills it from `expires_at`
(`cipher-store-cf/migrations/0006_session_budget_and_atomic_rate_counters.sql:34-38`).

The resulting `attachment_objects` row contains:

- a 32-character lowercase-hex ID and a unique R2 object key;
- a declared or actual positive size no greater than 537,919,488 bytes;
- `expires_at`, `content_expires_at`, and `created_at`;
- the 64-character lowercase-hex SHA-256 digest of the 16-byte fetch/delete
  capability, not the capability itself;
- one of `uploading`, `completing`, or `ready`; and
- an R2 multipart upload ID in the first two states, and `NULL` in `ready`.

Those are actual schema checks
(`cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:8-22`).
There is no CHECK on either expiry, no CHECK that
`content_expires_at` is non-NULL, and no CHECK relating the two times.

`attachment_parts` is subordinate metadata. Its foreign key is deleted with the
object row, part numbers are 1 through 65, part sizes are 1 through 8,388,608
bytes, and an ETag is either NULL or 1 through 256 characters
(`cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:31-37`).
The table does not contain part bytes. Part bytes and the assembled object live
in R2.

The Worker accepts only TTLs of 3,600, 86,400, 259,200, or 604,800 seconds
(`cipher-store-cf/src/endpoints/attachment.ts:32-39`). The capability header is
`X-OSL-Fetch-Token`; it must decode from exactly 32 hex characters. The Worker
normalizes the header to lowercase, hashes the represented 16 bytes, and
compares that digest with the row
(`cipher-store-cf/src/endpoints/attachment.ts:42-64`,
`cipher-store-cf/src/endpoints/attachment.ts:155-173`). The same capability
authorizes upload parts, completion, fetch, and deletion. It identifies no
person or account.

## State machine

The multipart state machine is:

```text
no row
  |
  | create R2 multipart upload, then conditional D1 insert
  v
uploading -----------------------------+
  |                                    |
  | all numbered parts have ETags,     | accepted part
  | are contiguous, and total exactly  | (state stays uploading)
  | equals declared size               |
  v                                    |
completing ----------------------------+
  |       |
  |       | R2 complete throws and the compensating UPDATE succeeds
  |       +----------------------------------------------> uploading
  |
  | R2 complete succeeds, its size matches, and D1 metadata batch succeeds
  v
ready
```

Direct upload bypasses the multipart states: it writes the complete R2 object
first and then conditionally inserts one `ready` row
(`cipher-store-cf/src/endpoints/attachment.ts:490-519`).

### Session creation: no row to `uploading`

`POST /v1/attachment/session` requires an allowed TTL, a valid capability, and
`X-OSL-Size-Bytes` from 1 through 537,919,488
(`cipher-store-cf/src/endpoints/attachment.ts:249-257`). A non-authoritative
read first rejects an obviously exhausted incomplete-session pool. The Worker
then creates the R2 multipart upload before attempting the authoritative D1
insert (`cipher-store-cf/src/endpoints/attachment.ts:259-272`).

The inserted row is `uploading`, stores the declared size, stores the R2 upload
ID, sets `created_at` to the current second, sets `expires_at` to 15 minutes
after that second, and sets `content_expires_at` to the current second plus the
requested content TTL (`cipher-store-cf/src/endpoints/attachment.ts:263-282`).
The response is HTTP 201 with the ID, the declared size, 8 MiB part size, 65
maximum parts, and `expires_at` equal to **the stored
`content_expires_at`**, not the row's current `expires_at`
(`cipher-store-cf/src/endpoints/attachment.ts:291-301`).

If the conditional insert refuses capacity, the handler aborts the newly
created multipart upload and returns 503. If the insert throws, it makes a
best-effort abort and rethrows (`cipher-store-cf/src/endpoints/attachment.ts:271-290`).

### Part upload: `uploading` stays `uploading`

`PUT /v1/attachment/:id/part/:number` accepts part numbers only from 1 through
65 and only while the authorized row is unexpired, `uploading`, and has an
upload ID (`cipher-store-cf/src/endpoints/attachment.ts:304-317`). The number
must also be no greater than `ceil(declared_size / 8 MiB)`. Every non-final part
must be exactly 8 MiB; the final part must be exactly the declared remainder.
`Content-Length` is mandatory and must state that exact length
(`cipher-store-cf/src/endpoints/attachment.ts:318-332`).

Before reading the request body, one conditional UPSERT reserves that part
number with a NULL ETag. It permits the sum of this part and every other part
reservation to be no greater than the object's declared size. Retrying a part
replaces that part's prior reservation and clears its old ETag
(`cipher-store-cf/src/endpoints/attachment.ts:334-361`).

The Worker then buffers and counts the entire part, up to 8 MiB, before calling
R2 (`cipher-store-cf/src/endpoints/attachment.ts:364-380`). A body larger than
the bound, empty, or different from `Content-Length` causes a best-effort R2
abort followed by deletion of the object row; the parts disappear through the
foreign key (`cipher-store-cf/src/endpoints/attachment.ts:367-377`). A valid R2
part receipt has its ETag stored in D1. Only after that write does the Worker
slide `expires_at` forward to the lesser of:

- 15 minutes after the accepted part; and
- the already-stored promised `content_expires_at`.

The UPDATE only moves the deadline forward and only while the row is still
`uploading` (`cipher-store-cf/src/endpoints/attachment.ts:379-397`).

R2 or D1 failure after the reservation leaves the row and its short reclaim
deadline in place. The client may retry, delete it, or leave it for the expiry
sweep (`cipher-store-cf/src/endpoints/attachment.ts:398-402`).

### Completion: `uploading` to `completing` to `ready`

`POST /v1/attachment/:id/complete` first loads all part rows with non-NULL
ETags. It refuses completion unless at least one exists, their sizes total
exactly the declared object size, and their ordered part numbers are exactly
1, 2, ... without a gap (`cipher-store-cf/src/endpoints/attachment.ts:408-425`).

One conditional UPDATE claims the row by changing `uploading` to `completing`.
A concurrent loser receives 409
(`cipher-store-cf/src/endpoints/attachment.ts:426-429`). Fetch remains closed
in `completing`.

The Worker asks R2 to complete the upload with the stored part-number/ETag
pairs. If R2 throws, the Worker attempts to return the row to `uploading`; a
failure of that compensating UPDATE is swallowed, so the row can remain
`completing` (`cipher-store-cf/src/endpoints/attachment.ts:431-443`). If R2
returns an object whose size differs from the declared size, the Worker makes a
best-effort object deletion, deletes the D1 row, and returns 500
(`cipher-store-cf/src/endpoints/attachment.ts:444-447`).

For a matching R2 object, a D1 batch changes the row to `ready`, clears
`upload_id`, copies the stored promised expiry into `expires_at`, and deletes
the part rows. It returns HTTP 201 with that same promised expiry
(`cipher-store-cf/src/endpoints/attachment.ts:451-461`). A later completion
request against `ready` is idempotent at the API level and returns the same
receipt with HTTP 200 (`cipher-store-cf/src/endpoints/attachment.ts:408-414`).

If R2 completion succeeds but the final D1 batch throws, the row remains
`completing` as far as this function is concerned. There is no route that
promotes a `completing` row to `ready`. Fetch returns 404, while authenticated
delete and the sweep can still discover and remove the completed R2 object
through `HEAD` (`cipher-store-cf/src/endpoints/attachment.ts:462-465`,
`cipher-store-cf/src/endpoints/attachment.ts:556-571`).

### Direct upload: no row to `ready`

`POST /v1/attachment` permits an absent `Content-Length`, but if supplied it
must be an unsigned nonzero value no greater than 26 MiB. The actual body is
always buffered and counted under the same 26 MiB ceiling, and a supplied
length must match (`cipher-store-cf/src/endpoints/attachment.ts:469-488`).

The Worker conditionally puts the bytes in R2, refusing to overwrite an existing
key. It verifies the returned R2 size, then inserts a `ready` row whose
`expires_at` and `content_expires_at` are both the current second plus the TTL
(`cipher-store-cf/src/endpoints/attachment.ts:490-514`). The content clock
therefore starts when the body has reached R2, immediately before the D1 insert.
If quota rejects the insert, the Worker deletes the R2 object. Other exceptions
also trigger a best-effort R2 delete (`cipher-store-cf/src/endpoints/attachment.ts:515-524`).

### Fetch and explicit delete

Authorization treats a row with `expires_at <= now` as absent before checking
the presented capability (`cipher-store-cf/src/endpoints/attachment.ts:155-173`).
Fetch then requires `ready`, requires an R2 object whose size equals the row,
and returns the R2 body as `application/octet-stream` with an exact
`Content-Length` and `Cache-Control: no-store`
(`cipher-store-cf/src/endpoints/attachment.ts:528-541`). `uploading` and
`completing` are deliberately indistinguishable from missing content at fetch.

Delete is idempotent only for a syntactically valid ID that authorization
reported missing: it returns 204 without touching R2 or D1. Otherwise it
requires the same capability, removes R2 state first, then deletes the matching
D1 row (`cipher-store-cf/src/endpoints/attachment.ts:544-553`). Consequently,
an expired row cannot be explicitly reclaimed through this route:
authorization hides it, and the 204 path performs no cleanup. The cron must
reclaim it.

For a row with an upload ID, storage removal first uses R2 `HEAD`. If an object
exists, it deletes that object. If no object exists, it resumes and aborts the
multipart upload, then performs an object delete. A ready row skips the
multipart branch and deletes the object
(`cipher-store-cf/src/endpoints/attachment.ts:556-571`). This is also how a
`completing` row is handled safely on either side of the R2-complete/D1-ready
crash window.

## The two expiry times

`expires_at` is the **reclaim deadline used by authorization and the sweep**.
`content_expires_at` is the **absolute expiry promised in the session receipt**.
They are not two names for the same clock.

For multipart creation at time `S`:

```text
content_expires_at = S + requested TTL
expires_at         = S + 900 seconds
```

They initially diverge because every accepted content TTL is at least one hour.
Each accepted part may move `expires_at` to `min(part_time + 900,
content_expires_at)`. They can therefore converge before completion when an
upload is within 15 minutes of its promised expiry
(`cipher-store-cf/src/endpoints/attachment.ts:385-396`). Successful completion
converges them by copying `content_expires_at` into `expires_at`; it does not
compute a new expiry (`cipher-store-cf/src/endpoints/attachment.ts:449-461`).
Direct upload writes the same `now + TTL` into both fields from the start
(`cipher-store-cf/src/endpoints/attachment.ts:501-514`). Migration `0006`
converged all pre-existing rows by backfilling `content_expires_at` from
`expires_at`
(`cipher-store-cf/migrations/0006_session_budget_and_atomic_rate_counters.sql:34-38`).

The promised value must be stored. The only client sends the TTL at session
creation, receives an absolute `expires_at`, and later rejects the completion
receipt unless its expiry is exactly equal to the session receipt
(`crates/ipc/src/cipher_store_client.rs:467-481`,
`crates/ipc/src/cipher_store_client.rs:528-545`). Recomputing `now + TTL` at
completion would produce a later timestamp and make the client treat a
successful upload as corrupt. The multipart session, part, and completion
response structs also use `deny_unknown_fields`, so the Worker cannot expose the
internal reclaim deadline by adding another response member without making the
client reject the response
(`crates/ipc/src/cipher_store_client.rs:213-235`). Storing the absolute promise
allows both receipts to remain byte-for-byte consistent in meaning while D1
uses a different internal deadline during upload.

The practical promise is availability until that timestamp, not physical
deletion at that timestamp. Authorization closes at `expires_at <= now`, while
the sweep selects only `expires_at < now` and runs on a five-minute cron
(`cipher-store-cf/src/endpoints/attachment.ts:166`,
`cipher-store-cf/src/lib/sweep.ts:75-82`,
`cipher-store-cf/wrangler.toml:88-92`).

## Bounds and quotas

All byte units below are binary units because every constant multiplies by
1024.

| Constant | Value | What actually enforces it |
|---|---:|---|
| `MAX_DIRECT_ATTACHMENT_BYTES` | 26 MiB / 27,262,976 bytes | Direct upload parses a supplied length against it and independently buffers/counts the body against it (`cipher-store-cf/src/endpoints/attachment.ts:469-488`). The Rust client selects multipart only when the file is larger than this value (`crates/ipc/src/cipher_store_client.rs:435-456`). |
| `MAX_PLAINTEXT_ATTACHMENT_BYTES` | 512 MiB / 536,870,912 bytes | Nothing. It is declared but never imported or referenced outside its defining file (`cipher-store-cf/src/lib/attachment-limits.ts:2`). The store sees opaque sealed bytes and cannot infer plaintext size. |
| `MAX_SEALED_ATTACHMENT_BYTES` | 513 MiB / 537,919,488 bytes | Session creation rejects a larger declared size (`cipher-store-cf/src/endpoints/attachment.ts:249-257`); the final schema CHECK rejects a larger object row (`cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:8-13`); and the Rust client rejects a larger file before network I/O (`crates/ipc/src/cipher_store_client.rs:424-440`). |
| `MAX_ATTACHMENT_PART_BYTES` | 8 MiB / 8,388,608 bytes | Part `Content-Length`, actual buffered bytes, fixed non-final-part sizing, and the parts-table CHECK enforce it (`cipher-store-cf/src/endpoints/attachment.ts:318-377`, `cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:31-35`). The client refuses a session advertising any other value (`crates/ipc/src/cipher_store_client.rs:476-485`). |
| `MAX_ATTACHMENT_PARTS` | 65 | The part route rejects numbers outside 1-65, the schema CHECK repeats 1-65, and completion requires contiguous numbering (`cipher-store-cf/src/endpoints/attachment.ts:304-320`, `cipher-store-cf/src/endpoints/attachment.ts:416-425`, `cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:31-34`). The client requires the server to advertise 65 (`crates/ipc/src/cipher_store_client.rs:476-485`). |
| `MAX_LIVE_ATTACHMENT_ROWS` | 512 rows | The conditional object INSERT requires `COUNT(*) < 512` before inserting (`cipher-store-cf/src/endpoints/attachment.ts:200-230`). It counts every row, including expired-but-unswept and incomplete rows. |
| `MAX_LIVE_ATTACHMENT_BYTES` | 8 GiB / 8,589,934,592 bytes | The same conditional INSERT requires the current `SUM(size_bytes)` to be no greater than `8 GiB - new_size` (`cipher-store-cf/src/endpoints/attachment.ts:200-230`). For incomplete rows, `size_bytes` is a declaration/reservation, not bytes already stored. |
| `INCOMPLETE_SESSION_TTL_SECONDS` | 900 seconds | Session creation writes `now + 900`; an accepted part may write `min(now + 900, promised expiry)` (`cipher-store-cf/src/endpoints/attachment.ts:263-282`, `cipher-store-cf/src/endpoints/attachment.ts:385-396`). The sweep is the physical enforcer. |
| `MAX_INCOMPLETE_SESSION_ROWS` | 64 rows | A read-only preflight can reject early, but the authoritative enforcement is the same conditional object INSERT, whose `state <> 'ready'` subquery requires the count to be below 64 (`cipher-store-cf/src/endpoints/attachment.ts:236-246`, `cipher-store-cf/src/endpoints/attachment.ts:200-230`). |
| `MAX_INCOMPLETE_SESSION_BYTES` | 2 GiB / 2,147,483,648 bytes | The preflight estimates it; the authoritative conditional INSERT requires the incomplete declared-size sum to be no greater than `2 GiB - new_size` (`cipher-store-cf/src/endpoints/attachment.ts:236-246`, `cipher-store-cf/src/endpoints/attachment.ts:200-230`). |
| `ATTACHMENT_SWEEP_BATCH_SIZE` | 100 rows | It limits each expiry SELECT and R2 bulk-delete batch. D1 metadata deletion is separately chunked to 90 IDs plus the expiry parameter (`cipher-store-cf/src/lib/sweep.ts:15-17`, `cipher-store-cf/src/lib/sweep.ts:75-107`). |

The 512-row/8-GiB global backstop and the 64-row/2-GiB incomplete pool are
enforced together by **one SQL statement**: the conditional
`INSERT ... SELECT ... WHERE` in `insertObject`
(`cipher-store-cf/src/endpoints/attachment.ts:200-230`). There is no aggregate
quota CHECK or trigger in the schema. A direct `ready` insert skips only the
incomplete-pool predicate; it never skips the global predicates
(`cipher-store-cf/src/endpoints/attachment.ts:207-229`). A multipart session
must pass both.

The part UPSERT is a separate declared-size guard. It prevents reserved part
sizes from summing above the object's declaration; it does not enforce either
storage pool (`cipher-store-cf/src/endpoints/attachment.ts:334-359`).

## Rate-limit gates

The router charges a bucket before calling the attachment handler:

| Route | Bucket | Budget per aligned hour | Store and failure posture |
|---|---|---:|---|
| `POST /v1/attachment` | `attachment-upload` | 140 | Atomic D1, fail closed |
| `POST /v1/attachment/session` | `attachment-session` | 24 | Atomic D1, fail closed |
| `PUT /v1/attachment/:id/part/:n` | `attachment-upload` | shared 140 | Atomic D1, fail closed |
| `POST /v1/attachment/:id/complete` | `attachment-upload` | shared 140 | Atomic D1, fail closed |
| `GET /v1/attachment/:id` | `attachment-fetch` | 120 | KV read/modify/write, eventually consistent, fail open |
| `DELETE /v1/attachment/:id` | `attachment-delete` | 60 | Atomic D1, fail closed |

The route-to-bucket mapping is in
`cipher-store-cf/src/index.ts:166-211`; the budgets and mutation set are in
`cipher-store-cf/src/lib/rate-limit.ts:53-98`. Mutation admission is one
conditional D1 UPSERT with `RETURNING used`
(`cipher-store-cf/src/lib/rate-limit.ts:157-175`). Read admission uses KV and
can over-admit under concurrency (`cipher-store-cf/src/lib/rate-limit.ts:178-194`).

The window is not rolling. It is an epoch-aligned one-hour window:
`windowStart = now - (now % 3600)`. The client address is HMACed together with
the bucket name, and the window start is included in the stored key
(`cipher-store-cf/src/lib/rate-limit.ts:100-147`). Because charging occurs in
the router, requests that match a route but later fail endpoint validation
still consume the corresponding budget.

## Reclamation, residue, and orphans

The five-minute cron invokes `sweepExpiredAttachments` independently of the
other sweep jobs (`cipher-store-cf/src/index.ts:72-96`). It repeatedly selects
up to 100 rows with `expires_at < now`, ordered oldest first. Rows with no
upload ID are bulk-deleted from R2. Rows with an upload ID go through
`removeAttachmentStorage`, which distinguishes a completed object from a live
multipart upload with `HEAD`. Only after R2 cleanup does D1 delete metadata, in
chunks of 90 IDs plus the expiry bind
(`cipher-store-cf/src/lib/sweep.ts:75-107`).

The ordering is deliberate and observable: an R2 failure leaves an indexed row
that a later cron can retry. A D1 failure after R2 cleanup leaves metadata
pointing at absent storage; a later cron or authorized delete can retry the
idempotent storage removal before deleting the row. Because the object-row
delete cascades, successful metadata reclamation also removes part receipts.

Every failure class leaves the following state:

| Failure point | State left behind | What can reclaim it |
|---|---|---|
| Request validation before R2/D1 allocation | Nothing | Nothing is required. |
| R2 multipart creation fails | No row and no upload ID known to D1 | R2 owns the failed call's semantics; the Worker has nothing to sweep. |
| Session quota insert refuses and R2 abort succeeds | Nothing | Nothing is required (`cipher-store-cf/src/endpoints/attachment.ts:271-286`). |
| Session insert/digest throws and abort fails, or quota refusal's awaited abort fails | No D1 row, but the R2 multipart upload may remain | No code path can reclaim it because the upload ID was never indexed. This is an R2 orphan (`cipher-store-cf/src/endpoints/attachment.ts:271-290`). |
| Part body stream throws while it is being buffered | `uploading` row plus a reserved part with NULL ETag | Client best-effort delete if it receives an error; otherwise the short `expires_at` sweep. |
| Part is oversized, empty, or length-mismatched | Handler tries to abort R2 and then deletes the row | A failed abort is swallowed. If row deletion succeeds after that failure, the multipart upload is an unindexed R2 orphan. If row deletion throws, indexed residue remains for client deletion or the sweep (`cipher-store-cf/src/endpoints/attachment.ts:367-377`). |
| R2 `uploadPart`, ETag UPDATE, or deadline UPDATE throws | `uploading` row remains. It may have a NULL ETag reservation, an uploaded R2 part, or a stored ETag without the deadline slide, depending on the failing operation | The Rust client attempts delete after any multipart error; otherwise explicit delete or the expiry sweep (`cipher-store-cf/src/endpoints/attachment.ts:379-402`, `crates/ipc/src/cipher_store_client.rs:495-555`). |
| Completion validation fails before claim | `uploading` unchanged | Caller can upload/retry missing parts, explicitly delete, or wait for sweep. |
| R2 completion throws and reversal succeeds | `uploading`; R2 may still contain multipart state | Retry, explicit delete, or sweep. |
| R2 completion throws and reversal also fails | `completing` | It cannot complete through the API. Explicit delete or sweep is required (`cipher-store-cf/src/endpoints/attachment.ts:431-443`). |
| R2 completion reports the wrong size | Handler attempts object delete, then row delete | If object deletion fails but row deletion succeeds, the completed R2 object is unindexed and no sweep can find it. If row deletion fails, the `completing` row remains retryable by delete/sweep (`cipher-store-cf/src/endpoints/attachment.ts:444-447`). |
| R2 completion succeeds and final D1 batch throws | `completing` row with an upload ID; completed R2 object may exist | `HEAD` lets explicit delete or sweep remove the completed object, then the row (`cipher-store-cf/src/endpoints/attachment.ts:451-465`, `cipher-store-cf/src/endpoints/attachment.ts:556-571`). |
| Direct R2 put succeeds but D1 insert refuses or throws | Handler attempts R2 delete | If that delete also fails, a complete R2 object exists without a D1 row and cannot be swept (`cipher-store-cf/src/endpoints/attachment.ts:490-524`). |
| Fetch sees missing R2 data or a size mismatch | `ready` row remains, fetch returns 404 | Authorized delete or expiry sweep. |
| Explicit delete removes R2 but D1 delete throws | Indexed metadata remains with missing storage | Retry delete while unexpired, or the sweep after expiry (`cipher-store-cf/src/endpoints/attachment.ts:544-553`). |
| Row reaches its deadline before explicit delete | Authorization returns 404/204 without cleanup | Only the expiry sweep reclaims it (`cipher-store-cf/src/endpoints/attachment.ts:166`, `cipher-store-cf/src/endpoints/attachment.ts:544-548`). |
| Sweep fails during R2 cleanup | Some earlier objects in the selected batch may already be absent; all not-yet-deleted D1 rows remain | Next cron invocation. |
| Sweep fails during a D1 delete chunk | R2 cleanup has already run; rows not deleted by completed chunks remain | Next cron invocation. |

An R2 object or multipart upload with no `attachment_objects` row is a permanent
orphan as far as this code is concerned: every normal removal path starts from
D1 metadata. Conversely, a D1 row whose R2 state is gone is visible to the
sweep and does not become an unbounded storage orphan, although it consumes
row/byte quota until metadata deletion succeeds.

## What the only client sends and accepts

`CipherStoreClient::upload_attachment_file` rejects an unsupported TTL, an
empty file, or a file larger than 537,919,488 bytes before sending a request. It
seeks the file to byte zero. Files up to and including 26 MiB use direct upload;
larger files use multipart (`crates/ipc/src/cipher_store_client.rs:424-456`).

For direct upload the client sends:

- `POST /v1/attachment`;
- `Content-Type: application/octet-stream`;
- exact `Content-Length`;
- `X-OSL-TTL-Seconds`;
- lowercase-hex `X-OSL-Fetch-Token`; and
- a sized streaming body backed by the sealed file.

It accepts any 2xx response whose first 4 KiB parse as JSON containing a
32-character lowercase-hex `id` and an integer `expires_at`
(`crates/ipc/src/cipher_store_client.rs:446-456`,
`crates/ipc/src/cipher_store_client.rs:1112-1142`). It does **not** require or
validate `size_bytes`, does not require a positive expiry, and ignores unknown
fields on the direct response.

For multipart the client sends a zero-length session POST with TTL, capability,
and exact sealed size. It accepts only a 2xx JSON object with exactly the five
declared session fields. The ID must be 32 lowercase-hex characters, expiry
must be positive, returned size must equal the file size, part size must be
exactly 8 MiB, and maximum parts must be exactly 65
(`crates/ipc/src/cipher_store_client.rs:459-487`).

It derives contiguous 8 MiB parts, clones and seeks the file for each part, and
sends each as a sized PUT with content type, exact content length, and the same
capability. Each strict part receipt must contain exactly the requested part
number and length (`crates/ipc/src/cipher_store_client.rs:488-520`). After all
parts, it checks that file metadata still reports the original length, then
sends a zero-length completion POST. The strict completion receipt must contain
exactly the session ID, the session's promised expiry, and the original size
(`crates/ipc/src/cipher_store_client.rs:522-550`).

Once a multipart session response has been parsed into a valid ID, any later
error triggers a best-effort DELETE. A session response that fails strict JSON
deserialization or contains a malformed ID returns before that cleanup is
available, so its server row is left to the short sweep. Direct upload errors
after the server has committed but before the response is accepted likewise
have no client cleanup path because no `UploadResult` is returned
(`crates/ipc/src/cipher_store_client.rs:476-493`,
`crates/ipc/src/cipher_store_client.rs:551-555`).

Fetch validates the ID locally, sends GET with the capability, maps 404 and 429
to dedicated errors, rejects a declared response length of zero or more than
513 MiB, and copies at most 513 MiB plus one byte into the caller's writer. It
rejects zero actual bytes and an actual body above the bound, but by then it may
have written the bounded prefix; the caller is required to remove its staging
file on error (`crates/ipc/src/cipher_store_client.rs:558-604`). Delete validates
the ID locally, sends DELETE with the capability, maps 429, and accepts any
other 2xx response (`crates/ipc/src/cipher_store_client.rs:607-625`).

## WHERE THE CODE DISAGREES WITH ITSELF

These are contradictions in the current tree, not cautions.

1. **The multipart content TTL does not start at completion.** The comment says
   completion is where it starts
   (`cipher-store-cf/src/endpoints/attachment.ts:449-450`), and a test is titled
   “starts the caller's content TTL only once the upload completes” and later
   says the row “only now” holds that TTL
   (`cipher-store-cf/test/attachment-session-budget.test.ts:109`,
   `cipher-store-cf/test/attachment-session-budget.test.ts:144-157`). The code
   computes the absolute promise at session creation
   (`cipher-store-cf/src/endpoints/attachment.ts:263-277`) and completion merely
   copies it (`cipher-store-cf/src/endpoints/attachment.ts:451-458`). The comment
   and test title are wrong. A slow upload receives less ready-state lifetime,
   not a fresh TTL.

2. **“Streamed upload” is false on the Worker side.** The module header claims
   “bounded streamed parts”
   (`cipher-store-cf/src/endpoints/attachment.ts:1-6`), and the R2 test says
   “streams upload and fetch”
   (`cipher-store-cf/test/attachment-r2.test.ts:82-84`). Both direct bodies and
   multipart parts are fully accumulated into a `Uint8Array` before R2 sees
   them (`cipher-store-cf/src/endpoints/attachment.ts:110-134`,
   `cipher-store-cf/src/endpoints/attachment.ts:364-380`,
   `cipher-store-cf/src/endpoints/attachment.ts:478-493`). Fetch streams; upload
   does not. The header and test title are wrong.

3. **The aggregate quotas are neither CHECK constraints nor a D1 trigger.**
   `attachment-limits.ts` says the 512-row/8-GiB values are duplicated as CHECK
   constraints and that a D1 trigger is authoritative
   (`cipher-store-cf/src/lib/attachment-limits.ts:11-14`). Migration `0004`
   explicitly says conditional Worker statements enforce the aggregates and
   creates no trigger
   (`cipher-store-cf/migrations/0004_attachment_capability_digests_and_quota.sql:27-37`).
   The only authority is the Worker's conditional insert
   (`cipher-store-cf/src/endpoints/attachment.ts:200-230`). The comment is wrong.

4. **`MAX_PLAINTEXT_ATTACHMENT_BYTES` enforces nothing.** It is declared as 512
   MiB and never used (`cipher-store-cf/src/lib/attachment-limits.ts:2`). The
   operative server and client maximum is the 513 MiB sealed-byte constant. The
   plaintext-named constant is dead code, not a limit.

5. **The incomplete pool does not fit four maximum-size sealed uploads.** The
   comment says four concurrent “full-size” uploads fit
   (`cipher-store-cf/src/lib/attachment-limits.ts:30-34`). A maximum sealed
   upload is 513 MiB, and `floor(2 GiB / 513 MiB)` is three. Four fit only if
   “full-size” silently means the unenforced 512 MiB plaintext constant. The
   comment is wrong for the bytes the pool actually reserves.

6. **An active incomplete session can be cut off.** The 15-minute constant's
   comment says accepted progress means a genuine slow upload is “never cut
   off” (`cipher-store-cf/src/lib/attachment-limits.ts:25-28`). The slide is
   explicitly capped at the session's original promised expiry
   (`cipher-store-cf/src/endpoints/attachment.ts:385-396`). Progress cannot
   extend a session beyond that absolute time. “Never” is wrong.

7. **The `MAX_LIVE_*` quotas are not live-only.** Their SQL counts and sums the
   entire `attachment_objects` table, without an expiry predicate and including
   incomplete declared reservations
   (`cipher-store-cf/src/endpoints/attachment.ts:200-211`). Expired rows consume
   the “live” quota until a sweep deletes them. The names deny what the SQL
   measures.

8. **The limiter is a fixed window, not a rolling window.** The file describes
   budgets “per rolling window”
   (`cipher-store-cf/src/lib/rate-limit.ts:22-28`) but snaps every request to an
   epoch-aligned hour (`cipher-store-cf/src/lib/rate-limit.ts:140-147`). A caller
   can spend one whole budget immediately before the boundary and another
   immediately after it. The prose is wrong.

9. **The session-budget arithmetic in the limiter comment is wrong.** It says
   24 session creations permit “twelve full-size uploads” and that filling a
   64-slot reservation pool costs at least eleven client addresses
   (`cipher-store-cf/src/lib/rate-limit.ts:81-88`). One upload creates one
   session, so 24 permits 24 creation attempts per address. Sixty-four session
   requests require three 24-request addresses, not eleven; and the 2-GiB byte
   pool is exhausted by one address long before 64 maximum-size sessions. The
   comment's numbers do not follow from either enforced quota.

10. **Expiry availability and sweep eligibility disagree at the boundary.**
    Authorization hides `expires_at == now`
    (`cipher-store-cf/src/endpoints/attachment.ts:166`), while the sweep deletes
    only `expires_at < now`
    (`cipher-store-cf/src/lib/sweep.ts:79-82`). At the exact second of expiry the
    row is unavailable but not sweep-eligible. One of the inequalities is wrong
    if those operations are meant to share a boundary.

11. **`sweepExpiredAttachments` reports selected rows, not rows proved
    deleted.** It increments `deleted` by `rows.length` after D1 DELETEs but
    never reads affected-row counts
    (`cipher-store-cf/src/lib/sweep.ts:97-109`). A concurrent change can make
    the guarded DELETE affect fewer rows than selected. The function name,
    variable, and return description overclaim the evidence they hold.

12. **The R2 “double” test no longer tests a double.** The file and `describe`
    text repeatedly call it a strict double
    (`cipher-store-cf/test/harness-strictness.test.ts:1-12`,
    `cipher-store-cf/test/harness-strictness.test.ts:28`), but the test imports
    `env` from `cloudflare:test` and exercises `env.ATTACHMENTS`, the real
    workerd R2 binding (`cipher-store-cf/test/harness-strictness.test.ts:14-15`,
    `cipher-store-cf/test/harness-strictness.test.ts:29-60`). The title and
    introductory prose are stale.

13. **The previously reported 101-bind sweep defect is not present in this
    source snapshot.** `ATTACHMENT_SWEEP_BATCH_SIZE` remains 100
    (`cipher-store-cf/src/lib/attachment-limits.ts:36`), but metadata deletion
    now slices IDs into groups of 90 and binds one expiry plus at most 90 IDs
    (`cipher-store-cf/src/lib/sweep.ts:15-17`,
    `cipher-store-cf/src/lib/sweep.ts:97-105`). Current code binds at most 91
    parameters to that statement. Describing it as still binding 101 would be
    inaccurate.

14. **The public attachment result's ID documentation is wrong for
    attachments.** `UploadResult.id_hex` says it is 16 hex characters / 8 random
    bytes (`crates/ipc/src/cipher_store_client.rs:160-166`). Both attachment
    paths require and return 32 hex characters
    (`crates/ipc/src/cipher_store_client.rs:446-456`,
    `crates/ipc/src/cipher_store_client.rs:476-477`). The shared struct's
    documentation describes legacy blobs, not attachment results.

15. **Not every attachment response rejects unknown fields.** The multipart
    structs do (`crates/ipc/src/cipher_store_client.rs:213-235`), but direct
    upload parses into `serde_json::Value`, extracts two members, and ignores
    everything else (`crates/ipc/src/cipher_store_client.rs:1112-1142`). Any
    blanket claim that the attachment client's responses are all
    `deny_unknown_fields` is wrong.

16. **The client does not detect an attachment changing during upload.** Its
    error text says “sealed attachment changed during upload,” but the only
    post-upload comparison is file length
    (`crates/ipc/src/cipher_store_client.rs:522-526`). Same-length replacement
    or mutation is accepted. The error text claims a stronger integrity check
    than the code performs.

17. **The expiry field is not the second at which the server deletes the
    object.** `UploadResult.expires_at` says exactly that
    (`crates/ipc/src/cipher_store_client.rs:165-166`). The server denies access
    at that second, but physical deletion waits until a later five-minute sweep
    and uses a strict-less-than comparison
    (`cipher-store-cf/src/endpoints/attachment.ts:166`,
    `cipher-store-cf/src/lib/sweep.ts:75-82`). The field is an access-expiry
    promise, not a deletion timestamp.

18. **The Wrangler cron comment says production logs an aggregate count, but
    the code deliberately does not.** The comment promises “swept N rows”
    (`cipher-store-cf/wrangler.toml:88-90`); the scheduled handler discards the
    returned count and logs only a fixed failure label
    (`cipher-store-cf/src/index.ts:72-96`). The configuration comment is wrong.
