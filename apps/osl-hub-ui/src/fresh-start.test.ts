import { describe, expect, it } from "vitest";
import { freshStartCleanupPresentation } from "./fresh-start";

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
