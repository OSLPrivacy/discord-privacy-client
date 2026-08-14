import { describe, expect, it } from "vitest";
import { homeOverallStatus, homeProtectionState, type HomeOverallStatusInput } from "./home-protection-state";

describe("home protection state", () => {
  it("shows an unknown capability as not checked, never unavailable", () => {
    expect(homeProtectionState(false, false, { enabled: "Ready", unavailable: "Unavailable" })).toMatchObject({
      evidence: "unknown",
      label: "Not checked",
      honestTone: "neutral",
      statusTone: "unknown",
    });
  });

  it("distinguishes confirmed capability from a completed unsuccessful check", () => {
    expect(homeProtectionState(true, true, { enabled: "Ready", unavailable: "Unavailable" })).toMatchObject({
      evidence: "confirmed",
      label: "Ready",
      honestTone: "affirmative",
      statusTone: "ok",
    });
    expect(homeProtectionState(true, false, { enabled: "Ready", unavailable: "Unavailable" })).toMatchObject({
      evidence: "not-confirmed",
      label: "Unavailable",
      honestTone: "neutral",
      statusTone: "unknown",
    });
  });
});

describe("home overall status", () => {
  const appLabels = { enabled: "Ready", unavailable: "Unavailable" };
  const confirmedApps = homeProtectionState(true, true, appLabels);
  const uncheckedApps = homeProtectionState(false, false, appLabels);
  const unavailableApps = homeProtectionState(true, false, appLabels);

  const ready: HomeOverallStatusInput = {
    coreReady: true,
    coreDetail: "Protection cannot start on this device.",
    storageProtected: true,
    storageDetail: "Device protection confirmed.",
    connectedApps: confirmedApps,
    pendingFriendReviews: 0,
    verifiedFriends: 1,
  };

  it("claims Protected only when every dependent check has run and passed", () => {
    expect(homeOverallStatus(ready)).toEqual({
      state: "protected",
      headline: "Protected",
      detail: "Device protection confirmed.",
    });
  });

  it("NEVER reads Protected while any dependent check is unknown or failing", () => {
    // Every non-empty combination of degradations. If a future edit lets even
    // one of these read "Protected", this sweep is the alarm the audit said
    // was missing: the shipped Home claimed protection over unrun checks.
    const degradations: Partial<HomeOverallStatusInput>[] = [
      { coreReady: false },
      { storageProtected: false },
      { connectedApps: uncheckedApps },
      { connectedApps: unavailableApps },
      { pendingFriendReviews: 2 },
      { verifiedFriends: 0 },
    ];
    for (let mask = 1; mask < 1 << degradations.length; mask += 1) {
      const input = { ...ready };
      for (let bit = 0; bit < degradations.length; bit += 1) {
        if (mask & (1 << bit)) Object.assign(input, degradations[bit]);
      }
      const status = homeOverallStatus(input);
      const label = JSON.stringify({ mask, state: status.state, headline: status.headline });
      expect(status.state, label).not.toBe("protected");
      expect(status.headline, label).not.toMatch(/protected/iu);
      expect(status.detail, label).not.toMatch(/protection confirmed/iu);
    }
  });

  it("reads Not checked yet when the only gap is an unrun check", () => {
    expect(homeOverallStatus({ ...ready, connectedApps: uncheckedApps })).toEqual({
      state: "not-checked",
      headline: "Not checked yet",
      detail: "Connected apps have not been checked",
    });
  });

  it("names every unmet item instead of averaging them away", () => {
    const status = homeOverallStatus({ ...ready, connectedApps: uncheckedApps, pendingFriendReviews: 2 });
    expect(status.state).toBe("needs-attention");
    expect(status.headline).toBe("2 things need your attention");
    expect(status.detail).toContain("2 people need your review");
    expect(status.detail).toContain("Connected apps have not been checked");
  });

  it("names a single unmet item under a Needs attention headline", () => {
    expect(homeOverallStatus({ ...ready, verifiedFriends: 0 })).toEqual({
      state: "needs-attention",
      headline: "Needs attention",
      detail: "No one is verified yet",
    });
  });
});
