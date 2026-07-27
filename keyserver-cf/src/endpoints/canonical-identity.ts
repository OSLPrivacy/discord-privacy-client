import type { Env } from "../env.js";
import {
  CANONICAL_IDENTITY_BUNDLE_VERSION,
  CANONICAL_IDENTITY_SCHEME,
  type CanonicalIdentityBundle,
  validateCanonicalIdentityBundle,
  verifyCanonicalEd25519Request,
} from "../lib/identity-authority.js";
import { badRequest, conflict, forbidden, json } from "../lib/http.js";

interface StoredCanonicalIdentity
  extends Omit<CanonicalIdentityBundle, "identity_bundle_version"> {
  identity_bundle_proof_sig: string;
  registration_sig: string;
  registered_at: string;
  identity_lookup_enabled: number;
}

const CANONICAL_REGISTER_FIELDS = [
  "identity_scheme",
  "identity_bundle_version",
  "identity_revision",
  "user_id",
  "ik_root_ed25519_pub",
  "ik_x25519_pub",
  "ik_ed25519_pub",
  "ik_mlkem768_pub",
  "ik_ratchet_initial_pub",
  "rn_capabilities",
  "identity_bundle_proof_sig",
  "registration_sig",
] as const;

function hasExactFields(
  body: Record<string, unknown>,
  includeRotationProof: boolean,
): boolean {
  const expected = includeRotationProof
    ? [...CANONICAL_REGISTER_FIELDS, "rotation_prev_sig"].sort()
    : [...CANONICAL_REGISTER_FIELDS].sort();
  const actual = Object.keys(body).sort();
  return actual.length === expected.length &&
    actual.every((field, index) => field === expected[index]);
}

function parseBundle(body: Record<string, unknown>): CanonicalIdentityBundle {
  return {
    user_id: body.user_id as string,
    identity_scheme: body.identity_scheme as 1,
    identity_bundle_version: body.identity_bundle_version as 1,
    identity_revision: body.identity_revision as number,
    ik_root_ed25519_pub: body.ik_root_ed25519_pub as string,
    ik_x25519_pub: body.ik_x25519_pub as string,
    ik_ed25519_pub: body.ik_ed25519_pub as string,
    ik_mlkem768_pub: body.ik_mlkem768_pub as string,
    ik_ratchet_initial_pub:
      body.ik_ratchet_initial_pub === undefined ||
      body.ik_ratchet_initial_pub === null
        ? null
        : body.ik_ratchet_initial_pub as string,
    rn_capabilities: body.rn_capabilities as number,
  };
}

async function readStored(
  db: D1Database,
  userId: string,
): Promise<StoredCanonicalIdentity | null> {
  return await db.prepare(
    `SELECT user_id,
            identity_scheme,
            identity_revision,
            ik_root_ed25519_pub,
            ik_x25519_pub,
            ik_ed25519_pub,
            ik_mlkem768_pub,
            ik_ratchet_initial_pub,
            rn_capabilities,
            identity_bundle_proof_sig,
            ik_x25519_signature AS registration_sig,
            registered_at,
            identity_lookup_enabled
       FROM users
      WHERE user_id = ?`,
  ).bind(userId).first<StoredCanonicalIdentity>();
}

function sameBundle(
  stored: StoredCanonicalIdentity,
  bundle: CanonicalIdentityBundle,
  rootProof: string,
  registrationProof: string,
): boolean {
  return (
    stored.identity_scheme === bundle.identity_scheme &&
    stored.identity_revision === bundle.identity_revision &&
    stored.ik_root_ed25519_pub === bundle.ik_root_ed25519_pub &&
    stored.ik_x25519_pub === bundle.ik_x25519_pub &&
    stored.ik_ed25519_pub === bundle.ik_ed25519_pub &&
    stored.ik_mlkem768_pub === bundle.ik_mlkem768_pub &&
    stored.ik_ratchet_initial_pub === bundle.ik_ratchet_initial_pub &&
    stored.rn_capabilities === bundle.rn_capabilities &&
    stored.identity_bundle_proof_sig === rootProof &&
    stored.registration_sig === registrationProof
  );
}

/**
 * Scheme-1 branch of the shipping `/v1/register` route.
 *
 * The immutable root key derives the opaque `osl1_...` identifier and signs
 * the complete encryption/capability bundle. The current Ed25519 key signs the
 * same bytes. A rotation additionally requires the old current key and advances
 * `identity_revision` by exactly one in the same conditional UPDATE. A CAS
 * conflict is returned once; this function never retries against a fresh row.
 */
