import type { Env } from "../env.js";
import { getSignedIdentity } from "../lib/db.js";
import { badRequest, json, notFound } from "../lib/http.js";
import { isDiscordSnowflake } from "../lib/validation.js";

/// GET /v1/pubkeys/:user_id — the public identity record.
///
/// # Why this route now returns a signature
///
/// Every response includes `registration_sig`, the Ed25519 signature
/// over the reconstructed REG_MSG. Clients verify it before accepting
/// any encryption key, so this service carries bundles but cannot
/// substitute X25519, ML-KEM, ratchet or capability values.
///
/// `rn_capabilities` is always present and defaults to 0, so a reader
/// that sees no bitmap and a reader that sees `0` reach the same
/// conclusion: **no OSL-RN capability**. There is no encoding of
/// "unknown, assume capable".
///
/// # Why this route publishes no lifecycle timing
///
/// This is an unauthenticated route. Anything it returns beyond the key bundle
/// is readable by anyone who holds an identifier, and the 2026-07-26 audit
/// recorded exactly that: `registered_at` and `last_rotated_at` gave adoption
/// and account-activity timing away for free. Migration 0029 closed the
/// enumeration half by refusing Discord snowflakes and making identities
/// opaque; this closes the per-identity half.
///
///   * `last_rotated_at` is not returned at all. It is the live-activity
///     signal, and every client already declares it `Option<String>`.
///   * `registered_at` is reduced to UTC date granularity. It cannot simply be
///     dropped: `PubkeysResponse` (crates/keystore/src/client.rs) and
///     `FetchPubkeysResponse` (crates/ipc/src/commands.rs) both declare it as a
///     required `String`, so a missing key fails deserialisation and breaks
///     every key fetch on a deployed client. Removing it is staged behind that
///     client change — see docs/reports/server-lane-2026-07-26.md.
export async function handlePubkeys(env: Env, userId: string): Promise<Response> {
  if (isDiscordSnowflake(userId)) {
    return badRequest("Discord identifiers are not OSL identities");
  }
  const row = await getSignedIdentity(env.DB, userId);
  if (!row) return notFound("unknown user_id");
  const body: Record<string, unknown> = {
    user_id: row.user_id,
    ik_x25519_pub: row.ik_x25519_pub,
    ik_ed25519_pub: row.ik_ed25519_pub,
    ik_mlkem768_pub: row.ik_mlkem768_pub,
    ik_ratchet_initial_pub: row.ik_ratchet_initial_pub,
    registered_at: coarsenToUtcDate(row.registered_at),
    rn_capabilities: row.rn_capabilities,
  };
  body.registration_sig = row.ik_x25519_signature;
  return json(body);
}

/// Truncate an ISO-8601 instant to its UTC date. Keeps the field's declared
/// shape (clients pattern-match `YYYY-MM-DDT...`) while removing the ordering
/// and correlation value of a precise timestamp. An unparseable legacy value is
/// replaced rather than passed through.
function coarsenToUtcDate(value: string): string {
  const parsed = Date.parse(value);
  if (Number.isNaN(parsed)) return "1970-01-01T00:00:00Z";
  return `${new Date(parsed).toISOString().slice(0, 10)}T00:00:00Z`;
}
