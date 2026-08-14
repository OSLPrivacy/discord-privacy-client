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

export type WhatsAppPrivateBoxCommand =
  | { type: "enterPrivateText"; text: string }
  | { type: "clearPrivateText" };

/**
 * Read the private box independently after a command has run. The count check
 * must not infer success from the command's input text.
 */
export interface WhatsAppPrivateBoxReader {
  readPrivateByteCount(privateBox: WhatsAppPrivateBoxState): number;
}

export interface WhatsAppPrivateCountCheck {
  fixtureBytes: number;
  countAfterEnter: number;
  countAfterClear: number;
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

/** Apply an explicit private-box command without writing into WhatsApp. */
export function executeWhatsAppPrivateBoxCommand(
  privateBox: WhatsAppPrivateBoxState,
  command: WhatsAppPrivateBoxCommand,
): WhatsAppPrivateBoxState {
  switch (command.type) {
    case "enterPrivateText":
      return reconcileWhatsAppPrivateBox(command.text);
    case "clearPrivateText":
      return {
        ...privateBox,
        privateDraft: "",
        byteCountText: `0 / ${WHATSAPP_PRIVATE_BOX_MAX_BYTES} bytes`,
      };
  }
}

/**
 * Execute a multi-byte entry and a direct clear, independently reading the box
 * after both commands. A stale or no-op reader cannot satisfy this check.
 */
export function checkWhatsAppPrivateCount(
  reader: WhatsAppPrivateBoxReader,
): WhatsAppPrivateCountCheck {
  const fixture = `${"a".repeat(34)}€`;
  const fixtureBytes = utf8Length(fixture);
  let privateBox = reconcileWhatsAppPrivateBox("");

  privateBox = executeWhatsAppPrivateBoxCommand(privateBox, {
    type: "enterPrivateText",
    text: fixture,
  });
  const countAfterEnter = reader.readPrivateByteCount(privateBox);
  if (countAfterEnter !== fixtureBytes) {
    throw new Error(
      `WhatsApp private-box reader returned ${countAfterEnter} bytes after enter; expected ${fixtureBytes}`,
    );
  }

  privateBox = executeWhatsAppPrivateBoxCommand(privateBox, { type: "clearPrivateText" });
  const countAfterClear = reader.readPrivateByteCount(privateBox);
  if (countAfterClear !== 0) {
    throw new Error(
      `WhatsApp private-box reader returned ${countAfterClear} bytes after clear; expected 0`,
    );
  }

  return {
    fixtureBytes,
    countAfterEnter,
    countAfterClear,
    whatsappComposerCharacters: privateBox.whatsappComposerCharacters,
  };
}
