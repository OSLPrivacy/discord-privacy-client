import { describe, expect, it } from "vitest";
import { offlineCapabilityStatus, type OfflineUnavailableCapability } from "./offline-capability-status";

const OFFLINE_CAPABILITIES: readonly OfflineUnavailableCapability[] = [
  "receiveNewMessages",
  "sendMessage",
  "lookUpNewContactKey",
  "confirmBurnOnServer",
  "enforceExpiryOnServer",
  "enforceViewOnceOnServer",
];

describe("offline capability status", () => {
  it("makes every relay-dependent capability unavailable while offline", () => {
    for (const capability of OFFLINE_CAPABILITIES) {
      const status = offlineCapabilityStatus(capability, "offline");

      expect(status.available).toBe(false);
      expect(status.title).not.toBe("Available");
      expect(status.detail).toMatch(/until it reconnects/i);
    }
  });

  it("fails closed when connection state is unknown", () => {
    for (const capability of OFFLINE_CAPABILITIES) {
      expect(offlineCapabilityStatus(capability, "unknown").available).toBe(false);
    }
  });

  it("keeps local functionality distinct from unavailable relay work", () => {
    expect(offlineCapabilityStatus("receiveNewMessages", "offline").detail).toMatch(/already on this device remain readable/i);
    expect(offlineCapabilityStatus("sendMessage", "offline").detail).toMatch(/compose and encrypt/i);
    expect(offlineCapabilityStatus("confirmBurnOnServer", "offline").detail).toMatch(/local copy can be removed now/i);
    expect(offlineCapabilityStatus("enforceExpiryOnServer", "offline").detail).toMatch(/local expiry can still run/i);
    expect(offlineCapabilityStatus("enforceViewOnceOnServer", "offline").detail).toMatch(/already on this device/i);
  });

  it("allows relay-dependent capabilities only with a positive online state", () => {
    for (const capability of OFFLINE_CAPABILITIES) {
      expect(offlineCapabilityStatus(capability, "online")).toEqual({
        available: true,
        title: "Available",
        detail: "OSL is connected.",
      });
    }
  });
});
