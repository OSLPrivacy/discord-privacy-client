/// Reusable Ed25519 signed-request verification.
///
/// SECURITY-CRITICAL. This is the open-registration trust anchor and
/// is deliberately general: `register` uses it now; the dormant
/// `prekey-bundle/replenish` and `wrapped-keys` DELETE (burn)
/// endpoints — which the Rust client already signs
/// (`sign_replenish_batch`, `sign_burn`) — can adopt it later WITHOUT
/// redesign (see `verifySignedRequest`).
///
/// The signed message is ALWAYS a domain-separated, version-tagged,
/// newline-delimited UTF-8 string reconstructed server-side from the
/// PARSED fields about to be persisted — never from raw request
/// bytes. This kills JSON-canonicalisation ambiguity: an attacker
/// cannot smuggle extra whitespace / field reordering past the
/// signature because the server signs over its own reconstruction,
/// not the body it received.
///
/// The Rust client builds the byte-identical string in
/// `crates/keystore/src/client.rs` (`build_register_request` /
/// rotation). A divergence of a single byte fails every
/// registration, so both sides carry a mirrored test vector
/// (`test/unit/signed-request.test.ts` ⇆ `client.rs` unit test).

import { verifyEd25519 } from "./crypto.js";
import { decodeBase64 } from "./validation.js";

/** Domain-separation + version tag for a first/refresh registration. */
export const REG_DOMAIN = "OSL-REGISTER-v1";
/** Domain-separation + version tag for an authenticated key rotation. */
export const ROT_DOMAIN = "OSL-ROTATE-v1";

// ---------------------------------------------------------------
// Signed protocol-capability advertisement
// ---------------------------------------------------------------

/**
 * Bit 0 — this identity can send and receive OSL-RN, wire `0x10`
 * (`osl_ratchet_next::WIRE_VERSION_RN`, `ipc::wire_rn`).
 *
 * The bitmap is deliberately NOT indexed by wire-version byte. `0x10`
 * would need a 17-bit map with 16 dead bits, and the two byte
 * namespaces (`wire version` vs `message type`) are already a
 * documented foot-gun in this product — see `WIRE_VERSION_RN`'s doc
 * comment. Named bits, allocated densely from 0, keep the two apart.
 */
export const RN_CAP_WIRE_RN = 1;

/**
 * Upper bound on the bitmap.
 *
 * Unknown bits are stored **verbatim** rather than rejected: a client
 * newer than the worker must be able to advertise a capability this
 * build has no name for, and a reader ignores bits it does not know.
 * The bound exists only so the column stays a small integer and the
 * signed message stays a bounded string.
 */
export const RN_CAP_MAX = 0xffff;

/**
 * Parse and validate a submitted `rn_capabilities` field.
 *
 * Returns:
 * - `{ present: false, value: 0 }` when the field is absent or `null`.
 *   This is the legacy shape every currently-deployed client sends, and
 *   it means **no capability**, never "assume capable".
 * - `{ present: true, value }` for a valid bounded non-negative integer.
 * - `null` when the field is present but malformed. The caller must
 *   refuse the request; coercing a malformed bitmap to `0` would let an
 *   attacker who can corrupt one byte of the field silently strip a
 *   capability instead of breaking the signature.
 */
export function parseRnCapabilities(
  value: unknown,
): { present: boolean; value: number } | null {
  if (value === undefined || value === null) return { present: false, value: 0 };
  if (typeof value !== "number") return null;
  if (!Number.isSafeInteger(value)) return null;
  if (value < 0 || value > RN_CAP_MAX) return null;
  return { present: true, value };
}

/**
 * REG_MSG — bound by the registrant's identity key.
 *
 *   "OSL-REGISTER-v1\n"
 *   || user_id              "\n"
 *   || ik_x25519_pub_b64    "\n"
 *   || ik_ed25519_pub_b64   "\n"
 *   || ik_mlkem768_pub_b64  "\n"
 *   || ik_ratchet_initial_pub_b64_or_empty
 *
 * The base64 strings are used VERBATIM as received in the JSON body
 * (the same strings the client base64-encoded and signed) — never
 * re-encoded. A null / absent ratchet pub contributes the empty
 * string (no trailing newline after the last component).
 *
 * ## The optional capability component
 *
 * When `rn_capabilities` is supplied, one more newline-delimited
 * component is appended: the bitmap as a decimal string. When it is
 * absent the message is byte-identical to the pre-capability form.
 *
 * This two-form encoding is what makes the advertisement **unstrippable
 * on the write path**, and it is why the bitmap lives inside this
 * signature rather than in a separate signed record:
 *
 * - A client that advertises a bitmap signs the extended form. An
 *   attacker who removes `rn_capabilities` from the request body makes
 *   the server reconstruct the *legacy* form, which the submitted
 *   signature does not cover → the registration is refused. The strip
 *   is detected, not silently applied.
 * - An attacker who alters the bitmap changes the reconstruction the
 *   same way → refused.
 * - A legacy client that never sends the field produces exactly the
 *   bytes it produces today, so nothing already deployed breaks. This
 *   is the only reason the component is optional rather than always
 *   present with a `0` default: making it mandatory would invalidate
 *   every signature every currently-shipped client produces.
 *
 * Unambiguity: the legacy form ends at `ratchet` with no trailing
 * newline, and the extended form appends `"\n" + <decimal>` where the
 * decimal is always at least one digit. No legacy message can be read
 * as an extended one or vice versa.
 */
