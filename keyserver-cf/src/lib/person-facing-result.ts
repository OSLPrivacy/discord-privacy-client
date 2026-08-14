export type PersonResultKind = "relay" | "key_server" | "payment_voucher";

export interface PersonResultRoute {
  kind: PersonResultKind;
  method: string;
  route: string;
}

export const PERSON_RESULT_ROUTES: readonly PersonResultRoute[] = [
  { kind: "relay", method: "POST", route: "/v1/control-inbox" },
  { kind: "key_server", method: "POST", route: "/v1/register" },
  { kind: "key_server", method: "POST", route: "/v1/usernames/claim" },
  { kind: "key_server", method: "POST", route: "/v1/usernames/lookup" },
  { kind: "payment_voucher", method: "POST", route: "/v1/license/redeem" },
  { kind: "payment_voucher", method: "POST", route: "/v1/license/validate" },
];

export function personResultRoute(request: Request): PersonResultRoute | undefined {
  const path = new URL(request.url).pathname;
  return PERSON_RESULT_ROUTES.find((route) => route.method === request.method && route.route === path);
}

const FORBIDDEN_PERSON_FIELDS = new Set([
  "error", "message", "detail", "description", "hint", "text",
]);

function stripPersonFields(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stripPersonFields);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .filter(([field]) => !FORBIDDEN_PERSON_FIELDS.has(field))
      .map(([field, child]) => [field, stripPersonFields(child)]),
  );
}

function stableCode(value: unknown): value is string {
  return typeof value === "string" && /^[a-z][a-z0-9_]*$/.test(value);
}

function stringParameters(value: unknown): Record<string, string> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const entries = Object.entries(value);
  if (!entries.every(([name, item]) => /^[a-z][a-z0-9_]*$/.test(name) && typeof item === "string")) {
    return undefined;
  }
  return Object.fromEntries(entries) as Record<string, string>;
}

function paymentCode(body: Record<string, unknown>, status: number): string {
  if (status === 409) return "payment_voucher_already_redeemed";
  if (status === 429) return "payment_voucher_rate_limited";
  const value = typeof body.status === "string" ? body.status.toLowerCase() : "";
  if (["active", "revoked", "expired", "unknown", "unredeemed"].includes(value)) {
    return `payment_voucher_${value}`;
  }
  return "payment_voucher_failed";
}

function resultCode(kind: PersonResultKind, body: Record<string, unknown>, status: number): string {
  // A handler that already speaks the stable contract owns its code. Passing
  // it through is important: an unknown future code must reach the strict
  // packaged-client resolver and fail visibly, not be hidden as "failed".
  if (stableCode(body.reason_code)) return body.reason_code;
  if (kind === "payment_voucher") return paymentCode(body, status);
  const oldCode = stableCode(body.error) ? body.error : "";
  if (kind === "relay") {
    if (oldCode === "recipient_inbox_full") return "relay_recipient_inbox_full";
    if (oldCode === "rate_limited") return "relay_rate_limited";
    return status >= 200 && status < 300 ? "relay_succeeded" : "relay_failed";
  }
  if (oldCode === "rate_limited") return "key_server_rate_limited";
  return status >= 200 && status < 300 ? "key_server_succeeded" : "key_server_failed";
}

function parameters(kind: PersonResultKind, code: string, body: Record<string, unknown>): Record<string, string> {
  const supplied = stringParameters(body.parameters);
  if (supplied !== undefined) return supplied;
  if (kind === "relay" && code === "relay_recipient_inbox_full") {
    const scope = body.scope === "recipient" || body.scope === "sender_recipient"
      ? body.scope
      : "recipient";
    return { scope };
  }
  return {};
}

function personResultResponse(
  route: PersonResultRoute,
  body: Record<string, unknown>,
  status: number,
  headers: HeadersInit,
): Response {
  const reason_code = resultCode(route.kind, body, status);
  const clean = stripPersonFields(Object.fromEntries(
    Object.entries(body).filter(([field]) =>
      field !== "reason_code" && field !== "parameters" && !FORBIDDEN_PERSON_FIELDS.has(field)
    ),
  )) as Record<string, unknown>;
  const outHeaders = new Headers(headers);
  outHeaders.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify({
    ...clean,
    reason_code,
    parameters: parameters(route.kind, reason_code, body),
  }), { status, headers: outHeaders });
}

/// Convert only classified person-visible service traffic. Protocol-only
/// responses and operator logs retain their existing contracts. English
/// `error`/`message` fields are removed before bytes leave the Worker.
export async function adaptPersonFacingResponse(request: Request, response: Response): Promise<Response> {
  const route = personResultRoute(request);
  if (!route) return response;
  const contentType = response.headers.get("content-type") ?? "";
  let body: Record<string, unknown>;
  if (contentType.includes("application/json")) try {
    const parsed = await response.clone().json();
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return personResultResponse(route, {}, response.status, response.headers);
    }
    body = parsed as Record<string, unknown>;
  } catch {
    return personResultResponse(route, {}, response.status, response.headers);
  } else {
    // A classified API route must never leak a text/HTML sentence because a
    // handler forgot the JSON helper. Preserve only status and headers.
    body = {};
  }
  return personResultResponse(route, body, response.status, response.headers);
}
