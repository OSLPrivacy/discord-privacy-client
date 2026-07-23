import { describe, expect, it } from "vitest";
import {
  isCompleteWhatsAppVisualBinding,
  parseWhatsAppVisualBindingBeginReceipt,
  parseWhatsAppVisualBindingConfirmReceipt,
} from "./whatsapp-visual-binding";

const captureId = "capture_0123456789abcdef";
const begin = {
  provider: "whatsapp",
  status: "awaitingConfirmation",
  captureId,
  regions: ["accountHeader", "chatHeader", "composer", "transcript"],
  fixedRegionsOnly: true,
  contentPersisted: false,
  privateStorageRead: false,
  foregroundChanged: false,
};
const confirmed = {
  provider: "whatsapp",
  status: "verified",
  captureId,
  selectorRevision: "whatsapp-visual-confirmed-v1",
  contextBindingSha256: "a".repeat(64),
  recipientSetSha256: "b".repeat(64),
  windowGeneration: 7,
  windowRect: [10, 20, 1010, 820],
  composerRect: [300, 720, 980, 790],
  transcriptRect: [300, 100, 980, 700],
  accountVerified: true,
  chatVerified: true,
  recipientSetVerified: true,
  composerVerified: true,
  transcriptVerified: true,
  protectedControlsAvailable: true,
  contentPersisted: false,
  privateStorageRead: false,
  foregroundChanged: false,
};

describe("WhatsApp visual-binding receipts", () => {
  it("accepts only a bounded fixed-region start receipt", () => {
    expect(parseWhatsAppVisualBindingBeginReceipt(begin)).toEqual(begin);
    for (const widened of [
      { ...begin, contentPersisted: true },
      { ...begin, privateStorageRead: true },
      { ...begin, foregroundChanged: true },
      { ...begin, regions: [...begin.regions, "messageContent"] },
      { ...begin, screenshot: "data:image/png;base64,secret" },
    ]) expect(() => parseWhatsAppVisualBindingBeginReceipt(widened)).toThrow();
  });

  it("requires every identity and geometry gate before verification", () => {
    expect(parseWhatsAppVisualBindingConfirmReceipt(confirmed)).toEqual(confirmed);
    expect(isCompleteWhatsAppVisualBinding(
      parseWhatsAppVisualBindingConfirmReceipt(confirmed),
      captureId,
    )).toBe(true);
    for (const key of [
      "accountVerified",
      "chatVerified",
      "recipientSetVerified",
      "composerVerified",
      "transcriptVerified",
      "protectedControlsAvailable",
    ] as const) {
      expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, [key]: false })).toThrow();
    }
  });

  it("rejects mismatched captures, widened receipts, and incomplete receipts", () => {
    expect(isCompleteWhatsAppVisualBinding(
      parseWhatsAppVisualBindingConfirmReceipt(confirmed),
      "capture_fedcba9876543210",
    )).toBe(false);
    expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, contentPersisted: true })).toThrow();
    expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, visualContent: "peer name" })).toThrow();
    expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, status: "rejected" })).toThrow();
    expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, composerRect: [0, 0, 0, 0] })).toThrow();
    expect(() => parseWhatsAppVisualBindingConfirmReceipt({ ...confirmed, selectorRevision: "visual-v0" })).toThrow();
  });
});
