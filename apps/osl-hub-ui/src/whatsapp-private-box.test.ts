import { describe, expect, it } from "vitest";
import {
  boundedWhatsAppPrivateDraft,
  reconcileWhatsAppPrivateBox,
} from "./whatsapp-private-box";

describe("WhatsApp locked private box", () => {
  it("counts the fixture's UTF-8 bytes while WhatsApp's confirmed box remains empty", () => {
    const fixture = "a".repeat(34) + "€";
    const state = reconcileWhatsAppPrivateBox(fixture);

    console.info(`TASK1069_FIXTURE_BYTES=${state.byteCountText}`);
    console.info(`TASK1069_WHATSAPP_COMPOSER_CHARACTERS=${state.whatsappComposerCharacters}`);
    expect(new TextEncoder().encode(fixture)).toHaveLength(37);
    expect(state.byteCountText).toBe("37 / 1000 bytes");
    expect(state.whatsappComposerCharacters).toBe(0);
  });

  it("returns the live count to zero after the OSL-owned box is cleared", () => {
    const cleared = reconcileWhatsAppPrivateBox("");

    console.info(`TASK1069_CLEARED_BYTES=${cleared.byteCountText}`);
    console.info(`TASK1069_CLEARED_WHATSAPP_COMPOSER_CHARACTERS=${cleared.whatsappComposerCharacters}`);
    expect(cleared.privateDraft).toBe("");
    expect(cleared.byteCountText).toBe("0 / 1000 bytes");
    expect(cleared.whatsappComposerCharacters).toBe(0);
  });

  it("does not split a UTF-8 character at the private-box limit", () => {
    expect(boundedWhatsAppPrivateDraft("a".repeat(999) + "€")).toBe("a".repeat(999));
  });
});
