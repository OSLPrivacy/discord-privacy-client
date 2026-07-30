import { describe, expect, it } from "vitest";
import {
  isDiscordSnowflake,
  isProtocolId,
  isReservedDerivedId,
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

  it("recognizes only Discord-shaped 17-to-20 digit identifiers", () => {
    expect(isDiscordSnowflake("90000000000000001")).toBe(true);
    expect(isDiscordSnowflake("90000000000000000000")).toBe(true);
    expect(isDiscordSnowflake("1234567890123456")).toBe(false);
    expect(isDiscordSnowflake("osl_90000000000000001")).toBe(false);
  });

  it("reserves preparatory and canonical derived identifiers only in lowercase base32", () => {
    expect(isReservedDerivedId(`osl1_${"a".repeat(32)}`)).toBe(true);
    expect(isReservedDerivedId(`osl1_${"a".repeat(52)}`)).toBe(true);
    expect(isReservedDerivedId(`osl1_${"A".repeat(52)}`)).toBe(false);
    expect(isReservedDerivedId(`osl1_${"a".repeat(43)}`)).toBe(false);
  });
});
