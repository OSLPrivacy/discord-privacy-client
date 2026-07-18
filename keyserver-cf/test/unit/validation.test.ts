import { describe, expect, it } from "vitest";
import {
  isProtocolId,
  isU32,
  MAX_PROTOCOL_ID_BYTES,
} from "../../src/lib/validation.js";

describe("canonical input validation", () => {
  it("accepts only exact unsigned 32-bit integers", () => {
    expect(isU32(0)).toBe(true);
    expect(isU32(0xffff_ffff)).toBe(true);
    for (const value of [-1, 1.5, 0x1_0000_0000, Number.MAX_SAFE_INTEGER]) {
      expect(isU32(value)).toBe(false);
    }
  });

  it("bounds service-neutral identifiers without imposing a numeric format", () => {
    expect(isProtocolId("opaque/service:用户-123")).toBe(true);
    expect(isProtocolId("a".repeat(MAX_PROTOCOL_ID_BYTES))).toBe(true);
    expect(isProtocolId("a".repeat(MAX_PROTOCOL_ID_BYTES + 1))).toBe(false);
    expect(isProtocolId("scope\nforged")).toBe(false);
    expect(isProtocolId("scope\u007fforged")).toBe(false);
  });
});
