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
    ];

    expect(confirmAt).toBeGreaterThan(0);
    for (const limitation of limitations) {
      const limitationAt = rendered.indexOf(limitation);
      expect(limitationAt).toBeGreaterThanOrEqual(0);
      expect(limitationAt).toBeLessThan(confirmAt);
    }
  });
});
