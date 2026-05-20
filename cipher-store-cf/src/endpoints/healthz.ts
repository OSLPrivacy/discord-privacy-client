/// Liveness probe. Returns the current epoch so external monitors
/// can confirm the worker is alive and the clock isn't wildly off.

import { json } from "../lib/http.js";

export function handleHealthz(): Response {
  return json({ ok: true, ts: Math.floor(Date.now() / 1000) });
}
