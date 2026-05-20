# oslprivacy-cipher-store

Cloudflare Worker + D1 that stores short-TTL E2E ciphertext blobs
keyed by random 8-byte IDs. Designed so a full database leak
yields opaque bytes (the cipher is encrypted to ratchet keys that
live only on user devices) and a leaked operations log yields
nothing — no identities recorded, no IPs persisted.

## Routes

| Method | Path                | Notes                                          |
|--------|---------------------|------------------------------------------------|
| GET    | `/v1/healthz`       | Liveness probe.                                |
| POST   | `/v1/blob`          | Body: ciphertext bytes. Header: `X-OSL-TTL-Seconds` ∈ {86400, 259200, 604800}. Returns `{ id, expires_at }`. |
| GET    | `/v1/blob/:id_hex`  | Returns raw ciphertext bytes, or 404 if missing / expired. |
| DELETE | `/v1/blob/:id_hex`  | Burns the blob. Anyone with the ID can delete (the ID itself is the capability). |

## Limits

- Max blob size: **64 KB**. Typical v=4 wire ciphertexts are ~1–2 KB.
- TTL values: 24h, 72h, or 7d. No other values accepted.
- Rate limits per IP per hour: 60 uploads / 600 fetches / 60 deletes.

## Subpoena-resistance posture

- **No identity binding.** Uploads carry no user_id, no auth header in Phase 1. Phase 6 adds Privacy Pass anonymous credentials.
- **No application logging** of who-uploaded-what beyond the aggregate sweep count.
- **TLS-terminator logs** (Cloudflare edge) are out of our control. Onion-routing the client side is a Phase 6+ research item.
- **Cleanup cron** runs every 5 minutes and deletes expired rows.

## Deploy

See `DEPLOY.md` (one-time setup) and `package.json` scripts.

## Phase status

Phase 1 of 7. Functional but unauthenticated. Phase 2 onward lives
in `discord-privacy-client/` (client side).
