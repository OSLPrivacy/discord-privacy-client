import { describe, expect, it } from "vitest";
import {
  checkWhatsAppPrivateCount,
  type WhatsAppPrivateBoxReader,
} from "./whatsapp-private-box";
import { utf8Length } from "./overlay-state";

const STUB_READER = process.env.OSL_TASK_1070_STUB_WHATSAPP_PRIVATE_BOX_READER === "1";

describe("TASK 1070 WhatsApp private count", () => {
  it("reads multi-byte entry bytes and returns to zero after a direct clear command", () => {
    const reader: WhatsAppPrivateBoxReader = {
      readPrivateByteCount(privateBox) {
        if (STUB_READER) {
          // Deliberately do nothing: do not inspect the post-command box and
          // return the stale count captured before entry.
          return 0;
        }
        return utf8Length(privateBox.privateDraft);
      },
    };

    const check = checkWhatsAppPrivateCount(reader);

    expect(check.fixtureBytes).toBe(37);
    expect(check.countAfterEnter).toBe(37);
    expect(check.countAfterClear).toBe(0);
    expect(check.whatsappComposerCharacters).toBe(0);

    console.info(`TASK1070_FIXTURE_BYTES=${check.fixtureBytes}`);
    console.info(`TASK1070_COUNT_AFTER_ENTER=${check.countAfterEnter}`);
    console.info(`TASK1070_COUNT_AFTER_DIRECT_CLEAR=${check.countAfterClear}`);
    console.info(`TASK1070_WHATSAPP_COMPOSER_CHARACTERS=${check.whatsappComposerCharacters}`);
  });
});
