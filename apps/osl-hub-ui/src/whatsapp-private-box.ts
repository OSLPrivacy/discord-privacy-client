import { utf8Length } from "./overlay-state";

/** The compact WhatsApp companion is deliberately bounded independently of its carrier. */
export const WHATSAPP_PRIVATE_BOX_MAX_BYTES = 1_000;

export interface WhatsAppPrivateBoxState {
  /** Text that exists only in the OSL-owned overlay field. */
  privateDraft: string;
  byteCountText: string;
  /**
   * The companion never writes into WhatsApp's confirmed composer. Its value is
   * therefore always empty while a private draft is composed or cleared.
   */
  whatsappComposerCharacters: 0;
}

export function boundedWhatsAppPrivateDraft(value: string): string {
  let result = "";
  for (const character of value) {
    if (utf8Length(result) + utf8Length(character) > WHATSAPP_PRIVATE_BOX_MAX_BYTES) break;
    result += character;
  }
  return result;
}

export function reconcileWhatsAppPrivateBox(value: string): WhatsAppPrivateBoxState {
  const privateDraft = boundedWhatsAppPrivateDraft(value);
  return {
    privateDraft,
    byteCountText: `${utf8Length(privateDraft)} / ${WHATSAPP_PRIVATE_BOX_MAX_BYTES} bytes`,
    whatsappComposerCharacters: 0,
  };
}
