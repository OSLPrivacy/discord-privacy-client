import { describe, expect, it } from "vitest";
import {
  SignalPrivateBoxController,
  signalPrivateBoxMarkup,
  type SignalFoundBoxHandle,
} from "./signal-private-box";

const FIXTURE = "Signal|private|café|🔒|1035|_37abc";

class FoundSignalBox implements SignalFoundBoxHandle {
  readonly active: boolean;
  readonly activeWindowIndex: number;
  readonly conversationNodeIndex: number;
  readonly typingBoxNodeIndex: number;
  value: string;
  locked = false;

  constructor({
    active = true,
    activeWindowIndex = 0,
    conversationNodeIndex = 1,
    typingBoxNodeIndex = 2,
    value = "",
  }: Partial<{
    active: boolean;
    activeWindowIndex: number;
    conversationNodeIndex: number;
    typingBoxNodeIndex: number;
    value: string;
  }> = {}) {
    this.active = active;
    this.activeWindowIndex = activeWindowIndex;
    this.conversationNodeIndex = conversationNodeIndex;
    this.typingBoxNodeIndex = typingBoxNodeIndex;
    this.value = value;
  }

  readValue(): string { return this.value; }
  isLocked(): boolean { return this.locked; }
  clear(): void { this.value = ""; }
  setLocked(locked: boolean): void { this.locked = locked; }
}

describe("Task 1035 Signal locked private box", () => {
  it("shows exactly 37 fixture bytes while Signal's own box holds zero characters", () => {
    const expectedBytes = new TextEncoder().encode(FIXTURE);
    expect(expectedBytes).toHaveLength(37);
    expect(Array.from(FIXTURE)).not.toHaveLength(37);

    const signalBox = new FoundSignalBox({ value: "old Signal draft" });
    const privateBox = SignalPrivateBoxController.lockOver(signalBox);
    const typed = privateBox.typePrivate(FIXTURE);

    expect(typed.locked).toBe(true);
    expect(signalBox.locked).toBe(true);
    expect(typed.privateDraft).toBe(FIXTURE);
    expect(typed.privateBytes).toEqual(expectedBytes);
    expect(typed.privateByteCount).toBe(37);
    expect(typed.signalComposerCharacters).toBe(0);
    expect(Array.from(signalBox.readValue())).toHaveLength(0);

    const markup = signalPrivateBoxMarkup(typed);
    expect(markup).toContain('data-over-signal-node="2"');
    expect(markup).toContain('data-lock-state="locked"');
    expect(markup).toContain('aria-label="OSL private message box"');
    expect(markup).toContain('id="signal-private-draft-bytes" aria-live="polite">37 bytes</output>');

    console.info(`TASK1035_FIXTURE_BYTES=${expectedBytes.length}`);
    console.info(`TASK1035_COUNTER_AFTER_TYPE=${typed.privateByteCount}`);
    console.info(`TASK1035_SIGNAL_BOX_CHARACTERS=${typed.signalComposerCharacters}`);
  });

  it("returns the counter to zero after clear and keeps Signal empty", () => {
    const signalBox = new FoundSignalBox();
    const privateBox = SignalPrivateBoxController.lockOver(signalBox);
    privateBox.typePrivate(FIXTURE);

    // Reassert the locked-empty invariant if the provider box drifts between
    // private-box input events.
    signalBox.value = "must not remain in Signal";
    const cleared = privateBox.clearPrivate();

    expect(cleared.privateDraft).toBe("");
    expect(cleared.privateBytes).toHaveLength(0);
    expect(cleared.privateByteCount).toBe(0);
    expect(cleared.signalComposerCharacters).toBe(0);
    expect(signalBox.locked).toBe(true);
    expect(Array.from(signalBox.readValue())).toHaveLength(0);
    expect(signalPrivateBoxMarkup(cleared)).toContain(
      'id="signal-private-draft-bytes" aria-live="polite">0 bytes</output>',
    );

    console.info(`TASK1035_COUNTER_AFTER_CLEAR=${cleared.privateByteCount}`);
    console.info(`TASK1035_SIGNAL_BOX_CHARACTERS_AFTER_CLEAR=${cleared.signalComposerCharacters}`);
  });

  it("refuses an inactive or malformed Signal finder receipt", () => {
    expect(() => SignalPrivateBoxController.lockOver(
      new FoundSignalBox({ active: false }),
    )).toThrow("Signal typing box was not found");
    expect(() => SignalPrivateBoxController.lockOver(
      new FoundSignalBox({ conversationNodeIndex: 2, typingBoxNodeIndex: 2 }),
    )).toThrow("Signal typing box was not found");
  });
});
