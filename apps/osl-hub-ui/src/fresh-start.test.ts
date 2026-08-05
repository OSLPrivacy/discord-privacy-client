import { describe, expect, it } from "vitest";
import { freshStartCleanupPresentation, freshStartLimitationsMarkup } from "./fresh-start";

const completeCleanup = {
  localCleanupComplete: true,
  removedTargets: ["hub_core"],
  failedTargets: [],
  remoteUnregister: { identitiesFound: 1, succeeded: 1, failed: 0, unavailable: 0 },
  restartRequired: true,
  originalDiscordDataUntouched: true as const,
};

describe("Fresh Start cleanup result", () => {
  it.each([
    { ...completeCleanup, failedTargets: ["service_profiles"] },
    { ...completeCleanup, remoteUnregister: { ...completeCleanup.remoteUnregister, unavailable: 1 } },
  ])("renders incomplete when cleanup is not fully confirmed", (result) => {
    const presentation = freshStartCleanupPresentation(result);

    expect(presentation.tone).toBe("warning");
    expect(presentation.complete).toBe(false);
    expect(presentation.message).toMatch(/partial|not acknowledged/iu);
  });
});

describe("Fresh Start limits", () => {
  it("renders every limit before the account-burn confirmation control", () => {
    const rendered = `${freshStartLimitationsMarkup()}<form id="burn-confirm-form"><button id="burn-confirm-submit">Burn now</button></form>`;
    const confirmAt = rendered.indexOf('id="burn-confirm-submit"');
    const limitations = [
      "Messages already opened by another person",
      "Server blobs whose deletion is still queued",
      "Cover text already posted on a platform",
      // D-254 CLOSED THE CAPABILITY, so the two limits D-249a added here are
      // no longer true and were replaced rather than deleted. Fresh Start now
      // runs the duress wipe's MANDATORY TpmEvict + KeyringPurge steps
      // (cleanup.rs run_key_material_wipe) and sweeps both application roots
      // for residue afterwards, so neither "the account key stays" nor "it
      // works from a fixed list" describes the shipping behaviour. What OSL
      // still cannot reach is written below, and the count of limits shown
      // before the confirmation control is unchanged.
      "Diagnostic logs and QA trace files OSL writes to the system temporary folder",
      "restart OSL and retry before treating the account as gone",
    ];

    expect(confirmAt).toBeGreaterThan(0);
    for (const limitation of limitations) {
      const limitationAt = rendered.indexOf(limitation);
      expect(limitationAt).toBeGreaterThanOrEqual(0);
      expect(limitationAt).toBeLessThan(confirmAt);
    }
  });
});
