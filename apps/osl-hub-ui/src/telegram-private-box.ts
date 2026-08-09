/** The exact Telegram composer name established by the typing-box gate. */
export const TELEGRAM_FOUND_BOX_NAME = "Write a message...";

/**
 * Narrow bridge to the Telegram box found by the native accessibility gate.
 * The renderer owns the private draft; this handle is used only to lock and
 * keep Telegram's box empty while private typing is active.
 */
export interface TelegramFoundBoxHandle {
  readonly accessibleName: string;
  readonly active: boolean;
  readValue(): string;
  isLocked(): boolean;
  clear(): void;
  setLocked(locked: boolean): void;
}

export interface TelegramPrivateBoxState {
  readonly foundBoxName: typeof TELEGRAM_FOUND_BOX_NAME;
  readonly locked: true;
  /** Text held by OSL's private message box, never Telegram's composer. */
  readonly privateDraft: string;
  /** A fresh copy of the exact UTF-8 bytes represented by `privateDraft`. */
  readonly privateBytes: Uint8Array;
  /** Live UTF-8 byte count displayed beside the OSL-owned private input. */
  readonly privateByteCount: number;
  readonly telegramComposerCharacters: 0;
}

function telegramCharacterCount(value: string): number {
  return Array.from(value).length;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

/**
 * Controller for the OSL-owned private box positioned over a found Telegram
 * composer. Every transition reasserts both invariants: Telegram stays locked,
 * and its own composer stays empty.
 */
export class TelegramPrivateBoxController {
  readonly #foundBox: TelegramFoundBoxHandle;
  #privateDraft = "";

  private constructor(foundBox: TelegramFoundBoxHandle) {
    this.#foundBox = foundBox;
    this.#reconcileTelegramBox();
  }

  static lockOver(foundBox: TelegramFoundBoxHandle): TelegramPrivateBoxController {
    if (!foundBox.active || foundBox.accessibleName !== TELEGRAM_FOUND_BOX_NAME) {
      throw new Error("Telegram message box was not found");
    }
    return new TelegramPrivateBoxController(foundBox);
  }

  #reconcileTelegramBox(): void {
    this.#foundBox.setLocked(true);
    this.#foundBox.clear();
    if (!this.#foundBox.isLocked()) {
      throw new Error("Telegram message box did not lock");
    }
    if (telegramCharacterCount(this.#foundBox.readValue()) !== 0) {
      throw new Error("Telegram message box did not clear");
    }
  }

  typePrivate(text: string): TelegramPrivateBoxState {
    this.#privateDraft = text;
    return this.state();
  }

  clearPrivate(): TelegramPrivateBoxState {
    this.#privateDraft = "";
    return this.state();
  }

  state(): TelegramPrivateBoxState {
    this.#reconcileTelegramBox();
    const privateBytes = new TextEncoder().encode(this.#privateDraft);
    return {
      foundBoxName: TELEGRAM_FOUND_BOX_NAME,
      locked: true,
      privateDraft: this.#privateDraft,
      privateBytes,
      privateByteCount: privateBytes.length,
      telegramComposerCharacters: 0,
    };
  }
}

/** Pure markup for the locked box the overlay renderer places over Telegram. */
export function telegramPrivateBoxMarkup(state: TelegramPrivateBoxState): string {
  return `<section class="telegram-private-box" data-over-telegram-box="${escapeHtml(state.foundBoxName)}" data-lock-state="locked" aria-label="OSL private message box">`
    + `<strong class="telegram-private-lock" aria-label="Telegram message box locked">🔒 Locked</strong>`
    + `<label for="telegram-private-draft">Private message</label>`
    + `<textarea id="telegram-private-draft" autocomplete="off" spellcheck="true" aria-describedby="telegram-private-draft-bytes">${escapeHtml(state.privateDraft)}</textarea>`
    + `<output id="telegram-private-draft-bytes" aria-live="polite">${state.privateByteCount} bytes</output>`
    + `</section>`;
}
