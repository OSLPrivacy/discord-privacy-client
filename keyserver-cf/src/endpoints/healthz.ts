import { json } from "../lib/http.js";

export function handleHealthz(): Response {
  return json({ ok: true });
}
