import type { Env } from "../../env.js";
import { serviceUnavailable } from "../../lib/http.js";

const BRIDGE_UNAVAILABLE =
  "control inbox unavailable during schema transition";

/**
 * Artifact A deliberately has no control-inbox implementation. Keeping these
 * handlers free of all D1 access lets migration 0031 be applied while the
 * bridge is active without either reading or writing its status surfaces.
 */
export async function handleControlInboxPost(
  _request: Request,
  _env: Env,
): Promise<Response> {
  return serviceUnavailable(BRIDGE_UNAVAILABLE);
}

export async function handleControlInboxGet(
  _request: Request,
  _env: Env,
  _userId: string,
): Promise<Response> {
  return serviceUnavailable(BRIDGE_UNAVAILABLE);
}

export async function handleControlInboxDelete(
  _request: Request,
  _env: Env,
  _inboxIdHex: string,
): Promise<Response> {
  return serviceUnavailable(BRIDGE_UNAVAILABLE);
}
