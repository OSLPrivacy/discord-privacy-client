import type { Env } from "../env.js";
import {
  canonicalRolloutAdvanceBytes,
  canonicalRolloutGenesisBytes,
  decodeCanonicalNonce,
  sha256Hex,
  type CanonicalIdentityBundle,
  validateCanonicalIdentityBundle,
  verifyCanonicalEd25519Request,
} from "../lib/identity-authority.js";
import { CONTROL_INBOX_DISPOSITION_CAPABILITY } from "../lib/control-inbox-sweep.js";
import {
  badRequest,
  conflict,
  forbidden,
  json,
  serviceUnavailable,
} from "../lib/http.js";

const REQUEST_ID_RE = /^[A-Za-z0-9_-]{43}$/u;
const FRESHNESS_MS = 5 * 60 * 1000;
const CAPABILITY_VERSION = 1;
const SCHEMA_OBSERVATION =
  "osl.sender-filter.schema-observation.v1\u0000" +
  "control_inbox_sender_disposition\u00001";

interface CanonicalIdentityRow extends CanonicalIdentityBundle {
  identity_bundle_proof_sig: string;
  registration_sig: string;
}

interface RolloutRootRow {
  root_user_id: string;
  root_ed25519_pub: string;
  identity_bundle_sha256: string;
  capability_version: number;
  monotonic_version: number;
  last_observation_sha256: string;
  provisioned_at_ms: number;
  updated_at_ms: number;
}

function parseObjectBody(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("request body must be an object");
  }
  return value as Record<string, unknown>;
}

function parseFreshRequest(body: Record<string, unknown>): {
  rootUserId: string;
  timestampMs: number;
  requestId: string;
  signatureB64: string;
} {
  if (
    typeof body.root_user_id !== "string" ||
    typeof body.timestamp_ms !== "number" ||
    !Number.isSafeInteger(body.timestamp_ms) ||
    body.timestamp_ms <= 0 ||
    Math.abs(Date.now() - body.timestamp_ms) > FRESHNESS_MS ||
    typeof body.request_id !== "string" ||
    !REQUEST_ID_RE.test(body.request_id) ||
    typeof body.signature_b64 !== "string"
  ) {
    throw new Error("rollout authority request is malformed or stale");
  }
  return {
    rootUserId: body.root_user_id,
    timestampMs: body.timestamp_ms,
    requestId: body.request_id,
    signatureB64: body.signature_b64,
  };
}

async function readCanonicalIdentity(
  db: D1Database,
  userId: string,
): Promise<CanonicalIdentityRow | null> {
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
            ik_x25519_signature AS registration_sig
       FROM users
      WHERE user_id = ?
        AND identity_scheme = 1
        AND identity_lookup_enabled = 1`,
  ).bind(userId).first<CanonicalIdentityRow>();
}

async function validatedIdentity(
  db: D1Database,
  userId: string,
): Promise<{
  row: CanonicalIdentityRow;
  bundleSha256: string;
}> {
  const row = await readCanonicalIdentity(db, userId);
  if (!row) throw new Error("canonical rollout root identity is unavailable");
  const validated = await validateCanonicalIdentityBundle(
    row,
    row.identity_bundle_proof_sig,
    row.registration_sig,
  );
  return { row, bundleSha256: validated.bundle_sha256 };
}

async function liveSchemaReady(db: D1Database): Promise<boolean> {
  try {
    const marker = await db.prepare(
      `SELECT version
         FROM worker_schema_capabilities
        WHERE capability = ?`,
    ).bind(CONTROL_INBOX_DISPOSITION_CAPABILITY)
      .first<{ version: number }>();
    if (marker?.version !== CAPABILITY_VERSION) return false;
    await db.prepare(
      `SELECT delivery_status,
              delivery_reason,
              delivery_attempts,
              sender_disabled_first_seen_at,
              delivery_next_retry_at,
              delivery_retain_until
         FROM control_inbox
        LIMIT 0`,
    ).all();
    return true;
  } catch {
    return false;
  }
}

async function readRoot(db: D1Database): Promise<RolloutRootRow | null> {
  return await db.prepare(
    `SELECT root_user_id,
            root_ed25519_pub,
            identity_bundle_sha256,
            capability_version,
            monotonic_version,
            last_observation_sha256,
            provisioned_at_ms,
            updated_at_ms
       FROM sender_filter_rollout_root
      WHERE singleton = 1`,
  ).first<RolloutRootRow>();
}

export async function handleSenderFilterRolloutRootProvision(
  request: Request,
  env: Env,
): Promise<Response> {
  let body: Record<string, unknown>;
  let parsed;
  try {
    body = parseObjectBody(await request.json());
    parsed = parseFreshRequest(body);
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }
  if (await readRoot(env.DB)) {
    return conflict("sender-filter rollout root is already provisioned");
  }
  if (!(await liveSchemaReady(env.DB))) {
    return serviceUnavailable("sender-filter capability schema unavailable");
  }

  let nonce: Uint8Array;
  let identity;
  try {
    nonce = decodeCanonicalNonce(body.genesis_nonce);
    identity = await validatedIdentity(env.DB, parsed.rootUserId);
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }
  const nonceSha256 = await sha256Hex(nonce);
  const canonical = canonicalRolloutGenesisBytes({
    root_user_id: parsed.rootUserId,
    genesis_nonce_sha256: nonceSha256,
    timestamp_ms: parsed.timestampMs,
    request_id: parsed.requestId,
  });
  try {
    if (
      !(await verifyCanonicalEd25519Request(
        identity.row.ik_root_ed25519_pub,
        canonical,
        parsed.signatureB64,
      ))
    ) {
      return forbidden("rollout genesis signature is invalid");
    }
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }

  const schemaObservationSha256 = await sha256Hex(
    new TextEncoder().encode(SCHEMA_OBSERVATION),
  );
  let results: D1Result[];
  try {
    results = await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO sender_filter_rollout_root (
           singleton,
           root_user_id,
           root_ed25519_pub,
           identity_bundle_sha256,
           capability_version,
           monotonic_version,
           last_observation_sha256,
           provisioned_at_ms,
           updated_at_ms
         )
         SELECT 1, ?, ?, ?, 1, 1, ?, ?, ?
           FROM sender_filter_rollout_genesis
          WHERE nonce_sha256 = ?
            AND consumed_at_ms IS NULL
            AND NOT EXISTS (
              SELECT 1 FROM sender_filter_rollout_root WHERE singleton = 1
            )`,
      ).bind(
        parsed.rootUserId,
        identity.row.ik_root_ed25519_pub,
        identity.bundleSha256,
        schemaObservationSha256,
        parsed.timestampMs,
        parsed.timestampMs,
        nonceSha256,
      ),
      env.DB.prepare(
        `UPDATE sender_filter_rollout_genesis
            SET consumed_at_ms = ?
          WHERE nonce_sha256 = ?
            AND consumed_at_ms IS NULL
            AND EXISTS (
              SELECT 1
                FROM sender_filter_rollout_root
               WHERE singleton = 1
                 AND root_user_id = ?
            )`,
      ).bind(parsed.timestampMs, nonceSha256, parsed.rootUserId),
    ]);
  } catch {
    return conflict("sender-filter rollout genesis was not consumed");
  }
  if (
    (results[0]?.meta?.changes ?? 0) !== 1 ||
    (results[1]?.meta?.changes ?? 0) !== 1
  ) {
    return conflict("sender-filter rollout genesis was absent or consumed");
  }
  return json(
    {
      format: "osl.sender-filter.rollout-root.v1",
      root_user_id: parsed.rootUserId,
      capability_version: 1,
      monotonic_version: 1,
      identity_bundle_sha256: identity.bundleSha256,
      last_observation_sha256: schemaObservationSha256,
      request_id: parsed.requestId,
    },
    { status: 201 },
  );
}

