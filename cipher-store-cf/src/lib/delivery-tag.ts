import type { Env } from "../env.js";

const TAG_RE = /^[0-9a-f]{32}$/;
const MAX_SUBSCRIPTION_TAGS = 200;

/** The sole durable result disclosed to the anonymous realtime connection. */
export type DeliveryMatch = Readonly<{ tag: string; blob_id: string }>;

/**
 * Find live payload pointers for an opaque rotating-tag subscription window.
 * The query deliberately selects neither upload metadata nor an aggregate, so
 * this module cannot turn the index into an account, size, or count oracle.
 */
export async function findDeliveryMatches(env: Env, tags: readonly string[]): Promise<DeliveryMatch[]> {
  const unique = [...new Set(tags)];
  if (unique.length === 0 || unique.length > MAX_SUBSCRIPTION_TAGS || unique.some((tag) => !TAG_RE.test(tag))) {
    return [];
  }
  const placeholders = unique.map(() => "?").join(", ");
  const result = await env.DB.prepare(
    `SELECT delivery_tag AS tag, blob_id
       FROM blob_capability_index
      WHERE delivery_tag IN (${placeholders}) AND expires_at >= ?`,
  ).bind(...unique, Math.floor(Date.now() / 1000)).all<DeliveryMatch>();
  return result.results ?? [];
}
