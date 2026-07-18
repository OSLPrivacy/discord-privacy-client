import { describe, expect, it } from "vitest";
import { parseMassCleanupCapabilities } from "./mass-cleanup";

const serviceIds = [
  "telegram", "discord", "whatsapp", "instagram", "snapchat", "email", "x", "signal", "slack", "linkedin", "teams", "messenger",
] as const;

function manifest() {
  return {
    proRequired: true,
    reviewRequiredEveryBatch: true,
    typedConfirmationRequiredEveryBatch: true,
    unattendedExecutionAllowed: false,
    services: serviceIds.map((serviceId) => ({
      serviceId,
      availability: "unavailable",
      discoverySupported: false,
      mutationSupported: false,
      plannedActions: serviceId === "telegram" ? ["leaveAndRemoveChat", "clearHistoryForSelf"] : [],
      status: "No reviewed local adapter is installed in this build.",
    })),
  };
}

describe("Mass Cleanup capability boundary", () => {
  it("accepts only the complete fail-closed native manifest", () => {
    const parsed = parseMassCleanupCapabilities(manifest());
    expect(parsed.services).toHaveLength(12);
    expect(parsed.unattendedExecutionAllowed).toBe(false);
    expect(parsed.services.every((service) => !service.mutationSupported)).toBe(true);
  });

  it("rejects optimistic, duplicate, and extended manifests", () => {
    const optimistic = manifest();
    optimistic.services[0].mutationSupported = true;
    optimistic.services[0].discoverySupported = true;
    optimistic.services[0].availability = "available";
    // Available remains a forward-compatible native state, so mutation must
    // still be represented exactly rather than inferred by the parser.
    expect(parseMassCleanupCapabilities(optimistic).services[0].mutationSupported).toBe(true);

    const duplicate = manifest();
    duplicate.services[1].serviceId = "telegram";
    expect(() => parseMassCleanupCapabilities(duplicate)).toThrow();

    expect(() => parseMassCleanupCapabilities({ ...manifest(), url: "https://example.test" })).toThrow();
  });
});
