import type { Env } from "../env.js";
import { badRequest, serviceUnavailable } from "../lib/http.js";

/**
 * Privacy boundary for a future cloud-carrier credit spend.
 *
 * A conventional server-side ledger would bind every debit to an account and
 * timestamp, which is a protected-message usage profile.  A signed bearer
 * balance is not a substitute: without a spent-token set its pre-decrement
 * form can be replayed indefinitely.  Until the product has an approved
 * anonymous, replay-safe e-cash issuer, fail closed rather than quietly
 * choosing either surveillance or unenforceable credits.
 */
export async function handleCreditSpend(request: Request, _env: Env): Promise<Response> {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return badRequest("invalid JSON body");
  }

  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    return badRequest("invalid credit spend request");
  }

  // Identity-bearing fields must never become an accidental metering API.
  const forbiddenIdentityFields = ["user_id", "identity", "account_id", "conversation_id"];
  if (forbiddenIdentityFields.some((field) => field in body)) {
    return badRequest("credit spend requests must not contain an identity");
  }

  return serviceUnavailable("anonymous replay-safe credit metering is not configured");
}
