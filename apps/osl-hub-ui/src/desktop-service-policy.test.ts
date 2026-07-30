import { describe, expect, it } from "vitest";
import {
  desktopServicePolicies,
  desktopServicePolicy,
  oslMailStage,
  oslMailStages,
  requiresNativeDesktopSurface,
} from "./desktop-service-policy";

describe("Windows desktop service policy", () => {
  it("does not route known desktop apps through the ordinary browser", () => {
    for (const id of [
      "discord",
      "outlook",
      "proton",
      "tuta",
      "fastmail",
      "zoho",
      "slack",
      "teams",
    ] as const) {
      expect(requiresNativeDesktopSurface(id)).toBe(true);
    }
  });

  it("keeps services without a current official Windows client on browser policy", () => {
    for (const id of ["instagram", "messenger", "x", "snapchat", "gmail", "yahoo", "aol", "gmx", "maildotcom", "icloud"] as const) {
      expect(requiresNativeDesktopSurface(id)).toBe(false);
    }
    expect(desktopServicePolicy("instagram").surface).toBe("packagedWeb");
    expect(desktopServicePolicy("messenger").surface).toBe("browserOnly");
  });

  it("does not claim unsupported separate native profiles", () => {
    expect(desktopServicePolicies.every((entry) => entry.separateProfileAvailable === false)).toBe(true);
  });

  it("leaves unverified desktop identities unavailable for native launch", () => {
    expect(desktopServicePolicy("proton").surface).toBe("candidate");
    expect(desktopServicePolicy("instagram").surface).toBe("packagedWeb");
    expect(desktopServicePolicy("outlook").surface).toBe("verified");
  });

  it("separates Discord send-mode terms status from enforcement likelihood", () => {
    const risk = desktopServicePolicy("discord").sendModeRisk ?? [];
    expect(risk.map((entry) => entry.mode)).toEqual(["clipboard", "double", "single"]);
    expect(risk.map((entry) => entry.mode)).not.toContain("manual");

    const clipboard = risk.find((entry) => entry.mode === "clipboard");
    expect(clipboard).toMatchObject({
      termsStatus: "No known restriction",
      enforcementLikelihood: "Unknown",
      enforcementEvidence: null,
    });

    for (const mode of ["double", "single"] as const) {
      const entry = risk.find((item) => item.mode === mode);
      expect(entry).toMatchObject({
        termsStatus: "May conflict with terms",
        enforcementLikelihood: "Unknown",
        enforcementEvidence: null,
      });
      expect(entry?.explanation).toContain("may put the account at risk");
    }
  });

  it("does not derive ban probabilities or terms-safety claims for Discord send modes", () => {
    const riskText = JSON.stringify(desktopServicePolicy("discord").sendModeRisk);
    expect(riskText).not.toMatch(/\d+\s*%|percentage|probability|ban rate/i);
    expect(riskText).not.toMatch(/undetectable|terms-safe|compliant because|allowed because/i);
  });

  it("models OSL Mail stages without promoting later mailbox promises", () => {
    expect(oslMailStages.map((stage) => stage.id)).toEqual(["stageA", "stageB", "stageC"]);
    expect(oslMailStage("stageA")).toMatchObject({
      availability: "available",
      externalEmailScope: "ordinaryExternalEmailUnlessSeparatelySupported",
    });
    expect(oslMailStage("stageA").boundary).toContain("explicitly authorizes");
    expect(oslMailStage("stageA").excludes).toContain("OSL-operated mailbox");
    expect(oslMailStage("stageB").availability).toBe("comingLater");
    expect(oslMailStage("stageC").availability).toBe("comingLater");
    expect(JSON.stringify(oslMailStages)).not.toMatch(/ordinary external email is OSL end-to-end encrypted|universal encrypted delivery available/i);
  });
});
