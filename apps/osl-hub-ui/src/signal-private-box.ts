import { signalAttachmentTrayMarkup } from "./signal-attachment-tray";

/**
 * Structural receipt supplied by the Signal finder gate.
 *
 * Signal's localized textbox name is deliberately absent. The native finder
 * identifies the active window, conversation, and typing box structurally and
 * hands the renderer only their opaque indices.
 */
export interface SignalFoundBoxHandle {
  readonly active: boolean;
  readonly activeWindowIndex: number;
  readonly conversationNodeIndex: number;
  readonly typingBoxNodeIndex: number;
  readValue(): string;
  isLocked(): boolean;
  clear(): void;
  setLocked(locked: boolean): void;
}

export interface SignalPrivateBoxState {
  readonly activeWindowIndex: number;
  readonly conversationNodeIndex: number;
  readonly typingBoxNodeIndex: number;
  readonly locked: true;
  /** Text held by OSL's private box, never Signal's composer. */
  readonly privateDraft: string;
  /** A fresh copy of the exact UTF-8 bytes represented by `privateDraft`. */
  readonly privateBytes: Uint8Array;
  /** Live UTF-8 byte count displayed beside the OSL-owned private input. */
  readonly privateByteCount: number;
  readonly signalComposerCharacters: 0;
}

export type SignalPrivateBoxCommand =
  | { readonly type: "enterPrivateText"; readonly text: string }
  | { readonly type: "clearPrivateText" };

/**
 * Reads the OSL-owned private box after a command has completed. Keeping this
 * boundary independent prevents a count check from trusting the command input.
 */
export interface SignalPrivateBoxReader {
  readPrivateByteCount(privateBox: SignalPrivateBoxState): number;
}

export interface SignalPrivateCountCheck {
  readonly fixtureCharacters: number;
  readonly fixtureBytes: number;
  readonly countAfterEnter: number;
  readonly countAfterClear: number;
  readonly signalComposerCharacters: 0;
}

function signalCharacterCount(value: string): number {
  return Array.from(value).length;
}

function validFinderReceipt(foundBox: SignalFoundBoxHandle): boolean {
  const indices = [
    foundBox.activeWindowIndex,
    foundBox.conversationNodeIndex,
    foundBox.typingBoxNodeIndex,
  ];
  return foundBox.active
    && indices.every((index) => Number.isSafeInteger(index) && index >= 0)
    && foundBox.conversationNodeIndex !== foundBox.typingBoxNodeIndex;
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
 * Owns the private draft displayed over the typing box found by the Signal
 * gate. Every transition reasserts both invariants: Signal stays locked, and
 * its own composer stays empty.
 */
export class SignalPrivateBoxController {
  readonly #foundBox: SignalFoundBoxHandle;
  #privateDraft = "";

  private constructor(foundBox: SignalFoundBoxHandle) {
    this.#foundBox = foundBox;
    this.#reconcileSignalBox();
  }

  static lockOver(foundBox: SignalFoundBoxHandle): SignalPrivateBoxController {
    if (!validFinderReceipt(foundBox)) {
      throw new Error("Signal typing box was not found");
    }
    return new SignalPrivateBoxController(foundBox);
  }

  #reconcileSignalBox(): void {
    this.#foundBox.setLocked(true);
    this.#foundBox.clear();
    if (!this.#foundBox.isLocked()) {
      throw new Error("Signal typing box did not lock");
    }
    if (signalCharacterCount(this.#foundBox.readValue()) !== 0) {
      throw new Error("Signal typing box did not clear");
    }
  }

  typePrivate(text: string): SignalPrivateBoxState {
    this.#privateDraft = text;
    return this.state();
  }

  clearPrivate(): SignalPrivateBoxState {
    this.#privateDraft = "";
    return this.state();
  }

  state(): SignalPrivateBoxState {
    this.#reconcileSignalBox();
    const privateBytes = new TextEncoder().encode(this.#privateDraft);
    return {
      activeWindowIndex: this.#foundBox.activeWindowIndex,
      conversationNodeIndex: this.#foundBox.conversationNodeIndex,
      typingBoxNodeIndex: this.#foundBox.typingBoxNodeIndex,
      locked: true,
      privateDraft: this.#privateDraft,
      privateBytes,
      privateByteCount: privateBytes.length,
      signalComposerCharacters: 0,
    };
  }
}

/** Apply an explicit private-box command while preserving Signal's locked box. */
export function executeSignalPrivateBoxCommand(
  controller: SignalPrivateBoxController,
  command: SignalPrivateBoxCommand,
): SignalPrivateBoxState {
  switch (command.type) {
    case "enterPrivateText":
      return controller.typePrivate(command.text);
    case "clearPrivateText":
      return controller.clearPrivate();
  }
}

/**
 * Execute multi-byte entry and clear commands, observing the private box after
 * each. A stale or no-op reader cannot satisfy the entry assertion.
 */
export function checkSignalPrivateCount(
  controller: SignalPrivateBoxController,
  reader: SignalPrivateBoxReader,
): SignalPrivateCountCheck {
  const fixture = `${"a".repeat(34)}€`;
  const fixtureCharacters = Array.from(fixture).length;
  const fixtureBytes = new TextEncoder().encode(fixture).length;

  const entered = executeSignalPrivateBoxCommand(controller, {
    type: "enterPrivateText",
    text: fixture,
  });
  const countAfterEnter = reader.readPrivateByteCount(entered);
  if (countAfterEnter !== fixtureBytes) {
    throw new Error(
      `Signal private-box reader returned ${countAfterEnter} bytes after enter; expected ${fixtureBytes}`,
    );
  }

  const cleared = executeSignalPrivateBoxCommand(controller, { type: "clearPrivateText" });
  const countAfterClear = reader.readPrivateByteCount(cleared);
  if (countAfterClear !== 0) {
    throw new Error(
      `Signal private-box reader returned ${countAfterClear} bytes after clear; expected 0`,
    );
  }

  return {
    fixtureCharacters,
    fixtureBytes,
    countAfterEnter,
    countAfterClear,
    signalComposerCharacters: cleared.signalComposerCharacters,
  };
}

/** Pure markup for the locked private box positioned over Signal's composer. */
export function signalPrivateBoxMarkup(state: SignalPrivateBoxState): string {
  return `<section class="signal-private-box" data-over-signal-node="${state.typingBoxNodeIndex}" data-lock-state="locked" aria-label="OSL private message box">`
    + `<strong class="signal-private-lock" aria-label="Signal typing box locked">🔒 Locked</strong>`
    + `<label for="signal-private-draft">Private message</label>`
    + `<textarea id="signal-private-draft" autocomplete="off" spellcheck="true" aria-describedby="signal-private-draft-bytes">${escapeHtml(state.privateDraft)}</textarea>`
    + `<output id="signal-private-draft-bytes" aria-live="polite">${state.privateByteCount} bytes</output>`
    + signalAttachmentTrayMarkup([])
    + `</section>`;
}
