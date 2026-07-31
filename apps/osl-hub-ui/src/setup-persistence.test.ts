import { describe, expect, it } from "vitest";
import {
  activeSetupRoutes,
  parseSetupPrivacyChoices,
  parseSetupResumeCheckpoint,
  scrubSetupSteps,
  setupPrivacyChoiceIds,
} from "./setup-persistence";

describe("setup resume checkpoints", () => {
  it.each(activeSetupRoutes)("restores the active %s route", (route) => {
    expect(parseSetupResumeCheckpoint(JSON.stringify({ route, scrubStep: "intro" }))).toEqual({ route, scrubStep: "intro" });
  });

  it.each(scrubSetupSteps)("restores the Scrub %s substep", (scrubStep) => {
    expect(parseSetupResumeCheckpoint(JSON.stringify({ route: "scrub", scrubStep }))).toEqual({ route: "scrub", scrubStep });
  });

  it("fails closed for obsolete, unknown, and malformed routes", () => {
    for (const route of ["tutorial", "apps", "install", "unknown"]) {
      expect(parseSetupResumeCheckpoint(JSON.stringify({ route, scrubStep: "intro" }))).toBeNull();
    }
    expect(parseSetupResumeCheckpoint("not-json")).toBeNull();
    expect(parseSetupResumeCheckpoint(JSON.stringify({ route: "scrub", scrubStep: "unknown" }))).toEqual({ route: "scrub", scrubStep: "intro" });
  });
});

describe("setup privacy persistence", () => {
  it("accepts only a bounded, allowlisted set and deduplicates it", () => {
    expect([...parseSetupPrivacyChoices(JSON.stringify(["auto-lock", "auto-lock", "clear-clipboard"]))]).toEqual(["auto-lock", "clear-clipboard"]);
  });

  it("uses secure defaults for malformed, unknown, or oversized data", () => {
    const defaults = ["hide-notifications"];
    expect([...parseSetupPrivacyChoices("bad")]).toEqual(defaults);
    expect([...parseSetupPrivacyChoices(JSON.stringify(["unknown"]))]).toEqual(defaults);
    expect([...parseSetupPrivacyChoices(JSON.stringify([...setupPrivacyChoiceIds, "auto-lock"]))]).toEqual(defaults);
  });
});
