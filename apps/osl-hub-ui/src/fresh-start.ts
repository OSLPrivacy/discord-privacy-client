import type { HubFullCleanupResult } from "./adapters";

export interface FreshStartCleanupPresentation {
  tone: "success" | "warning";
  message: string;
  complete: boolean;
}

/**
 * Fresh Start only reaches local OSL state. These limits are part of the
 * irreversible-action contract and must be shown before its confirmation.
 */
export function freshStartLimitationsMarkup(): string {
  return `<section class="burn-truth fresh-start-limitations" aria-labelledby="fresh-start-limits-title"><strong id="fresh-start-limits-title">What Fresh Start cannot remove</strong><ul><li>Messages already opened by another person remain on their device.</li><li>Server blobs whose deletion is still queued can remain available until OSL reconnects and the server confirms deletion.</li><li>Cover text already posted on a platform remains on that platform.</li></ul></section>`;
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
