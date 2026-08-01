import type { HubFullCleanupResult } from "./adapters";

export interface FreshStartCleanupPresentation {
  tone: "success" | "warning";
  message: string;
  complete: boolean;
}

/**
 * A cleanup result is successful only when every local target and remote
 * unregister has been confirmed. Native completion alone is not proof.
 */
export function freshStartCleanupPresentation(result: HubFullCleanupResult): FreshStartCleanupPresentation {
  if (!result.localCleanupComplete || result.failedTargets.length > 0) {
    return {
      tone: "warning",
      complete: false,
      message: `Cleanup was partial. Removed: ${result.removedTargets.join(", ") || "none"}. Still present: ${result.failedTargets.join(", ") || "unknown"}. Restart OSL and retry.`,
    };
  }

  const unconfirmedRemote = result.remoteUnregister.failed + result.remoteUnregister.unavailable;
  if (unconfirmedRemote > 0) {
    return {
      tone: "warning",
      complete: false,
      message: `All local OSL data was removed. Remote unregister was not acknowledged for ${unconfirmedRemote} identity ${unconfirmedRemote === 1 ? "record" : "records"}; no remote deletion success is being claimed.`,
    };
  }

  return {
    tone: "success",
    complete: true,
    message: "All local OSL identities, decrypt material, caches, and preferences were removed from this computer.",
  };
}