export function buildRegMsg(fields: {
  user_id: string;
  ik_x25519_pub: string;
  ik_ed25519_pub: string;
  ik_mlkem768_pub: string;
  ik_ratchet_initial_pub?: string | null;
  rn_capabilities?: number | null;
}): Uint8Array {
  const ratchet = fields.ik_ratchet_initial_pub ?? "";
  let msg =
    REG_DOMAIN +
    "\n" +
    fields.user_id +
    "\n" +
    fields.ik_x25519_pub +
    "\n" +
    fields.ik_ed25519_pub +
    "\n" +
    fields.ik_mlkem768_pub +
    "\n" +
    ratchet;
  if (fields.rn_capabilities !== undefined && fields.rn_capabilities !== null) {
    msg += "\n" + String(fields.rn_capabilities);
  }
  return new TextEncoder().encode(msg);
}

/**
 * ROT_MSG — authenticated key rotation.
 *
 *   "OSL-ROTATE-v1\n"
 *   || user_id                  "\n"
 *   || prev_ik_ed25519_pub_b64  "\n"
 *   || new_ik_x25519_pub_b64    "\n"
 *   || new_ik_ed25519_pub_b64   "\n"
 *   || new_ik_mlkem768_pub_b64  "\n"
 *   || new_ik_ratchet_initial_pub_b64_or_empty
 *
 * `prev_ik_ed25519_pub` MUST byte-equal the currently-stored key
 * (checked by the caller) so a replayed old rotation no longer
 * matches once a rotation has occurred — no nonce/clock/extra column
 * needed.
 */
export function buildRotMsg(fields: {
  user_id: string;
  prev_ik_ed25519_pub: string;
  new_ik_x25519_pub: string;
  new_ik_ed25519_pub: string;
  new_ik_mlkem768_pub: string;
  new_ik_ratchet_initial_pub?: string | null;
  rn_capabilities?: number | null;
}): Uint8Array {
  const ratchet = fields.new_ik_ratchet_initial_pub ?? "";
  let msg =
    ROT_DOMAIN +
    "\n" +
    fields.user_id +
    "\n" +
    fields.prev_ik_ed25519_pub +
    "\n" +
    fields.new_ik_x25519_pub +
    "\n" +
    fields.new_ik_ed25519_pub +
    "\n" +
    fields.new_ik_mlkem768_pub +
    "\n" +
    ratchet;
  // Same optional-component rule as `buildRegMsg`. A rotation
  // re-states the whole record, so the bitmap it carries must be
  // covered by the rotation authorisation too — otherwise a rotation
  // would be a signature-free way to change a peer's advertised
  // capability.
  if (fields.rn_capabilities !== undefined && fields.rn_capabilities !== null) {
    msg += "\n" + String(fields.rn_capabilities);
  }
  return new TextEncoder().encode(msg);
}

/**
 * Verify `signatureB64` over `message` against `publicKeyB64`.
 *
 * Reusable across endpoints: a future `replenish` / `burn` adoption
 * builds its own canonical `message` (those already have
 * length-prefixed encoders in `canonical.ts`) and calls this with
 * the signer's stored `ik_ed25519_pub` — no changes here required.
 *
 * Never throws (bad base64 / wrong length / verify-fail → `false`),
 * so route handlers stay branch-simple.
 */
export async function verifySignedRequest(
  publicKeyB64: string,
  message: Uint8Array,
  signatureB64: string,
): Promise<boolean> {
  let pub: Uint8Array;
  let sig: Uint8Array;
  try {
    pub = decodeBase64(publicKeyB64);
    sig = decodeBase64(signatureB64);
  } catch {
    return false;
  }
  if (pub.length !== 32 || sig.length !== 64) return false;
  return verifyEd25519(pub, message, sig);
}

/**
 * Startup self-test: verify a known Ed25519 vector so a Workers
 * runtime regression (SubtleCrypto dropping `Ed25519`) fails LOUD at
 * boot instead of silently 403-ing every registration. Cached after
 * the first success (idempotent, ~microseconds when cached).
 *
 * Vector: RFC 8032 test 1 (empty message).
 *   secret  9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60
 *   public  d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a
 *   sig     e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155
 *           5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b
 */
let selfTestPassed = false;
export async function ed25519SelfTest(): Promise<void> {
  if (selfTestPassed) return;
  const hex = (h: string): Uint8Array => {
    const out = new Uint8Array(h.length / 2);
    for (let i = 0; i < out.length; i++) {
      out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
  };
  const pub = hex(
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
  );
  const sig = hex(
    "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155" +
      "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
  );
  const ok = await verifyEd25519(pub, new Uint8Array(0), sig);
  if (!ok) {
    throw new Error(
      "FATAL: Ed25519 self-test failed — Workers SubtleCrypto Ed25519 " +
        "unavailable. Open registration cannot verify signatures; " +
        "refusing to serve so failures are loud, not silent 403s.",
    );
  }
  selfTestPassed = true;
}
