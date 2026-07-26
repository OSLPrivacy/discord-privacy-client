import type { Env } from "../env.js";
import { getSignedIdentity } from "../lib/db.js";
import { json, notFound } from "../lib/http.js";

/// GET /v1/pubkeys/:user_id — the public identity record.
///
/// # Why this route now returns a signature
///
/// The signed protocol-capability advertisement (migration 0026) only
/// closes the write path on its own: REG_MSG covers `rn_capabilities`,
/// so nobody without the identity's Ed25519 secret can register a
/// stripped or lowered bitmap. That says nothing about the *read* path.
/// A reader that simply believes this JSON is trusting the key server —
/// and anything terminating TLS in front of it — to report the bitmap
/// honestly, which is exactly the trust the advertisement is supposed
/// to remove.
///
/// So a record that advertises a capability also returns
/// `registration_sig`: the Ed25519 signature over the REG_MSG this
/// record reconstructs to. A client rebuilds those bytes from the
/// fields below and verifies. Stripping or lowering `rn_capabilities`
/// in transit then makes the reconstruction stop matching the
/// signature, so the client detects tampering instead of quietly
/// concluding "this peer cannot do OSL-RN".
///
/// `registration_sig` is emitted ONLY when `rn_capabilities` is
/// non-zero. For every record that does not advertise a capability —
/// which is every record that exists today — the response gains only
/// the explicit `rn_capabilities: 0`, and `test/integration/
/// pubkeys.test.ts`'s pin that no signature is exposed still holds
/// unchanged and untouched. Nothing here is secret (a signature
/// over public keys, by a public key), but shipping it only where it
/// is load-bearing keeps the blast radius at zero for identities that
/// are not using the feature.
///
/// `rn_capabilities` is always present and defaults to 0, so a reader
/// that sees no bitmap and a reader that sees `0` reach the same
/// conclusion: **no OSL-RN capability**. There is no encoding of
/// "unknown, assume capable".
export async function handlePubkeys(env: Env, userId: string): Promise<Response> {
  const row = await getSignedIdentity(env.DB, userId);
  if (!row) return notFound("unknown user_id");
  const body: Record<string, unknown> = {
    user_id: row.user_id,
    ik_x25519_pub: row.ik_x25519_pub,
    ik_ed25519_pub: row.ik_ed25519_pub,
    ik_mlkem768_pub: row.ik_mlkem768_pub,
    ik_ratchet_initial_pub: row.ik_ratchet_initial_pub,
    registered_at: row.registered_at,
    last_rotated_at: row.last_rotated_at,
    rn_capabilities: row.rn_capabilities,
  };
  if (row.rn_capabilities !== 0) {
    body.registration_sig = row.ik_x25519_signature;
  }
  return json(body);
}
