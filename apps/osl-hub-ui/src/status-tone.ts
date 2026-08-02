import type { WebSurfaceCapability } from "./web-surface-label";

/** The CSS modifier emitted for a status claim. */
export type StatusTone = "ok" | "unknown";

/**
 * Resolves a status tone from the capability report shared with the web-surface
 * label contract. A success tone is an L3 claim, and is valid only when every
 * reported capability is one the client understands. Labels are intentionally
 * absent from this API: changing copy must not change a status claim.
 */
export function statusTone(
  capabilities: readonly (WebSurfaceCapability | string)[],
): StatusTone {
  if (capabilities.some((capability) => (
    capability !== "L1" && capability !== "L2" && capability !== "L3"
  ))) {
    return "unknown";
  }

  return capabilities.includes("L3") ? "ok" : "unknown";
}
