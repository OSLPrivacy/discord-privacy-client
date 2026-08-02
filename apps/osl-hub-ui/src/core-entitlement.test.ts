import { describe, expect, it } from "vitest";
import { parseHubLicenseState } from "./core";

const timestamps = {
  currentPeriodEnd: 1_735_689_600,
  lastValidatedAt: 1_733_097_600,
};

describe("hub entitlement state boundary", () => {
  it("round-trips every supported status with its fail-closed access tier", () => {
    const states = [
      ["UNCONFIGURED", "free"],
      ["ACTIVE", "pro"],
      ["CANCELLED", "pro"],
      ["GRACE", "pro"],
      ["EXPIRED", "free"],
      ["REVOKED", "free"],
      ["UNKNOWN", "free"],
      ["PENDING", "free"],
      ["UNREDEEMED", "free"],
    ] as const;

    for (const [status, access] of states) {
      expect(parseHubLicenseState({ access, status, ...timestamps })).toEqual({
        access,
        status,
        ...timestamps,
      });
    }
  });

  it("rejects unknown and future shelf-expired statuses instead of granting Pro", () => {
    for (const status of ["FUTURE_STATUS", "SHELF_EXPIRED"]) {
      expect(() => parseHubLicenseState({ access: "pro", status, ...timestamps }))
        .toThrow("invalid activation state response");
    }
  });

  it("rejects an unredeemed code presented as Pro", () => {
    expect(() => parseHubLicenseState({ access: "pro", status: "UNREDEEMED", ...timestamps }))
      .toThrow("invalid activation state response");
  });
});
