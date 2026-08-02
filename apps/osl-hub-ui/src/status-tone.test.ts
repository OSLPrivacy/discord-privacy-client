import { describe, expect, it } from "vitest";
import { statusTone } from "./status-tone";

describe("status tone", () => {
  it("uses the neutral unknown tone for an unrecognised reported capability set", () => {
    expect(statusTone(["L1", "L2", "future-layer"])).toBe("unknown");
  });

  it("uses success only for the complete recognised L3 capability", () => {
    expect(statusTone(["L1", "L2", "L3"])).toBe("ok");
    expect(statusTone(["L1", "L2", "L3", "future-layer"])).toBe("unknown");
  });
});
