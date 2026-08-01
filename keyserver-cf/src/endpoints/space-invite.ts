/// Opaque, one-time Space invite redemption ledger.
///
/// The encrypted invite itself is exchanged out of band. Keeping it out of
/// this Worker means the service never learns which Space or people an invite
/// connects. Possession is authorization for both capabilities: the invite
/// capability consumes once; the distinct revocation capability deletes the
/// live row immediately, including while the inviter is offline.

import type { Env } from "../env.js";
import { badRequest, conflict, notFound } from "../lib/http.js";
import { isHighEntropyRequestId } from "../lib/validation.js";

const MAX_INVITE_LIFETIME_SECONDS = 7 * 24 * 60 * 60;

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

async function capabilityHash(capability: string): Promise<Uint8Array> {
  const padded = capability.replaceAll("-", "+").replaceAll("_", "/") + "=";
  const raw = atob(padded);
  const bytes = Uint8Array.from(raw, (value) => value.charCodeAt(0));
  return new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
}

async function jsonBody(request: Request): Promise<Record<string, unknown> | Response> {
  try {
    const body = await request.json();
    if (!body || typeof body !== "object" || Array.isArray(body)) {
      return badRequest("JSON object required");
    }
    return body as Record<string, unknown>;
  } catch {
    return badRequest("malformed JSON body");
  }
}

/** POST /v1/space-invite — register an already out-of-band invite. */
export async function handleSpaceInviteIssue(request: Request, env: Env): Promise<Response> {
  const body = await jsonBody(request);
  if (body instanceof Response) return body;
  const capability = body.capability;
  const revocationCapability = body.revocation_capability;
  const expiresAt = body.expires_at;
  const now = nowSeconds();
  if (!isHighEntropyRequestId(capability) || !isHighEntropyRequestId(revocationCapability)) {
    return badRequest("capabilities must be 256-bit base64url values");
  }
  if (capability === revocationCapability) {
    return badRequest("invite and revocation capabilities must differ");
  }
  if (
    typeof expiresAt !== "number" ||
    !Number.isSafeInteger(expiresAt) ||
    expiresAt <= now ||
    expiresAt > now + MAX_INVITE_LIFETIME_SECONDS
  ) {
    return badRequest("expires_at must be within the next seven days");
  }

  try {
    await env.DB.prepare(
      `INSERT INTO space_invites (invite_hash, revocation_hash, expires_at)
       VALUES (?, ?, ?)`,
    ).bind(
      await capabilityHash(capability),
      await capabilityHash(revocationCapability),
      expiresAt,
    ).run();
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    if (detail.includes("UNIQUE") || detail.includes("PRIMARY")) {
      return conflict("invite capability already registered");
    }
    console.error("[space-invite] issue failed");
    return new Response(JSON.stringify({ error: "invite issuance unavailable" }), { status: 503 });
  }
  return new Response(null, { status: 201 });
}

/** POST /v1/space-invite/consume — atomically spend the bearer capability. */
export async function handleSpaceInviteConsume(request: Request, env: Env): Promise<Response> {
  const body = await jsonBody(request);
  if (body instanceof Response) return body;
  const capability = body.capability;
  if (!isHighEntropyRequestId(capability)) {
    return badRequest("capability must be a 256-bit base64url value");
  }
  const row = await env.DB.prepare(
    `DELETE FROM space_invites
      WHERE invite_hash = ? AND expires_at > ?
      RETURNING invite_hash`,
  ).bind(await capabilityHash(capability), nowSeconds()).first();
  // Identical refusal for unknown, expired, spent and revoked values prevents
  // the endpoint from becoming an invitation-status oracle.
  if (!row) return notFound("invite unavailable");
  return new Response(null, { status: 204 });
}

/** DELETE /v1/space-invite — revoke by its distinct bearer capability. */
export async function handleSpaceInviteRevoke(request: Request, env: Env): Promise<Response> {
  const body = await jsonBody(request);
  if (body instanceof Response) return body;
  const revocationCapability = body.revocation_capability;
  if (!isHighEntropyRequestId(revocationCapability)) {
    return badRequest("revocation_capability must be a 256-bit base64url value");
  }
  // Idempotent and deliberately non-enumerating: an unavailable invite is
  // already the desired revocation result.
  await env.DB.prepare("DELETE FROM space_invites WHERE revocation_hash = ?")
    .bind(await capabilityHash(revocationCapability))
    .run();
  return new Response(null, { status: 204 });
}
