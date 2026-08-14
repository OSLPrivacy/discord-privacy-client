import { describe, expect, it } from "vitest";
import {
  checkTelegramPrivateCount,
  type TelegramPrivateBoxReader,
} from "./telegram-private-box";

const STUB_READER = process.env.OSL_TASK_1007_STUB_TELEGRAM_PRIVATE_BOX_READER === "1";

describe("TASK 1007 Telegram private count", () => {
  it("reads multi-byte entry bytes and returns to zero after a direct clear command", () => {
    const reader: TelegramPrivateBoxReader = {
      readPrivateByteCount(privateBox) {
        if (STUB_READER) {
          // Deliberately do nothing: do not inspect the post-command box and
          // return the stale empty count captured before entry.
          return 0;
        }
        return privateBox.privateByteCount;
      },
    };

    const check = checkTelegramPrivateCount(reader);

    expect(check.fixtureBytes).toBe(37);
    expect(check.countAfterEnter).toBe(37);
    expect(check.countAfterClear).toBe(0);
    expect(check.telegramComposerCharacters).toBe(0);

    console.info(`TASK1007_FIXTURE_BYTES=${check.fixtureBytes}`);
    console.info(`TASK1007_COUNT_AFTER_ENTER=${check.countAfterEnter}`);
    console.info(`TASK1007_COUNT_AFTER_DIRECT_CLEAR=${check.countAfterClear}`);
    console.info(`TASK1007_TELEGRAM_COMPOSER_CHARACTERS=${check.telegramComposerCharacters}`);
  });
});
