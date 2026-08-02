import { describe, expect, it } from "vitest";
import { homeProtectionState } from "./home-protection-state";

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