export async function handleSenderFilterRolloutRootAdvance(
  request: Request,
  env: Env,
): Promise<Response> {
  let body: Record<string, unknown>;
  let parsed;
  try {
    body = parseObjectBody(await request.json());
    parsed = parseFreshRequest(body);
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }
  if (
    !Number.isSafeInteger(body.expected_monotonic_version) ||
    (body.expected_monotonic_version as number) <= 0 ||
    typeof body.observation_sha256 !== "string"
  ) {
    return badRequest("rollout advance fields are malformed");
  }
  const expectedVersion = body.expected_monotonic_version as number;
  const observationSha256 = body.observation_sha256;
  let root: RolloutRootRow | null;
  let identity;
  try {
    root = await readRoot(env.DB);
    if (!root || root.root_user_id !== parsed.rootUserId) {
      return forbidden("requester is not the durable rollout root");
    }
    identity = await validatedIdentity(env.DB, parsed.rootUserId);
    if (
      identity.row.ik_root_ed25519_pub !== root.root_ed25519_pub
    ) {
      return forbidden("rollout root identity binding changed");
    }
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }
  let canonical: Uint8Array;
  try {
    canonical = canonicalRolloutAdvanceBytes({
      root_user_id: parsed.rootUserId,
      expected_monotonic_version: expectedVersion,
      observation_sha256: observationSha256,
      timestamp_ms: parsed.timestampMs,
      request_id: parsed.requestId,
    });
    if (
      !(await verifyCanonicalEd25519Request(
        root.root_ed25519_pub,
        canonical,
        parsed.signatureB64,
      ))
    ) {
      return forbidden("rollout advance signature is invalid");
    }
  } catch (error) {
    return badRequest(error instanceof Error ? error.message : "bad request");
  }
  const nextVersion = expectedVersion + 1;
  const result = await env.DB.prepare(
    `UPDATE sender_filter_rollout_root
        SET monotonic_version = ?,
            last_observation_sha256 = ?,
            updated_at_ms = ?
      WHERE singleton = 1
        AND root_user_id = ?
        AND root_ed25519_pub = ?
        AND monotonic_version = ?`,
  ).bind(
    nextVersion,
    observationSha256,
    parsed.timestampMs,
    parsed.rootUserId,
    root.root_ed25519_pub,
    expectedVersion,
  ).run();
  if ((result.meta?.changes ?? 0) !== 1) {
    return conflict(
      "sender-filter rollout state changed concurrently; retry refused",
    );
  }
  return json({
    format: "osl.sender-filter.rollout-root.v1",
    root_user_id: parsed.rootUserId,
    capability_version: root.capability_version,
    monotonic_version: nextVersion,
    last_observation_sha256: observationSha256,
    request_id: parsed.requestId,
  });
}
