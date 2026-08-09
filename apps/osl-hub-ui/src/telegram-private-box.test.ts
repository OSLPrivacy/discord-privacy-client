import { describe, expect, it } from "vitest";
import {
  TELEGRAM_FOUND_BOX_NAME,
  TelegramPrivateBoxController,
  telegramPrivateBoxMarkup,
  type TelegramFoundBoxHandle,
} from "./telegram-private-box";

const FIXTURE = "telegram-private-1005|café|🔒";
const TWELVE_BYTE_FIXTURE = "hello world!";

class FoundTelegramBox implements TelegramFoundBoxHandle {
  readonly accessibleName: string;
  readonly active: boolean;
  value: string;
  locked = false;

  constructor(accessibleName = TELEGRAM_FOUND_BOX_NAME, active = true, value = "") {
    this.accessibleName = accessibleName;
    this.active = active;
    this.value = value;
  }

  readValue(): string { return this.value; }
  isLocked(): boolean { return this.locked; }
  clear(): void { this.value = ""; }
  setLocked(locked: boolean): void { this.locked = locked; }
}

describe("Telegram locked private box", () => {
  it("holds the exact typed bytes while Telegram's found box stays empty", () => {
    const telegramBox = new FoundTelegramBox(TELEGRAM_FOUND_BOX_NAME, true, "old Telegram draft");
    const privateBox = TelegramPrivateBoxController.lockOver(telegramBox);
    const typed = privateBox.typePrivate(FIXTURE);
    const expectedBytes = new TextEncoder().encode(FIXTURE);

    expect(typed.locked).toBe(true);
    expect(telegramBox.locked).toBe(true);
    expect(typed.foundBoxName).toBe(TELEGRAM_FOUND_BOX_NAME);
    expect(typed.privateDraft).toBe(FIXTURE);
    expect(typed.privateBytes).toEqual(expectedBytes);
    expect(typed.privateByteCount).toBe(expectedBytes.length);
    expect(typed.telegramComposerCharacters).toBe(0);
    expect(Array.from(telegramBox.readValue())).toHaveLength(0);

    const markup = telegramPrivateBoxMarkup(typed);
    expect(markup).toContain('data-over-telegram-box="Write a message..."');
    expect(markup).toContain('data-lock-state="locked"');
    expect(markup).toContain('aria-label="OSL private message box"');
    expect(markup).toContain("telegram-private-draft");

    console.info("TASK1005_LOCKED=true");
    console.info(`TASK1005_TYPED_TEXT=${typed.privateDraft}`);
    console.info(`TASK1005_TYPED_BYTES=${typed.privateBytes.length}`);
    console.info("TASK1005_EXACT_TYPED_BYTES=true");
    console.info(`TASK1005_TELEGRAM_BOX_CHARACTERS=${typed.telegramComposerCharacters}`);
  });

  it("renders the live private-box byte count and returns it to zero after clear", () => {
    const telegramBox = new FoundTelegramBox();
    const privateBox = TelegramPrivateBoxController.lockOver(telegramBox);
    const typed = privateBox.typePrivate(TWELVE_BYTE_FIXTURE);

    expect(new TextEncoder().encode(TWELVE_BYTE_FIXTURE)).toHaveLength(12);
    expect(typed.privateByteCount).toBe(12);
    expect(telegramPrivateBoxMarkup(typed)).toContain('id="telegram-private-draft-bytes" aria-live="polite">12 bytes</output>');

    const cleared = privateBox.clearPrivate();
    expect(cleared.privateByteCount).toBe(0);
    expect(telegramPrivateBoxMarkup(cleared)).toContain('id="telegram-private-draft-bytes" aria-live="polite">0 bytes</output>');

    console.info(`TASK1006_TYPED_BYTE_COUNT=${typed.privateByteCount}`);
    console.info(`TASK1006_CLEARED_BYTE_COUNT=${cleared.privateByteCount}`);
  });

  it("clears the private box to zero and continues to keep Telegram empty", () => {
    const telegramBox = new FoundTelegramBox();
    const privateBox = TelegramPrivateBoxController.lockOver(telegramBox);
    privateBox.typePrivate(FIXTURE);

    // Even if the provider box changes between private input events, the next
    // private-box transition must restore the locked-empty invariant.
    telegramBox.value = "must not remain in Telegram";
    const cleared = privateBox.clearPrivate();

    expect(cleared.privateDraft).toBe("");
    expect(cleared.privateBytes).toHaveLength(0);
    expect(cleared.telegramComposerCharacters).toBe(0);
    expect(Array.from(telegramBox.readValue())).toHaveLength(0);
    expect(telegramBox.locked).toBe(true);

    console.info(`TASK1005_PRIVATE_BYTES_AFTER_CLEAR=${cleared.privateBytes.length}`);
    console.info(`TASK1005_TELEGRAM_BOX_CHARACTERS_AFTER_CLEAR=${cleared.telegramComposerCharacters}`);
  });

  it("refuses to render over a search field or inactive Telegram box", () => {
    expect(() => TelegramPrivateBoxController.lockOver(new FoundTelegramBox("Search"))).toThrow(
      "Telegram message box was not found",
    );
    expect(() => TelegramPrivateBoxController.lockOver(
      new FoundTelegramBox(TELEGRAM_FOUND_BOX_NAME, false),
    )).toThrow("Telegram message box was not found");
  });
});
