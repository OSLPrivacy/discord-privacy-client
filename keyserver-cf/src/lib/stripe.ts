/// Thin Stripe REST API wrapper. The official `stripe` npm package
/// works on Workers via `createFetchHttpClient()` but adds ~80 KB
/// of bundle and an extra abstraction layer; on Workers it's
/// cleaner to talk to Stripe's REST API directly with `fetch`.
///
/// Two surfaces:
///   - createCheckoutSession() — POST /v1/checkout/sessions
///   - createBillingPortalSession() — POST /v1/billing_portal/sessions
///
/// Stripe expects form-urlencoded request bodies on these endpoints
/// (it parses the same shape as their official SDK's positional
/// args — flattened nested keys like `line_items[0][price]`).

const STRIPE_API = "https://api.stripe.com/v1";

export interface CheckoutSessionInput {
  priceId: string;
  /** Either 'subscription' or 'payment'; we always use subscription. */
  successUrl: string;
  cancelUrl: string;
  /** Optional Stripe customer_id; lets us track repeat purchasers. */
  customerEmail?: string;
  /** Optional metadata stored on the resulting subscription. */
  metadata?: Record<string, string>;
}

export interface CheckoutSession {
  id: string;
  url: string;
  customer?: string | null;
}

export async function createCheckoutSession(
  secretKey: string,
  input: CheckoutSessionInput,
  fetcher: typeof fetch = fetch,
): Promise<CheckoutSession> {
  const form = new URLSearchParams();
  form.set("mode", "subscription");
  form.set("line_items[0][price]", input.priceId);
  form.set("line_items[0][quantity]", "1");
  form.set("success_url", input.successUrl);
  form.set("cancel_url", input.cancelUrl);
  if (input.customerEmail) form.set("customer_email", input.customerEmail);
  if (input.metadata) {
    for (const [k, v] of Object.entries(input.metadata)) {
      form.set(`metadata[${k}]`, v);
    }
  }

  const res = await fetcher(`${STRIPE_API}/checkout/sessions`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${secretKey}`,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: form.toString(),
  });
  if (!res.ok) {
    throw new StripeError(
      `Stripe checkout session failed: ${res.status} ${await res.text()}`,
    );
  }
  return (await res.json()) as CheckoutSession;
}

export interface BillingPortalSessionInput {
  customerId: string;
  returnUrl: string;
}

export interface BillingPortalSession {
  id: string;
  url: string;
}

export async function createBillingPortalSession(
  secretKey: string,
  input: BillingPortalSessionInput,
  fetcher: typeof fetch = fetch,
): Promise<BillingPortalSession> {
  const form = new URLSearchParams();
  form.set("customer", input.customerId);
  form.set("return_url", input.returnUrl);

  const res = await fetcher(`${STRIPE_API}/billing_portal/sessions`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${secretKey}`,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: form.toString(),
  });
  if (!res.ok) {
    throw new StripeError(
      `Stripe billing portal session failed: ${res.status} ${await res.text()}`,
    );
  }
  return (await res.json()) as BillingPortalSession;
}

export class StripeError extends Error {}

/** Webhook signature verification.
 *
 * Stripe sends a `stripe-signature` header of shape:
 *   `t=<unix>,v1=<hex>,v1=<hex>,...`
 *
 * Compute HMAC-SHA256(secret, `${t}.${rawBody}`) and verify against
 * any of the v1 hashes. Returns true on success, false on any
 * failure (malformed header, signature mismatch, timestamp skew
 * beyond `toleranceSec`).
 *
 * `rawBody` must be the literal request body — JSON-stringified
 * after parse will NOT match because Stripe signs the original
 * byte stream.
 */
export async function verifyWebhookSignature(args: {
  rawBody: string;
  signatureHeader: string | null;
  secret: string;
  toleranceSec?: number;
  /** Override "now" for tests. Defaults to current wallclock. */
  nowUnix?: number;
}): Promise<boolean> {
  if (!args.signatureHeader) return false;
  const tolerance = args.toleranceSec ?? 300; // Stripe default: 5 min
  const now = args.nowUnix ?? Math.floor(Date.now() / 1000);

  let timestamp: number | null = null;
  const v1Hashes: string[] = [];
  for (const part of args.signatureHeader.split(",")) {
    const [k, v] = part.split("=");
    if (!k || !v) continue;
    if (k.trim() === "t") timestamp = Number.parseInt(v.trim(), 10);
    else if (k.trim() === "v1") v1Hashes.push(v.trim());
  }
  if (timestamp === null || Number.isNaN(timestamp)) return false;
  if (Math.abs(now - timestamp) > tolerance) return false;
  if (v1Hashes.length === 0) return false;

  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(args.secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const payload = `${timestamp}.${args.rawBody}`;
  const sigBuf = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(payload),
  );
  const computed = bytesToHex(new Uint8Array(sigBuf));
  // Constant-time compare. Any match wins.
  let any = false;
  for (const v1 of v1Hashes) {
    if (constantTimeStringEqual(v1, computed)) any = true;
  }
  return any;
}

function bytesToHex(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex;
}

function constantTimeStringEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

/** Parse a Stripe event JSON envelope. We don't validate the full
 *  Stripe schema — we extract the fields we actually use. */
export interface StripeEvent {
  id: string;
  type: string;
  data: { object: Record<string, unknown> };
  created?: number;
}

export function parseEvent(rawBody: string): StripeEvent | null {
  try {
    const j = JSON.parse(rawBody) as Partial<StripeEvent>;
    if (
      typeof j.id !== "string" ||
      typeof j.type !== "string" ||
      !j.data ||
      typeof j.data.object !== "object"
    ) {
      return null;
    }
    return j as StripeEvent;
  } catch {
    return null;
  }
}
