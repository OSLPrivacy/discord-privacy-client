# oslprivacy-cipher-store

Cloudflare Worker + D1 that stores short-TTL E2E ciphertext blobs
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

## Limits

- Max blob size: **64 KB**. Typical v=4 wire ciphertexts are ~1–2 KB.
- TTL values: 1h, 24h, 72h, or 7d. No other values accepted.
- Best-effort KV rate limits per IP per hour: 600 uploads / 3600 fetches / 600 deletes. Identifiers are HMAC-pseudonymized with a Worker secret. KV is eventually consistent; limiter failures deny anonymous writes while reads remain available.

## Data-minimisation posture

- **No identity binding.** Uploads carry no user ID or account credential. The opaque fetch token separates a bare blob ID from fetch/delete authority, but it is not proof of identity, sender authenticity, or conversation membership.
- **No variable application logging.** Fixed failure event names contain no
  identifiers, URLs, sizes, row counts, or timing details; sweep volume is not
  logged.
- **TLS-terminator logs** (Cloudflare edge) are out of our control. Onion-routing the client side is a Phase 6+ research item.
- **Cleanup cron** runs every 5 minutes and deletes expired rows.

## Deploy

See `DEPLOY.md` (one-time setup) and `package.json` scripts.

## Phase status

The short-TTL store and fetch-token gate are implemented. Privacy Pass
or another anonymous abuse-control credential remains future work.
