# Migration 0038: D81 social-graph cutover

Status: **design and release gate only — do not apply a D1 migration yet.**

This is deliberately not an additive SQL migration.  The current protocol
signs `sender_id`, `recipient_id`, and (for a drain) `user_id`/the optional
`sender_id` into v1 canonical bytes.  Removing or changing those fields in a
server-only release makes every deployed v1 client fail signature verification.
Keeping a v1 compatibility path, or copying its rows into a new table, retains
the named social graph.  Either outcome fails D81.

## Chosen design

Use a client-generated, per-pair 256-bit `pair_route` as the durable routing
key.  It is derived from existing pair secret material (with a domain-separated
KDF) or generated and delivered inside an already authenticated E2EE pair
bootstrap.  It must never be derived from a user id, username, or a server
secret, and it must not appear in a URL.  The server stores and indexes only
this opaque value.  It has no key that lets it recover either endpoint.

`pair_route` is both the routing capability and the pair-local quota key.  It
is intentionally different for every pair: one recipient's rows cannot be
grouped across their contacts by a stable mailbox id.  Possession authorizes
drain/delete; sender attribution is taken from the sealed control bundle by
the receiving client, not from a D1 envelope column.

The sender's v2 request still identifies a public signing key to the Worker
while it is verified.  That identity is request-transient and is never written
to D1.  The canonical request digest is retained only as a SHA-256 replay
commitment; no identity is retained alongside it.

## v2 wire and storage contract

All v2 endpoints use fixed paths.  `pair_route`, `user_id`, and sender data
are request-body fields or headers, never path/query fields.  New canonical
domains make v1 and v2 signatures unambiguous; the v2 canonical bytes bind the
route and payload digest.  No v2 handler may fall back to a v1 table.

| Current durable surface | v2 durable surface |
| --- | --- |
| `control_inbox(recipient_id, sender_id, ...)` | `control_inbox_v2(pair_route BLOB, id, scope_id, bundle, expiry, delivery metadata)`; no identity columns |
| `control_inbox_requests(sender_id, recipient_id, request_digest, ...)` | `control_inbox_requests_v2(request_digest PRIMARY KEY, inbox_id, expires_at, eviction_count)` |
| `wrapped_keys(sender_id, recipient_id, ...)` and `wrapped_key_post_receipts(sender_id, ...)` | v2 rows keyed by `pair_route`/opaque content capability and replay digest only |
| `consuming_get_receipts(requester_id, recipient_id, target_id, ...)` | `consuming_get_receipts_v2(request_digest PRIMARY KEY, target_id, expires_at)` |
| `mail_sender_consents(recipient_user_id, sender_user_id, ...)` | `mail_sender_consents_v2(pair_route PRIMARY KEY, allowed, updated_at)` |

The v2 D1 schema must contain no foreign key, index, trigger, view, or retained
column that joins `pair_route` to `users`, `username_directory`, or
`mail_address_epochs`.  The only named lookups needed to verify a sender or
resolve an address happen in request memory and are not written as a pair.

## Required coordinated release

1. Ship a client release that can create, retain, and rotate a unique
   `pair_route` for every existing pair, and can consume sender attribution
   solely from the sealed payload.  It must implement the fixed-path v2 POST,
   drain, delete, wrapped-key, and mail-consent requests and their v2
   canonical-byte domains.
2. Ship a Worker that implements those v2 endpoints and the v2 schema only for
   v2 traffic.  Its rollout gate must attest a client minimum version and a
   successful two-client v2 delivery test.  Do not enable v2 in production
   merely because this plan exists.
3. Keep v1 only for the announced transition window.  Do not describe the
   privacy property as satisfied during this window: v1 rows and requests are
   still a plaintext social graph.
4. After every supported client is v2-capable, refuse v1 traffic.  Wait for the
   maximum v1 retention period, including the 0031 quarantine hold (14 days
   today), and verify that all five v1 envelope tables contain zero rows.
5. In the same release window, apply the actual `0038_*.sql` table-rebuild
   migration, remove v1 routes/canonical domains, and remove every legacy
   identity-bearing index and trigger.  The migration must not copy legacy
   rows: retaining or transforming them preserves the graph.  Deploy the
   Worker that requires the new schema capability immediately after the
   migration; rollback is to the v2 Worker, never the v1 Worker.
6. Run the D1-read adversary test below against the migrated schema and retain
   its output in the release evidence.  Only then may D81 F2/F4 be marked
   fixed.

## Acceptance test for the eventual SQL migration

Seed two rows whose former endpoints are present in `username_directory`.
Against the *post-cutover database*, the audit query must fail because neither
`control_inbox` nor any v2 table has `sender_id`/`recipient_id`; equivalent
joins through the four other envelope tables must also fail.  A D1 reader may
count opaque routes, but cannot recover a named pair or map a route to an
endpoint from D1 data.

The accompanying test enforces the release-plan facts that make this a real
cutover rather than a column rename.  It also asserts the present v1 canonical
binding, so a future server-only "migration" cannot be mistaken for a safe
deployment.
