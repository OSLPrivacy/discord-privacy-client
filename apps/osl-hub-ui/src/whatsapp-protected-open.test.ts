import { describe, expect, it } from "vitest";
import { parseWhatsAppProtectedOpenReceipt } from "./whatsapp-protected-open";

const valid = {
  provider: "whatsapp",
  status: "opened",
  plaintext: "protected hello",
  personToPersonE2ee: true,
  contextVerified: true,
  contextBindingSha256: "a".repeat(64),
  providerHistoryChanged: false,
  providerStorageRead: false,
};

describe("WhatsApp protected-open receipt", () => {
  it("accepts only the strict truthful receipt", () => {
    expect(parseWhatsAppProtectedOpenReceipt(valid).plaintext).toBe("protected hello");
  });

  it.each([
    { ...valid, contextVerified: false },
    { ...valid, personToPersonE2ee: false },
    { ...valid, providerHistoryChanged: true },
    { ...valid, providerStorageRead: true },
    { ...valid, contextBindingSha256: "bad" },
    { ...valid, extra: "field" },
  ])("rejects unsafe or expanded receipts", (receipt) => {
    expect(() => parseWhatsAppProtectedOpenReceipt(receipt)).toThrow();
  });

  it("rejects empty and over-limit plaintext", () => {
    expect(() => parseWhatsAppProtectedOpenReceipt({ ...valid, plaintext: "" })).toThrow();
    expect(() => parseWhatsAppProtectedOpenReceipt({ ...valid, plaintext: "é".repeat(501) })).toThrow();
  });
});
