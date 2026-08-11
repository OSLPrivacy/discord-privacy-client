import { adaptPersonFacingResponse as keyserver } from "../keyserver-cf/src/lib/person-facing-result.ts";
import { adaptPersonFacingResponse as storage } from "../cipher-store-cf/src/lib/person-facing-result.ts";

const jsonResponse = (body, status = 200) => new Response(JSON.stringify(body), {
  status,
  headers: { "content-type": "application/json; charset=utf-8" },
});

const id = "0123456789abcdef0123456789abcdef";
const cases = [
  // Relay result vocabulary.
  [keyserver, "POST", "/v1/control-inbox", { ok: true }, 200],
  [keyserver, "POST", "/v1/control-inbox", { error: "recipient_inbox_full", scope: "sender_recipient" }, 429],
  [keyserver, "POST", "/v1/control-inbox", { error: "rate_limited" }, 429],
  [keyserver, "POST", "/v1/control-inbox", { error: "an English relay sentence" }, 503],

  // Key-server API results a packaged account flow can show.
  [keyserver, "POST", "/v1/register", { registered_at: "2026-08-11" }, 201],
  [keyserver, "POST", "/v1/register", { error: "rate_limited" }, 429],
  [keyserver, "POST", "/v1/usernames/claim", { username: "opaque" }, 201],
  [keyserver, "POST", "/v1/usernames/lookup", { error: "an English key-server sentence" }, 400],

  // Payment-voucher status and refusal vocabulary.
  [keyserver, "POST", "/v1/license/redeem", { status: "ACTIVE", checksum_ok: true }, 200],
  [keyserver, "POST", "/v1/license/redeem", { status: "REVOKED", message: "this code was refunded", checksum_ok: true }, 200],
  [keyserver, "POST", "/v1/license/redeem", { status: "UNKNOWN", checksum_ok: false }, 200],
  [keyserver, "POST", "/v1/license/redeem", { status: "UNREDEEMED", checksum_ok: true }, 200],
  [keyserver, "POST", "/v1/license/redeem", { error: "license code already redeemed" }, 409],
  [keyserver, "POST", "/v1/license/redeem", { error: "rate_limited" }, 429],
  [keyserver, "POST", "/v1/license/redeem", { error: "English payment sentence" }, 503],
  [keyserver, "POST", "/v1/license/validate", { status: "EXPIRED", checksum_ok: true }, 200],

  // Storage JSON results. Successful binary fetches and empty 204 deletes are
  // protocol bytes; their JSON refusals are the results Windows can render.
  [storage, "PUT", "/v1/blob", { id: "opaque" }, 201],
  [storage, "PUT", "/v1/blob", { error: "storage_capacity", message: "English capacity sentence" }, 503],
  [storage, "PUT", "/v1/blob", { error: "rate_limited", message: "English rate sentence" }, 429],
  [storage, "PUT", "/v1/blob", {
    error: "bad request sentence",
    message: "English failure sentence",
    context: { detail: "nested English detail", attempt: "1" },
  }, 400],
  [storage, "GET", `/v1/blob/${id}`, { error: "unavailable", message: "English unavailable sentence" }, 404],
  [storage, "POST", `/v1/blob/${id}/ack`, { error: "rate_limited", message: "English rate sentence" }, 429],
  [storage, "DELETE", `/v1/blob/${id}`, { error: "delete_grant_required", message: "English grant sentence" }, 403],
  [storage, "POST", "/v1/attachment", { id, size_bytes: 42 }, 201],
  [storage, "POST", "/v1/attachment/session", { id, size_bytes: 42 }, 201],
  [storage, "PUT", `/v1/attachment/${id}/part/1`, { part_number: 1, size_bytes: 42 }, 201],
  [storage, "POST", `/v1/attachment/${id}/complete`, { id, size_bytes: 42 }, 201],
  [storage, "GET", `/v1/attachment/${id}`, { error: "fetch_token_mismatch", message: "English token sentence" }, 403],
  [storage, "DELETE", `/v1/attachment/${id}`, { error: "rate_limited", message: "English rate sentence" }, 429],
  [storage, "POST", "/v1/link", { id, expires_at: 1 }, 201],
  [storage, "POST", `/v1/link/${id}/status`, { state: "created" }, 200],
  [storage, "DELETE", `/v1/link/${id}`, { error: "rate_limited", message: "English rate sentence" }, 429],
  [storage, "POST", `/v/${id}/fetch`, { error: "unavailable", message: "English unavailable sentence" }, 404],
];

const observations = [];
for (const [adapter, method, route, original, status] of cases) {
  const request = new Request(`https://service.invalid${route}`, { method });
  const response = await adapter(request, jsonResponse(original, status));
  observations.push({
    method,
    route,
    status: response.status,
    body: await response.json(),
    client_entry_point: "windows.resolve_person_service_result",
    resolver_calls: 1,
  });
}
process.stdout.write(JSON.stringify({ schema: "osl.windows-service-runtime-traffic.v1", observations }));
