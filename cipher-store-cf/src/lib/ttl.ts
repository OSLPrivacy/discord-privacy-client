/** The only delivery windows the relay exposes, in seconds. */
export const TTL_ALLOWLIST = [3_600, 86_400, 259_200, 604_800] as const;
export const DEFAULT_DELIVERY_TTL_FLOOR = 604_800;

export type ExpiryMode = "default" | "absolute";

/**
 * Parse the public delivery-window headers without silently coercing values.
 * Relative-clock messages are the default and must retain an undelivered copy
 * for seven days.  The explicit absolute mode is the documented opt-out.
 */
export function parseUploadTtl(
  rawTtl: string | null,
  rawMode: string | null,
): { ttl: number; mode: ExpiryMode } | null {
  if (!rawTtl || !/^\d+$/.test(rawTtl)) return null;
  const ttl = Number(rawTtl);
  if (!Number.isSafeInteger(ttl) || !TTL_ALLOWLIST.includes(ttl as typeof TTL_ALLOWLIST[number])) return null;

  const mode: ExpiryMode = rawMode === "absolute" ? "absolute" : rawMode === null ? "default" : "default";
  if (rawMode !== null && rawMode !== "absolute") return null;
  if (mode === "default" && ttl < DEFAULT_DELIVERY_TTL_FLOOR) return null;
  return { ttl, mode };
}
