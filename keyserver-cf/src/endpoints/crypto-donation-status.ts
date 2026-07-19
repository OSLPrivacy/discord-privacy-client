import type { Env } from "../env.js";
import {
  cryptoDonationClaimMatches,
  getCryptoDonationInvoice,
} from "../lib/crypto-donations.js";
import { badRequest, forbidden, gone, json, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCryptoDonationStatus(
  request: Request,
  env: Env,
): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 10, "crypto-donation-status-v1");
  if (!limit.ok) return tooMany(limit.retryAfter);
  let parsedBody: unknown;
  try {
    parsedBody = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsedBody || typeof parsedBody !== "object" || Array.isArray(parsedBody)) {
    return badRequest("JSON body must be an object");
  }
  const body = parsedBody as { invoice_id?: unknown; claim_token?: unknown };
  const allowedKeys = new Set(["invoice_id", "claim_token"]);
  if (Object.keys(body).some((key) => !allowedKeys.has(key)) || Object.keys(body).length !== 2) {
    return badRequest("unexpected status field");
  }
  if (typeof body.invoice_id !== "string" || !/^cdon_[0-9a-f]{32}$/.test(body.invoice_id)) {
    return badRequest("invoice_id malformed");
  }
  if (typeof body.claim_token !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(body.claim_token)) {
    return badRequest("claim_token malformed");
  }
  const invoice = await getCryptoDonationInvoice(env.DB, body.invoice_id);
  if (!invoice || !(await cryptoDonationClaimMatches(invoice, body.claim_token))) {
    return forbidden("invalid invoice claim");
  }
  if (invoice.status === "expired") return gone("invoice expired");
  return json({
    invoice_id: invoice.invoice_id,
    status: invoice.status === "recorded" ? "recorded" : "pending",
    payment_method: invoice.payment_method,
    amount_usd_cents: invoice.amount_usd_cents,
    expires_at: invoice.expires_at,
  });
}
