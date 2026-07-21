import { describe, expect, it } from "vitest";
import {
  desktopServicePolicies,
  desktopServicePolicy,
  requiresNativeDesktopSurface,
} from "./desktop-service-policy";

describe("Windows desktop service policy", () => {
  it("does not route known desktop apps through the ordinary browser", () => {
    for (const id of [
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
});
