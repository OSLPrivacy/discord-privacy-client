/// POST /v1/crypto/submit
///
/// Body: { payment_id: string, txid: string }
/// Returns: { ok: true, status: "awaiting" }
///
/// User-driven: after sending the displayed amount to the displayed
/// address, they submit the txid. We transition `quoted` →
/// `awaiting`. Admin endpoint (separate route) confirms after
/// manual review and issues the license.
///
/// No txid format validation — both BTC and XMR have different
/// shapes, and a typo gets caught at manual review.

import type { Env } from "../env.js";
import { submitCryptoTxid } from "../lib/crypto-payments.js";
import { badRequest, json, notFound, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCryptoSubmit(
  request: Request,
  env: Env,
): Promise<Response> {
  const rl = await checkRateLimit(env, callerIp(request), 10);
  if (!rl.ok) return tooMany(rl.retryAfter);

  let body: { payment_id?: unknown; txid?: unknown };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.payment_id !== "string" || body.payment_id.length === 0) {
    return badRequest("payment_id required");
  }
  if (typeof body.txid !== "string" || body.txid.length < 4 || body.txid.length > 256) {
    return badRequest("txid must be 4-256 chars");
  }
  const ok = await submitCryptoTxid(env.DB, body.payment_id, body.txid);
  if (!ok) {
    return notFound("payment_id not in 'quoted' state (already submitted or unknown)");
  }
  return json({ ok: true, status: "awaiting" });
}
