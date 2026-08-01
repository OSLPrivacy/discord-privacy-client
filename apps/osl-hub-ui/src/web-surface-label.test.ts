import { describe, expect, it } from "vitest";
import { webSurfaceLabel } from "./web-surface-label";

const serviceIds = [
  "telegram", "discord", "whatsapp", "instagram", "snapchat", "email",
  "x", "signal", "slack", "linkedin", "teams", "messenger",
] as const;

describe("web surface label", () => {
  it("changes each service label as its reported capability gains L3", () => {
    const labelsByService = serviceIds.map((serviceId) => ({
      serviceId,
      l1: webSurfaceLabel(["L1"]),
      l3: webSurfaceLabel(["L1", "L2", "L3"]),
    }));

    expect(labelsByService).toEqual(serviceIds.map((serviceId) => ({
      serviceId,
      l1: "Default-browser companion · unprotected",
      l3: "Isolated OSL profile",
    })));
  });
});
