/// Liveness probe. Returns the current epoch so external monitors
/// can confirm the worker is alive and the clock isn't wildly off.

import type { Env } from "../env.js";
import { json } from "../lib/http.js";

const STORAGE_ACK_CAPABILITY = "storage_ack_v1";

async function hasStorageAckSchema(env: Env): Promise<boolean> {
  try {
    const row = await env.DB.prepare(
      "SELECT 1 FROM worker_schema_capabilities WHERE capability = ? AND version >= 1 LIMIT 1",
    ).bind(STORAGE_ACK_CAPABILITY).first();
    return row !== null;
  } catch {
    // An old database has neither the marker table nor this capability. Health
    // remains a liveness response, but it must not advertise a Worker/schema
    // combination that cannot safely serve receipt-aware payloads.
    return false;
  }
}

export async function handleHealthz(env: Env): Promise<Response> {
  const capabilities = await hasStorageAckSchema(env)
    ? [STORAGE_ACK_CAPABILITY]
    : [];
  return json({ ok: true, ts: Math.floor(Date.now() / 1000), capabilities });
}
