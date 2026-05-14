import type { Env } from "../env.js";
import { serviceUnavailable } from "../lib/http.js";

export function handleSelectorManifest(env: Env): Response {
  const raw = env.SELECTOR_MANIFEST_JSON;
  if (!raw || raw.length === 0) {
    return serviceUnavailable("selector manifest not configured on this keyserver");
  }
  // Server stores the envelope as a string (the secret value is the
  // signed-manifest JSON). Pass through verbatim — clients verify
  // the embedded Ed25519 signature against their release-hardcoded
  // key. The keyserver is a delivery channel, not a trust anchor.
  return new Response(raw, {
    status: 200,
    headers: { "content-type": "application/json; charset=utf-8" },
  });
}
