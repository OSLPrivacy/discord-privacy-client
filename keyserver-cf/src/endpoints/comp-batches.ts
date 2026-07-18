import type { Env } from "../env.js";
import { checkCompAdminTokens } from "../lib/auth.js";
import { issueCompBatch, revokeCompBatch } from "../lib/comp-batches.js";
import { badRequest, conflict, json, notFound, serviceUnavailable } from "../lib/http.js";

export async function handleCompBatchIssue(request: Request, env: Env): Promise<Response> {
  const denied = await checkCompAdminTokens(request, env);
  if (denied) return denied;
  if (!env.COMP_AUDIT_HMAC_SECRET) {
    return serviceUnavailable("comp audit authorization not configured");
  }
  const issuer = env.DEPLOYMENT_ENV === "qa" ? "qa" : "production";
  const licenseHmacSecret = issuer === "qa"
    ? env.QA_LICENSE_HMAC_SECRET
    : env.LICENSE_HMAC_SECRET;
  if (!licenseHmacSecret) return serviceUnavailable("license issuer not configured");
  if (
    issuer === "qa" &&
    env.LICENSE_HMAC_SECRET &&
    env.LICENSE_HMAC_SECRET === env.QA_LICENSE_HMAC_SECRET
  ) {
    return serviceUnavailable("QA license trust root is not isolated");
  }

  let body: {
    quantity?: unknown;
    purpose?: unknown;
    expires_at?: unknown;
    request_id?: unknown;
    delivery_public_key_spki?: unknown;
  };
  try {
    body = (await request.json()) as typeof body;
  } catch {
    return badRequest("malformed JSON body");
  }
  if (
    typeof body.quantity !== "number" ||
    typeof body.purpose !== "string" ||
    typeof body.expires_at !== "number" ||
    typeof body.request_id !== "string" ||
    typeof body.delivery_public_key_spki !== "string"
  ) {
    return badRequest("quantity, purpose, expires_at, request_id, and delivery_public_key_spki are required");
  }

  try {
    const batch = await issueCompBatch(env.DB, {
      quantity: body.quantity,
      purpose: body.purpose,
      expiresAt: body.expires_at,
      requestId: body.request_id,
      deliveryPublicKeySpki: body.delivery_public_key_spki,
      issuer,
      licenseHmacSecret,
      auditHmacSecret: env.COMP_AUDIT_HMAC_SECRET,
    });
    return json({
      batch_id: batch.batchId,
      quantity: batch.quantity,
      expires_at: batch.expiresAt,
      audit_digest: batch.auditDigest,
      delivery: batch.delivery,
    }, { status: 201 });
  } catch (error) {
    const message = error instanceof Error ? error.message : "comp batch issuance failed";
    if (message.includes("already been used")) return conflict(message);
    if (
      message.includes("quantity") ||
      message.includes("purpose") ||
      message.includes("request_id") ||
      message.includes("expires_at") ||
      message.includes("delivery public key")
    ) {
      return badRequest(message);
    }
    console.error("[comp] batch issuance failed");
    return serviceUnavailable("comp batch issuance failed");
  }
}

export async function handleCompBatchRevoke(
  request: Request,
  env: Env,
  batchId: string,
): Promise<Response> {
  const denied = await checkCompAdminTokens(request, env);
  if (denied) return denied;
  if (!/^comp_[0-9a-f]{32}$/.test(batchId)) return badRequest("batch_id malformed");
  const revoked = await revokeCompBatch(env.DB, batchId);
  if (!revoked) return notFound("unknown comp batch");
  return json({ ok: true, batch_id: batchId, status: "revoked" });
}
