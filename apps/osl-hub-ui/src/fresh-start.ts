import type { HubFullCleanupResult } from "./adapters";
import { DELETION_REFERENCE_DISCLOSURE } from "./feature-claims";

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
  return `<section class="burn-truth fresh-start-limitations" aria-labelledby="fresh-start-limits-title"><strong id="fresh-start-limits-title">What Fresh Start cannot remove</strong><ul><li>Messages already opened by another person remain on their device.</li><li>Server blobs whose deletion is still queued can remain available until OSL reconnects and the server confirms deletion.</li><li>Cover text already posted on a platform remains on that platform.</li><li>Diagnostic logs and QA trace files OSL writes to the system temporary folder sit outside OSL's own storage directories and are not removed.</li><li>A run that reports anything left behind removed nothing further; restart OSL and retry before treating the account as gone.</li></ul></section>`;
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
      message: `The local OSL data in OSL's own storage was removed. Remote unregister was not acknowledged for ${unconfirmedRemote} identity ${unconfirmedRemote === 1 ? "record" : "records"}; no remote deletion success is being claimed. ${DELETION_REFERENCE_DISCLOSURE}`,
    };
  }

  return {
    tone: "success",
    complete: true,
    message: "Every OSL identity, decrypt key, cache, and preference in OSL's own storage was removed, and OSL then checked that storage is empty.",
  };
}
