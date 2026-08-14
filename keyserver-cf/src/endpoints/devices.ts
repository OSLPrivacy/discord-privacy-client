import type { Env } from "../env.js";
import { canonicalDeviceListBytes } from "../lib/canonical.js";
import { getSignedIdentity } from "../lib/db.js";
import { badRequest, conflict, json, notFound, tooMany, unauthorized } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { verifySignedRequest } from "../lib/signed-request.js";
import { isNonEmptyBase64, isProtocolId, isU32 } from "../lib/validation.js";

const MAX_DEVICE_ROSTER_ENTRIES = 64;
export const DEVICE_LOOKUP_RESPONSE_BYTES = 65536;

interface DeviceRosterEntry {
  device_id: string;
  prekey_bundle: string;
}

function parseDeviceRosterBody(body: unknown):
  | {
    ok: true;
    value: {
      user_id: string;
      version: number;
      devices: DeviceRosterEntry[];
      root_signature_b64: string;
    };
  }
  | { ok: false; error: string } {
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    return { ok: false, error: "device list body must be an object" };
  }
  const record = body as Record<string, unknown>;
  const expected = ["devices", "root_signature_b64", "user_id", "version"];
  const actual = Object.keys(record).sort();
  if (
    actual.length !== expected.length ||
    actual.some((field, index) => field !== expected[index])
  ) {
    return { ok: false, error: "device list fields are noncanonical" };
  }
  if (!isProtocolId(record.user_id)) {
    return {
      ok: false,
      error: "user_id must be a bounded identifier without control characters",
    };
  }
  if (!isU32(record.version) || record.version < 1) {
    return { ok: false, error: "device list version must be a positive u32" };
  }
  if (!Array.isArray(record.devices)) {
    return { ok: false, error: "devices must be an array" };
  }
  if (record.devices.length > MAX_DEVICE_ROSTER_ENTRIES) {
    return { ok: false, error: "device list has too many devices" };
  }
  if (!isNonEmptyBase64(record.root_signature_b64)) {
    return { ok: false, error: "root_signature_b64 must be valid base64" };
  }

  const seen = new Set<string>();
  const devices: DeviceRosterEntry[] = [];
  for (const device of record.devices) {
    if (!device || typeof device !== "object" || Array.isArray(device)) {
      return { ok: false, error: "device entries must be objects" };
    }
    const entry = device as Record<string, unknown>;
    const entryExpected = ["device_id", "prekey_bundle"];
    const entryActual = Object.keys(entry).sort();
    if (
      entryActual.length !== entryExpected.length ||
      entryActual.some((field, index) => field !== entryExpected[index])
    ) {
      return { ok: false, error: "device entry fields are noncanonical" };
    }
    if (!isProtocolId(entry.device_id)) {
      return {
        ok: false,
        error: "device_id must be a bounded identifier without control characters",
      };
    }
    if (typeof entry.prekey_bundle !== "string" || entry.prekey_bundle.length === 0) {
      return { ok: false, error: "prekey_bundle is required" };
    }
    if (seen.has(entry.device_id)) {
      return { ok: false, error: "device_id must be unique" };
    }
    seen.add(entry.device_id);
    devices.push({
      device_id: entry.device_id,
      prekey_bundle: entry.prekey_bundle,
    });
  }

  return {
    ok: true,
    value: {
      user_id: record.user_id,
      version: record.version,
      devices,
      root_signature_b64: record.root_signature_b64,
    },
  };
}

/** Public delivery targets for an account.  The endpoint deliberately returns
 * no account identity key and no registration timing. */
export async function handleDevices(env: Env, userId: string): Promise<Response> {
  if (!isProtocolId(userId)) return notFound();
  const rows = await env.DB.prepare(
    "SELECT device_id, prekey_bundle FROM device_roster WHERE user_id = ? ORDER BY device_id ASC",
  ).bind(userId).all<{ device_id: string; prekey_bundle: string }>();
  return json(rows.results.map((row) => ({ device_id: row.device_id, prekey_bundle: row.prekey_bundle })));
}

export async function handleDevicesPost(request: Request, env: Env): Promise<Response> {
  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  const body = parseDeviceRosterBody(parsed);
  if (!body.ok) return badRequest(body.error);

  const identity = await getSignedIdentity(env.DB, body.value.user_id);
  if (!identity?.ik_root_ed25519_pub) return notFound();
  const message = canonicalDeviceListBytes({
    user_id: body.value.user_id,
    version: body.value.version,
    devices: body.value.devices,
  });
  const valid = await verifySignedRequest(
    identity.ik_root_ed25519_pub,
    message,
    body.value.root_signature_b64,
  );
  if (!valid) return unauthorized("device list signature invalid");

  const now = new Date().toISOString();
  const statements: D1PreparedStatement[] = [
    env.DB.prepare(
      `INSERT INTO device_roster_versions (user_id, version, updated_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(user_id) DO UPDATE SET
         version = excluded.version,
         updated_at = excluded.updated_at
       WHERE excluded.version > device_roster_versions.version`,
    ).bind(body.value.user_id, body.value.version, now),
    env.DB.prepare(
      `DELETE FROM device_roster
        WHERE user_id = ?1
          AND EXISTS (
            SELECT 1 FROM device_roster_versions
             WHERE user_id = ?1 AND version = ?2
          )`,
    ).bind(body.value.user_id, body.value.version),
  ];
  for (const device of body.value.devices) {
    statements.push(
      env.DB.prepare(
        `INSERT INTO device_roster
           (user_id, device_id, prekey_bundle, registered_at)
         SELECT ?1, ?2, ?3, ?4
          WHERE EXISTS (
            SELECT 1 FROM device_roster_versions
             WHERE user_id = ?1 AND version = ?5
          )`,
      ).bind(
        body.value.user_id,
        device.device_id,
        device.prekey_bundle,
        now,
        body.value.version,
      ),
    );
  }

  const results = await env.DB.batch(statements);
  if ((results[0]?.meta?.changes ?? 0) !== 1) {
    return conflict("device list version must go up");
  }
  return json({
    user_id: body.value.user_id,
    version: body.value.version,
    devices_stored: body.value.devices.length,
  });
}

function paddedDevicesResponse(
  rows: readonly { device_id: string; prekey_bundle: string }[],
): Response {
  const body: Record<string, unknown> = {
    devices: rows.map((row) => ({
      device_id: row.device_id,
      prekey_bundle: row.prekey_bundle,
    })),
    pad: "",
  };
  const baseline = new TextEncoder().encode(JSON.stringify(body)).length;
  const needed = DEVICE_LOOKUP_RESPONSE_BYTES - baseline;
  if (needed < 0) {
    return json({ error: "device roster exceeds the padded response bound" }, { status: 500 });
  }
  body.pad = "A".repeat(needed);
  return json(body, { status: 200 });
}

/** Public delivery targets for a live account. Hits, deleted accounts and
 * misses share one fixed response size, and the account id stays out of the
 * request path. */
export async function handleDevicesLookup(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 120, "devices-lookup-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try {
    body = await request.json() as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!isProtocolId(body.user_id)) return badRequest("user_id invalid");
  const rows = await env.DB.prepare(
    `SELECT r.device_id, r.prekey_bundle
       FROM device_roster r
      WHERE r.user_id = ?
        AND EXISTS (SELECT 1 FROM users u WHERE u.user_id = r.user_id)
      ORDER BY r.device_id ASC`,
  ).bind(body.user_id).all<{ device_id: string; prekey_bundle: string }>();
  return paddedDevicesResponse(rows.results);
}
