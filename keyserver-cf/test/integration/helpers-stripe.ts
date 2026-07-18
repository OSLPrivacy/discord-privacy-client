/// Stripe webhook test helpers. Signs an event body with the same
/// HMAC algorithm Stripe uses so the worker's verifyWebhookSignature
/// accepts it.

const WEBHOOK_SECRET = "whsec_test_secret";

export interface BuildEventInput {
  id: string;
  type: string;
  data: { object: Record<string, unknown> };
  livemode?: boolean;
  created?: number;
}

export async function signStripeWebhook(
  rawBody: string,
  secret: string = WEBHOOK_SECRET,
  unixSec: number = Math.floor(Date.now() / 1000),
): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const payload = `${unixSec}.${rawBody}`;
  const sigBuf = await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(payload),
  );
  const hex = bytesToHex(new Uint8Array(sigBuf));
  return `t=${unixSec},v1=${hex}`;
}

function bytesToHex(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex;
}

export function uniqueEventId(prefix = "evt"): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

export function uniqueSubId(prefix = "sub"): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

export async function postSignedWebhook(
  self: { fetch: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response> },
  event: BuildEventInput,
): Promise<Response> {
  const body = JSON.stringify({
    livemode: true,
    created: Math.floor(Date.now() / 1000),
    ...event,
  });
  const sig = await signStripeWebhook(body);
  return await self.fetch("http://test/v1/stripe/webhook", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "stripe-signature": sig,
    },
    body,
  });
}
