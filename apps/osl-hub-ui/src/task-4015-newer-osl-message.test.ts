import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import type { NativeDiscordOverlayOpenedBatch } from "./overlay-state";

const NEWER_OSL_SENTENCE = "A protected message needs a newer version of OSL to open.";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function receiveStatusText(batch: NativeDiscordOverlayOpenedBatch): string {
  const status = { textContent: "" };
  const opened = batch.messages.length;
  if (opened > 0) status.textContent = `${opened} private ${opened === 1 ? "message" : "messages"} received through OSL.`;
  else if (batch.deferredRows > 0) status.textContent = "OSL could not reach the protected message store. Retrying.";
  else if (batch.unrecognizedWireRows > 0) status.textContent = NEWER_OSL_SENTENCE;
  else if (!batch.decryptDisplayEnabled) status.textContent = "Decrypted text is off for this conversation.";
  return status.textContent;
}

describe("TASK 4015 newer OSL message notice", () => {
  it("puts the exact newer-OSL sentence on screen without reporting opened private messages", () => {
    const batch: NativeDiscordOverlayOpenedBatch = {
      messages: [],
      pendingViewOnce: [],
      acknowledgments: [],
      fetched: 0,
      decryptDisplayEnabled: true,
      deferredRows: 0,
      unrecognizedWireRows: 1,
    };
    const source = readRelative("./overlay.ts");
    const screenSentence = receiveStatusText(batch);
    const openedPrivateMessages = batch.messages.length;

    console.log(`TASK4015_SCREEN_OPENED_PRIVATE_MESSAGES=${openedPrivateMessages}`);
    console.log(`TASK4015_SCREEN_SENTENCE=${screenSentence}`);

    expect(openedPrivateMessages).toBe(0);
    expect(screenSentence).toBe(NEWER_OSL_SENTENCE);
    expect(source).toContain(`status.textContent = "${NEWER_OSL_SENTENCE}"`);
    expect(source.indexOf("batch.unrecognizedWireRows > 0")).toBeLessThan(
      source.indexOf(`status.textContent = "${NEWER_OSL_SENTENCE}"`),
    );
  });
});
