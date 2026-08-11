export interface StoragePersonRoute {
  method: string;
  route: RegExp;
}

export const STORAGE_PERSON_ROUTES: readonly StoragePersonRoute[] = [
  { method: "PUT", route: /^\/v1\/blob$/ },
  { method: "GET", route: /^\/v1\/blob\/[0-9a-f]+$/i },
  { method: "POST", route: /^\/v1\/blob\/[0-9a-f]{32}\/ack$/ },
  { method: "DELETE", route: /^\/v1\/blob\/[0-9a-f]+$/i },
  { method: "POST", route: /^\/v1\/attachment$/ },
  { method: "POST", route: /^\/v1\/attachment\/session$/ },
  { method: "PUT", route: /^\/v1\/attachment\/[0-9a-f]{32}\/part\/\d+$/ },
  { method: "POST", route: /^\/v1\/attachment\/[0-9a-f]{32}\/complete$/ },
  { method: "GET", route: /^\/v1\/attachment\/[0-9a-f]+$/ },
  { method: "DELETE", route: /^\/v1\/attachment\/[0-9a-f]+$/ },
  { method: "POST", route: /^\/v1\/link$/ },
  { method: "POST", route: /^\/v1\/link\/[0-9a-f]{32}\/status$/ },
  { method: "DELETE", route: /^\/v1\/link\/[0-9a-f]{32}$/ },
  { method: "POST", route: /^\/v\/[^/]{1,128}\/fetch$/ },
];

export function isStoragePersonRoute(request: Request): boolean {
  const path = new URL(request.url).pathname;
  return STORAGE_PERSON_ROUTES.some((route) => route.method === request.method && route.route.test(path));
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

export async function adaptPersonFacingResponse(request: Request, response: Response): Promise<Response> {
  if (!isStoragePersonRoute(request)) return response;
  const contentType = response.headers.get("content-type") ?? "";
  // Successful fetches are opaque bytes and successful deletes are empty 204s:
  // those are protocol results, not words. JSON and textual responses on the
  // same deployed routes are person-visible failures and must be catalogued.
  if (!contentType.includes("application/json") && (response.ok || response.status === 204)) {
    return response;
  }
  let body: Record<string, unknown>;
  if (contentType.includes("application/json")) try {
    const parsed = await response.clone().json();
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) body = {};
    else body = parsed as Record<string, unknown>;
  } catch {
    body = {};
  } else body = {};
  const suppliedCode = typeof body.reason_code === "string" && /^[a-z][a-z0-9_]*$/.test(body.reason_code)
    ? body.reason_code
    : undefined;
  const oldCode = typeof body.error === "string" && /^[a-z][a-z0-9_]*$/.test(body.error) ? body.error : "";
  const reason_code = suppliedCode ?? (response.ok
    ? "storage_succeeded"
    : oldCode === "storage_capacity"
      ? "storage_capacity"
      : oldCode === "rate_limited"
        ? "storage_rate_limited"
        : "storage_failed");
  const suppliedParameters = body.parameters && typeof body.parameters === "object" && !Array.isArray(body.parameters)
    && Object.entries(body.parameters).every(([name, value]) => /^[a-z][a-z0-9_]*$/.test(name) && typeof value === "string")
    ? body.parameters as Record<string, string>
    : {};
  const clean = stripPersonFields(Object.fromEntries(
    Object.entries(body).filter(([field]) =>
      field !== "reason_code" && field !== "parameters"
      && !FORBIDDEN_PERSON_FIELDS.has(field)
    ),
  )) as Record<string, unknown>;
  const headers = new Headers(response.headers);
  headers.set("content-type", "application/json; charset=utf-8");
  return new Response(JSON.stringify({ ...clean, reason_code, parameters: suppliedParameters }), {
    status: response.status,
    headers,
  });
}
