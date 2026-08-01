/** Capabilities reported for the active web surface. */
export type WebSurfaceCapability = "L1" | "L2" | "L3";

/**
 * The surface label is derived solely from its reported protection capability.
 * L3 is the first layer that can honestly describe the surface as protected.
 */
export function webSurfaceLabel(capabilities: readonly WebSurfaceCapability[]): string {
  return capabilities.includes("L3")
    ? "Isolated OSL profile"
    : "Default-browser companion · unprotected";
}
