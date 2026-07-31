import { describe, expect, it } from "vitest";
import { createWhatsAppProtectedControls, parseWhatsAppProtectedActionReceipt, parseWhatsAppVerificationReceipt } from "./whatsapp-protected-controls";

const hash = (value: string) => value.repeat(64);
const verified = { status:"verified", provider:"whatsapp", selectorRevision:"whatsapp-uia-test-v1", accountVerified:true, chatVerified:true, recipientSetVerified:true, composerVerified:true, transcriptVerified:true, contextBindingSha256:hash("a"), recipientSetSha256:hash("b"), windowGeneration:7, composerRect:[1,700,900,780], transcriptRect:[1,100,900,690], protectedControlsAvailable:true };
const action = { action:"multilineUtf8", status:"readyForExplicitPlacement", provider:"whatsapp", contextBindingSha256:hash("a"), messageCommitmentSha256:hash("c"), composerRect:[1,700,900,780], transcriptRect:[1,100,900,690], realMessageSent:false, providerHistoryChanged:false, providerStorageRead:false };

describe("WhatsApp protected-control receipts", () => {
  it("requires all five exact verification dimensions", () => {
    expect(parseWhatsAppVerificationReceipt(verified).protectedControlsAvailable).toBe(true);
    expect(() => parseWhatsAppVerificationReceipt({ ...verified, recipientSetVerified:false })).toThrow();
    expect(() => parseWhatsAppVerificationReceipt({ ...verified, provider:"telegram" })).toThrow();
  });

  it("accepts only non-mutating, non-sending, non-storage-reading semantic receipts", () => {
    expect(parseWhatsAppProtectedActionReceipt(action).status).toBe("readyForExplicitPlacement");
    for (const widened of [{ ...action, realMessageSent:true }, { ...action, providerStorageRead:true }, { ...action, url:"https://web.whatsapp.com" }]) expect(() => parseWhatsAppProtectedActionReceipt(widened)).toThrow();
  });

  it("clears controls for unverified selectors and changed context", () => {
    const controls=createWhatsAppProtectedControls();
    const unavailable={ status:"selectorContractUnverified", provider:"whatsapp", selectorRevision:null, accountVerified:false, chatVerified:false, recipientSetVerified:false, composerVerified:false, transcriptVerified:false, contextBindingSha256:null, recipientSetSha256:null, windowGeneration:null, composerRect:null, transcriptRect:null, protectedControlsAvailable:false };
    expect(controls.bind(unavailable)).toBe(false);
    expect(controls.accept(action)).toBeNull();
    expect(controls.bind(verified)).toBe(true);
    expect(controls.accept(action)).not.toBeNull();
    expect(controls.accept({ ...action, contextBindingSha256:hash("d") })).toBeNull();
  });
});
