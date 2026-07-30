import type { Env } from "../../env.js";
import { json } from "../../lib/http.js";

/**
 * Artifact A identifies itself without probing migration 0031. A 0 capability
 * is intentional: the bridge preserves the non-inbox Worker surface while the
 * status-aware schema and final Worker are staged.
 */
export async function handleHealthz(_env: Env): Promise<Response> {
  return json({
    ok: true,
    readiness_artifact: "A-pre-0031-bridge",
  });
}
