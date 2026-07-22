import { describe, expect, it, vi } from "vitest";
import { createWhatsAppQaShell, parseWhatsAppQaReceipt, type WhatsAppQaDependencies, type WhatsAppQaReceipt, type WhatsAppQaStatus } from "./whatsapp-qa-shell";

const receipt = (status: WhatsAppQaStatus): WhatsAppQaReceipt => ({ provider: "whatsapp", status, reason: "none", mode: "existingNativeCompanion", captureProtected: false });
const deps = (): WhatsAppQaDependencies => ({ claim: vi.fn().mockResolvedValue(receipt("hosted")), resize: vi.fn().mockResolvedValue(receipt("resized")), focus: vi.fn().mockResolvedValue(receipt("focused")), detach: vi.fn().mockResolvedValue(receipt("detached")) });

describe("WhatsApp Desktop-only QA shell", () => {
  it("has no provider, install, browser, credential, or dedicated-profile authority", async () => {
    const shell=createWhatsAppQaShell(deps());
    await expect(shell.open()).resolves.toMatchObject({ phase:"open", browserFallbackAllowed:false, installAllowed:false, credentialsAccepted:false, sessionMode:"existingSession" });
  });

  it("requires exact WhatsApp existing-companion receipts", () => {
    expect(() => parseWhatsAppQaReceipt({ ...receipt("hosted"), provider:"telegram" })).toThrow();
    expect(() => parseWhatsAppQaReceipt({ ...receipt("hosted"), captureProtected:true })).toThrow();
    expect(() => parseWhatsAppQaReceipt({ ...receipt("hosted"), mode:"ownedBorderless" })).toThrow();
    expect(() => parseWhatsAppQaReceipt({ ...receipt("hosted"), url:"https://web.whatsapp.com" })).toThrow();
  });

  it("preserves fail-closed ambiguity and identity status", async () => {
    const dependencies=deps();
    dependencies.claim=vi.fn().mockResolvedValue({ provider:"whatsapp", status:"failed", reason:"existingSessionAmbiguous", mode:"none", captureProtected:false });
    await expect(createWhatsAppQaShell(dependencies).open()).resolves.toMatchObject({ phase:"failed", reason:"existingSessionAmbiguous" });
  });

  it("resizes, focuses and detaches only after an exact claim", async () => {
    const shell=createWhatsAppQaShell(deps());
    await expect(shell.resize()).resolves.toMatchObject({ phase:"failed", reason:"notHosted" });
    const fresh=createWhatsAppQaShell(deps()); await fresh.open();
    await expect(fresh.resize()).resolves.toMatchObject({ phase:"open" });
    await expect(fresh.focus()).resolves.toMatchObject({ phase:"open" });
    await expect(fresh.close()).resolves.toMatchObject({ phase:"idle" });
  });
});