export async function handleCanonicalIdentityRegister(
  body: Record<string, unknown>,
  env: Env,
): Promise<Response> {
  if (body.identity_scheme !== CANONICAL_IDENTITY_SCHEME) {
    return badRequest("canonical identity_scheme must be 1");
  }
  const hasRotationProof = "rotation_prev_sig" in body;
  if (!hasExactFields(body, hasRotationProof)) {
    return badRequest("canonical identity registration fields are noncanonical");
  }
  const rootProof = body.identity_bundle_proof_sig;
  const registrationProof = body.registration_sig;
  if (
    typeof rootProof !== "string" ||
    typeof registrationProof !== "string"
  ) {
    return badRequest("canonical identity proofs are required");
  }

  const bundle = parseBundle(body);
  let validated;
  try {
    validated = await validateCanonicalIdentityBundle(
      bundle,
      rootProof,
      registrationProof,
    );
  } catch (error) {
    return badRequest(
      error instanceof Error ? error.message : "canonical identity is invalid",
    );
  }

  const stored = await readStored(env.DB, bundle.user_id);
  const now = new Date().toISOString();
  if (!stored) {
    if (hasRotationProof) {
      return badRequest("canonical identity genesis cannot carry a rotation proof");
    }
    if (bundle.identity_revision !== 1) {
      return conflict("canonical identity genesis revision must be 1");
    }
    try {
      const result = await env.DB.prepare(
        `INSERT INTO users (
           user_id,
           ik_x25519_pub,
           ik_ed25519_pub,
           ik_mlkem768_pub,
           ik_x25519_signature,
           registered_at,
           last_rotated_at,
           ik_ratchet_initial_pub,
           identity_lookup_enabled,
           rn_capabilities,
           identity_scheme,
           ik_root_ed25519_pub,
           identity_revision,
           identity_bundle_proof_sig
         ) VALUES (?, ?, ?, ?, ?, ?, NULL, ?, 1, ?, 1, ?, 1, ?)`,
      ).bind(
        bundle.user_id,
        bundle.ik_x25519_pub,
        bundle.ik_ed25519_pub,
        bundle.ik_mlkem768_pub,
        registrationProof,
        now,
        bundle.ik_ratchet_initial_pub,
        bundle.rn_capabilities,
        bundle.ik_root_ed25519_pub,
        rootProof,
      ).run();
      if ((result.meta?.changes ?? 0) !== 1) {
        return conflict("canonical identity genesis lost a concurrent race");
      }
    } catch {
      return conflict("canonical identity genesis lost a concurrent race");
    }
    return json(
      {
        user_id: bundle.user_id,
        identity_scheme: 1,
        identity_bundle_version: CANONICAL_IDENTITY_BUNDLE_VERSION,
        identity_revision: 1,
        identity_bundle_sha256: validated.bundle_sha256,
        registered_at: now,
      },
      { status: 201 },
    );
  }

  if (
    stored.identity_scheme !== 1 ||
    stored.ik_root_ed25519_pub !== bundle.ik_root_ed25519_pub
  ) {
    return forbidden("canonical identity root does not match durable identity");
  }
  if (
    bundle.identity_revision === stored.identity_revision &&
    sameBundle(stored, bundle, rootProof, registrationProof)
  ) {
    if (hasRotationProof) {
      return badRequest("canonical identity no-op cannot carry a rotation proof");
    }
    if (stored.identity_lookup_enabled !== 1) {
      await env.DB.prepare(
        `UPDATE users
            SET identity_lookup_enabled = 1
          WHERE user_id = ?
            AND identity_scheme = 1
            AND identity_revision = ?
            AND ik_ed25519_pub = ?`,
      ).bind(
        bundle.user_id,
        stored.identity_revision,
        stored.ik_ed25519_pub,
      ).run();
    }
    return json({
      user_id: bundle.user_id,
      identity_scheme: 1,
      identity_bundle_version: CANONICAL_IDENTITY_BUNDLE_VERSION,
      identity_revision: stored.identity_revision,
      status: "noop",
    });
  }
  if (bundle.identity_revision !== stored.identity_revision + 1) {
    return conflict(
      `identity_revision must advance exactly to ${stored.identity_revision + 1}`,
    );
  }
  if (
    typeof body.rotation_prev_sig !== "string" ||
    !(await verifyCanonicalEd25519Request(
      stored.ik_ed25519_pub,
      validated.canonical_bytes,
      body.rotation_prev_sig,
    ))
  ) {
    return forbidden("current identity key did not authorize the revision");
  }

  const result = await env.DB.prepare(
    `UPDATE users
        SET ik_x25519_pub = ?,
            ik_ed25519_pub = ?,
            ik_mlkem768_pub = ?,
            ik_x25519_signature = ?,
            ik_ratchet_initial_pub = ?,
            last_rotated_at = ?,
            identity_lookup_enabled = 1,
            rn_capabilities = ?,
            identity_revision = ?,
            identity_bundle_proof_sig = ?
      WHERE user_id = ?
        AND identity_scheme = 1
        AND identity_revision = ?
        AND ik_root_ed25519_pub = ?
        AND ik_ed25519_pub = ?`,
  ).bind(
    bundle.ik_x25519_pub,
    bundle.ik_ed25519_pub,
    bundle.ik_mlkem768_pub,
    registrationProof,
    bundle.ik_ratchet_initial_pub,
    now,
    bundle.rn_capabilities,
    bundle.identity_revision,
    rootProof,
    bundle.user_id,
    stored.identity_revision,
    stored.ik_root_ed25519_pub,
    stored.ik_ed25519_pub,
  ).run();
  if ((result.meta?.changes ?? 0) !== 1) {
    return conflict("canonical identity changed concurrently; retry refused");
  }
  return json({
    user_id: bundle.user_id,
    identity_scheme: 1,
    identity_bundle_version: CANONICAL_IDENTITY_BUNDLE_VERSION,
    identity_revision: bundle.identity_revision,
    identity_bundle_sha256: validated.bundle_sha256,
    status: "rotated",
    last_rotated_at: now,
  });
}
