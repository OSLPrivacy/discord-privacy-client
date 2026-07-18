import type { Env } from "../env.js";
import {
  acknowledgeCryptoDelivery,
  claimMatches,
  getAnonymousInvoice,
  markCryptoDeliveryFetched,
} from "../lib/anonymous-crypto.js";
import { badRequest, forbidden, gone, json, notFound, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCryptoStatus(request: Request, env: Env): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 10, "crypto-status-v2");
  if (!limit.ok) return tooMany(limit.retryAfter);
  let body: {
    invoice_id?: unknown;
    claim_token?: unknown;
    acknowledge_delivery?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.invoice_id !== "string" || !/^cpay_[0-9a-f]{32}$/.test(body.invoice_id)) {
    return badRequest("invoice_id malformed");
  }
  if (typeof body.claim_token !== "string" || body.claim_token.length !== 43) {
    return badRequest("claim_token malformed");
  }
  if (body.acknowledge_delivery !== undefined && body.acknowledge_delivery !== true) {
    return badRequest("acknowledge_delivery must be true when provided");
  }
  const invoice = await getAnonymousInvoice(env.DB, body.invoice_id);
  if (!invoice) return notFound("unknown invoice");
  if (!(await claimMatches(invoice, body.claim_token))) return forbidden("invalid claim token");
  if (body.acknowledge_delivery === true) {
    const result = await acknowledgeCryptoDelivery(env.DB, invoice.invoice_id);
    if (result === "not_ready") return gone("crypto delivery is not ready");
    return json({ status: "acknowledged", already_acknowledged: result === "already_acknowledged" });
  }
  if (invoice.acknowledged_at !== null) return gone("crypto delivery already acknowledged");
  if (invoice.status === "delivery_ready" && invoice.encrypted_license) {
    await markCryptoDeliveryFetched(env.DB, invoice.invoice_id);
  }
  return json({
    invoice_id: invoice.invoice_id,
    status: invoice.status,
    expires_at: invoice.expires_at,
    encrypted_license: invoice.encrypted_license,
    delivery: invoice.encrypted_license ? "rsa-oaep-sha256" : null,
  });
}
