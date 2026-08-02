import type { Env } from "../env.js";
import { json, notFound } from "../lib/http.js";
import { isProtocolId } from "../lib/validation.js";

/** Public delivery targets for an account.  The endpoint deliberately returns
 * no account identity key and no registration timing.  Pairing/linking owns
 * writes to `device_roster`; this read surface is safe to ship independently. */
export async function handleDevices(env: Env, userId: string): Promise<Response> {
  if (!isProtocolId(userId)) return notFound();
  const rows = await env.DB.prepare(
    "SELECT device_id, prekey_bundle FROM device_roster WHERE user_id = ? ORDER BY device_id ASC",
  ).bind(userId).all<{ device_id: string; prekey_bundle: string }>();
  return json(rows.results.map((row) => ({ device_id: row.device_id, prekey_bundle: row.prekey_bundle })));
}
