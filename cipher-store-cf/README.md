# oslprivacy-cipher-store

Cloudflare Worker + D1/R2 that stores short-TTL E2E ciphertext blobs
keyed by random 8-byte IDs. A database leak yields E2E-encrypted
payloads rather than plaintext, and the application schema records no
user identities. Cloudflare edge logs and short-lived hashed-IP rate-limit
keys remain part of the provider-side metadata exposure.

## Routes

| Method | Path                | Notes                                          |
|--------|---------------------|------------------------------------------------|
| GET    | `/v1/healthz`       | Liveness probe.                                |
| POST   | `/v1/blob`          | Body: ciphertext bytes. Requires `X-OSL-TTL-Seconds` ∈ {3600, 86400, 259200, 604800} and a 32-hex-character `X-OSL-Fetch-Token`. Returns `{ id, expires_at }`. |
| GET    | `/v1/blob/:id_hex`  | Requires the upload's fetch token. Returns raw ciphertext bytes, or 404 if missing / expired. |
| DELETE | `/v1/blob/:id_hex`  | Requires the upload's fetch token and burns the blob idempotently. |
| POST   | `/v1/attachment` | Streams already-sealed bytes to private R2. Same TTL/token headers; returns a 32-hex opaque ID. |
| POST   | `/v1/attachment/session` | Starts a multipart upload. Also requires `X-OSL-Size-Bytes` for the complete sealed object. |
| PUT    | `/v1/attachment/:id_hex/part/:number` | Streams one sealed part (1–65, at most 8 MiB) with the attachment token. |
| POST   | `/v1/attachment/:id_hex/complete` | Atomically completes a fully received multipart object. |
| GET    | `/v1/attachment/:id_hex` | Capability-gated streaming ciphertext response. |
| DELETE | `/v1/attachment/:id_hex` | Capability-gated, idempotent object burn. |

## Limits

- Max blob size: **64 KB**. Typical v=4 wire ciphertexts are ~1–2 KB.
- Direct uploads remain capped at **26 MiB**. Multipart uploads support a **512 MiB plaintext** file with a bounded framing allowance (**513 MiB sealed**, 65 parts of at most 8 MiB).
- D1 stores only opaque object/part/expiry state and SHA-256 capability digests, never raw bearer tokens. Atomic conditional inserts cap live attachment metadata at 512 rows and 8 GiB of declared sealed bytes.
- TTL values: 1h, 24h, 72h, or 7d. No other values accepted.
- Best-effort KV rate limits per IP per hour: 600 uploads / 3600 fetches / 600 deletes. Identifiers are HMAC-pseudonymized with a Worker secret. KV is eventually consistent; limiter failures deny anonymous writes while reads remain available.
- Attachment transport uses separate budgets: 140 upload/session/part requests, 120 fetches, and 60 deletes per IP per hour.

## Data-minimisation posture

- **No identity binding.** Uploads carry no user ID or account credential. The opaque fetch token separates a bare blob ID from fetch/delete authority, but it is not proof of identity, sender authenticity, or conversation membership.
- **No variable application logging.** Fixed failure event names contain no
  identifiers, URLs, sizes, row counts, or timing details; sweep volume is not
  logged.
- **TLS-terminator logs** (Cloudflare edge) are out of our control. Onion-routing the client side is a Phase 6+ research item.
- **Cleanup cron** runs every 5 minutes and deletes expired rows.
- Attachment cleanup aborts expired incomplete multipart sessions and deletes completed R2 objects before their minimal D1 rows. The hard row cap bounds the entire sweep backlog.

## Deploy

See `DEPLOY.md` (one-time setup) and `package.json` scripts.

## Phase status

The short-TTL store and fetch-token gate are implemented. Privacy Pass
or another anonymous abuse-control credential remains future work.
