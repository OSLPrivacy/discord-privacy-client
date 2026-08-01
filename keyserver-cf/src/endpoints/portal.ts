/// POST /v1/billing-portal-session
///
/// The billing portal was retired with the move to one-time licenses. Keep a
/// deliberate endpoint response so clients get an actionable answer rather
/// than mistaking the removed flow for a transient failure.

import type { Env } from "../env.js";
import { gone } from "../lib/http.js";

export function handleBillingPortal(_request: Request, _env: Env): Response {
  return gone("billing portal is no longer available");
}
