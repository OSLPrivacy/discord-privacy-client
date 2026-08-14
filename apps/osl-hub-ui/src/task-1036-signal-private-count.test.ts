import { describe, expect, it } from "vitest";
import {
  checkSignalPrivateCount,
  SignalPrivateBoxController,
  type SignalFoundBoxHandle,
  type SignalPrivateBoxReader,
} from "./signal-private-box";

const STUB_READER = process.env.OSL_TASK_1036_STUB_SIGNAL_PRIVATE_BOX_READER === "1";

class FoundSignalBox implements SignalFoundBoxHandle {
  readonly active = true;
  readonly activeWindowIndex = 0;
  readonly conversationNodeIndex = 1;
  readonly typingBoxNodeIndex = 2;
  value = "";
  locked = false;

  readValue(): string { return this.value; }
  isLocked(): boolean { return this.locked; }
  clear(): void { this.value = ""; }
  setLocked(locked: boolean): void { this.locked = locked; }
}

describe("TASK 1036 Signal private count", () => {
  it("reads multi-byte entry bytes and returns to zero after a clear command", () => {
    const signalBox = new FoundSignalBox();
    const controller = SignalPrivateBoxController.lockOver(signalBox);
    const reader: SignalPrivateBoxReader = {
      readPrivateByteCount(privateBox) {
        if (STUB_READER) {
          // Deliberately do nothing: do not inspect the post-command box and
          // return the stale count captured before entry.
          return 0;
        }
        return privateBox.privateByteCount;
      },
    };

    const check = checkSignalPrivateCount(controller, reader);

    expect(check.fixtureCharacters).toBe(35);
    expect(check.fixtureBytes).toBe(37);
    expect(check.fixtureBytes).toBeGreaterThan(check.fixtureCharacters);
    expect(check.countAfterEnter).toBe(37);
    expect(check.countAfterClear).toBe(0);
    expect(check.signalComposerCharacters).toBe(0);
    expect(signalBox.locked).toBe(true);
    expect(Array.from(signalBox.readValue())).toHaveLength(0);

    console.info(`TASK1036_FIXTURE_CHARACTERS=${check.fixtureCharacters}`);
    console.info(`TASK1036_FIXTURE_BYTES=${check.fixtureBytes}`);
    console.info(`TASK1036_COUNT_AFTER_ENTER=${check.countAfterEnter}`);
    console.info(`TASK1036_COUNT_AFTER_COMMAND_CLEAR=${check.countAfterClear}`);
    console.info(`TASK1036_SIGNAL_BOX_CHARACTERS=${check.signalComposerCharacters}`);
  });
});
