import type { Env } from "../env.js";
import { badRequest, json, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";
import { isProtocolId } from "../lib/validation.js";

export const DEVICE_LOOKUP_RESPONSE_BYTES = 65536;

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

/** Public delivery targets for a live account.
 *
 * The user id is deliberately in the POST body rather than the request path so
 * edge request logs do not retain "this address looked up this account". Hits,
 * deleted accounts and misses all share status 200 and one fixed body size.
 */
export async function handleDevicesLookup(request: Request, env: Env): Promise<Response> {
  const rlIp = await checkRateLimit(env, callerIp(request), 120, "devices-lookup-ip");
  if (!rlIp.ok) return tooMany(rlIp.retryAfter);
  let body: Record<string, unknown>;
  try { body = await request.json() as Record<string, unknown>; }
  catch { return badRequest("malformed JSON body"); }
  if (!isProtocolId(body.user_id)) return badRequest("user_id invalid");
  const userId = body.user_id;
  const rows = await env.DB.prepare(
    `SELECT r.device_id, r.prekey_bundle
       FROM device_roster r
      WHERE r.user_id = ?
        AND EXISTS (SELECT 1 FROM users u WHERE u.user_id = r.user_id)
      ORDER BY r.device_id ASC`,
  ).bind(userId).all<{ device_id: string; prekey_bundle: string }>();
  return paddedDevicesResponse(rows.results);
}
