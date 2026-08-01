import { describe, expect, it } from "vitest";
import { webSurfaceLabel } from "./web-surface-label";

describe("web surface label", () => {
  it("derives the label from the reported capability set", () => {
    expect(webSurfaceLabel(["L1"])).toBe("Default-browser companion · unprotected");
    expect(webSurfaceLabel(["L1", "L2", "L3"])).toBe("Isolated OSL profile");
  });
});
