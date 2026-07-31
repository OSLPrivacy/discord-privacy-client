import { describe, expect, it } from "vitest";
import { parseWhatsAppPreparedCarrier } from "./whatsapp-overlay-prepare";

const valid = () => ({
  provider: "whatsapp",
  status: "readyForExplicitPlacement",
  coverText: "ordinary-looking protected carrier",
  expiresAt: 1_800_000_000,
  personToPersonE2ee: true,
  contextBindingSha256: "a".repeat(64),
  automaticPlacement: false,
  realMessageSent: false,
});

describe("WhatsApp protected-carrier receipt", () => {
  it("accepts only explicit-placement peer E2EE", () => {
    expect(parseWhatsAppPreparedCarrier(valid()).automaticPlacement).toBe(false);
  });

  it("rejects false send claims and widened fields", () => {
    expect(() => parseWhatsAppPreparedCarrier({ ...valid(), realMessageSent: true })).toThrow();
    expect(() => parseWhatsAppPreparedCarrier({ ...valid(), providerStorageRead: false })).toThrow();
  });

  it("rejects invalid context commitments and unbounded carriers", () => {
    expect(() => parseWhatsAppPreparedCarrier({ ...valid(), contextBindingSha256: "A".repeat(64) })).toThrow();
    expect(() => parseWhatsAppPreparedCarrier({ ...valid(), coverText: "x".repeat(16 * 1024 + 1) })).toThrow();
  });
});
