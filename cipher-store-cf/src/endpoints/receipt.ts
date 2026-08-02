/// Identity-free durable-receipt endpoint for opaque payloads.
import type { Env } from "../env.js";
import { constantTimeEqualHex, sha256Hex } from "../lib/digest.js";
import { notFound } from "../lib/http.js";
import { R2PayloadStore } from "../lib/payload-store.js";

const ACK_CAP_RE = /^[0-9a-f]{32}$/;
const ID_RE = /^[0-9a-f]{32}$/;

type AckRow = {
  ack_digest_sha256_hex: string;
  fetch_digest_sha256_hex: string;
  object_class: "single-ack" | "multi-fetch";
};

function ackCap(request: Request): string | null {
  const value = request.headers.get("x-osl-ack-cap")?.trim().toLowerCase();
  return value && ACK_CAP_RE.test(value) ? value : null;
}

/**
 * A receipt proves only possession of the decryption-derived capability. It
 * records no account, device, or recipient identity. A syntactically valid
 * repeat receipt succeeds after its single-ack row has been removed, making
 * the operation safely idempotent without retaining a delivery-status row.
 */
export async function handleAck(request: Request, env: Env, blobId: string): Promise<Response> {
  const capability = ackCap(request);
  if (!capability || !ID_RE.test(blobId)) return notFound();

  const row = await env.DB.prepare(
    "SELECT ack_digest_sha256_hex, fetch_digest_sha256_hex, object_class FROM blob_capability_index WHERE blob_id = ? LIMIT 1",
  ).bind(blobId).first<AckRow>();

  if (!row) return new Response(null, { status: 204 });
  if (!constantTimeEqualHex(await sha256Hex(capability), row.ack_digest_sha256_hex)) return notFound();
  if (row.object_class === "multi-fetch") return new Response(null, { status: 204 });

  await new R2PayloadStore(env.PAYLOADS).deleteByDigest(row.fetch_digest_sha256_hex);
  await env.DB.prepare("DELETE FROM blob_capability_index WHERE blob_id = ?").bind(blobId).run();
  return new Response(null, { status: 204 });
}
