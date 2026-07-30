/// Optional cloud AutoScrub admission.
///
/// The Worker is never the authority that decides what local account/scope may
/// be processed. It only consumes a native-issued, attended run authority and
/// refuses to mint or infer one from caller-supplied cloud fields.

import { badRequest, json } from "../lib/http.js";
import { decodeBase64, isPlainString } from "../lib/validation.js";

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

export interface NativeAutoScrubRunAuthority {
  native_run_id: string;
  attended_authority_id: string;
  operator_attended: true;
  scope_commitment_b64: string;
}

export interface AcceptedCloudAutoScrubRun {
  status: "accepted";
  native_run_id: string;
  scope_commitment_b64: string;
  cloud_authority_minted: false;
}

export function validateNativeAutoScrubRunAuthority(
  value: unknown,
): NativeAutoScrubRunAuthority | string {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return "native_run_authority is required";
  }
  const authority = value as Record<string, unknown>;
  if (!isPlainString(authority.native_run_id) || !UUID_RE.test(authority.native_run_id)) {
    return "native_run_authority.native_run_id must be a UUID";
  }
  if (
    !isPlainString(authority.attended_authority_id) ||
    !UUID_RE.test(authority.attended_authority_id)
  ) {
    return "native_run_authority.attended_authority_id must be a UUID";
  }
  if (authority.operator_attended !== true) {
    return "native_run_authority.operator_attended must be true";
  }
  if (!isPlainString(authority.scope_commitment_b64)) {
    return "native_run_authority.scope_commitment_b64 must be base64";
  }
  try {
    if (decodeBase64(authority.scope_commitment_b64).length !== 32) {
      return "native_run_authority.scope_commitment_b64 wrong length";
    }
  } catch {
    return "native_run_authority.scope_commitment_b64 must be base64";
  }
  return {
    native_run_id: authority.native_run_id,
    attended_authority_id: authority.attended_authority_id,
    operator_attended: true,
    scope_commitment_b64: authority.scope_commitment_b64,
  };
}

export async function handleCloudAutoScrub(request: Request): Promise<Response> {
  let parsed: unknown;
  try {
    parsed = await request.json();
  } catch {
    return badRequest("malformed JSON body");
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return badRequest("cloud AutoScrub body must be an object");
  }
  const body = parsed as Record<string, unknown>;
  if ("cloud_authority" in body || "cloud_authority_minted" in body) {
    return badRequest("cloud AutoScrub cannot mint or accept cloud authority");
  }
  const authority = validateNativeAutoScrubRunAuthority(body.native_run_authority);
  if (typeof authority === "string") {
    return badRequest(authority);
  }
  return json({
    status: "accepted",
    native_run_id: authority.native_run_id,
    scope_commitment_b64: authority.scope_commitment_b64,
    cloud_authority_minted: false,
  } satisfies AcceptedCloudAutoScrubRun);
}
