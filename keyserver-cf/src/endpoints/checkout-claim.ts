import type { Env } from "../env.js";
import {
  acknowledgeStripeClaimDelivery,
  getStripeCheckoutClaim,
  markStripeClaimFetched,
  stripeClaimMatches,
  validClaimToken,
} from "../lib/stripe-checkout-claims.js";
import { badRequest, forbidden, gone, json, notFound, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleCheckoutClaim(request: Request, env: Env): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 120, "stripe-checkout-claim");
  if (!limit.ok) return tooMany(limit.retryAfter);

  let body: {
    session_id?: unknown;
    claim_token?: unknown;
    acknowledge_delivery?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (typeof body.session_id !== "string" || !/^cs_(?:live|test)_[A-Za-z0-9_]+$/.test(body.session_id)) {
    return badRequest("session_id malformed");
  }
  if (!validClaimToken(body.claim_token)) return badRequest("claim_token malformed");

  const claim = await getStripeCheckoutClaim(env.DB, body.session_id);
  if (!claim) return notFound("unknown checkout");
  if (!(await stripeClaimMatches(claim, body.claim_token))) {
    return forbidden("invalid claim token");
  }
  if (body.acknowledge_delivery !== undefined && body.acknowledge_delivery !== true) {
    return badRequest("acknowledge_delivery must be true when provided");
  }
  if (claim.status === "expired" || claim.expires_at < Math.floor(Date.now() / 1000)) {
    return gone("checkout claim expired");
  }
  if (claim.status !== "delivery_ready") {
    return json({ status: "pending", retry_after_seconds: 2 });
  }
  if (body.acknowledge_delivery === true) {
    const result = await acknowledgeStripeClaimDelivery(env.DB, claim.session_id);
    if (result === "not_ready") return gone("checkout claim is not deliverable");
    return json({ status: "acknowledged", already_acknowledged: result === "already_acknowledged" });
  }
  if (claim.acknowledged_at !== null) return gone("checkout claim already acknowledged");

  await markStripeClaimFetched(env.DB, claim.session_id);
  return json({
    status: "delivery_ready",
    encrypted_license: claim.encrypted_license,
    delivery: "rsa-oaep-sha256",
  });
}
