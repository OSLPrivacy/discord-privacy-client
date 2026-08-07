import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { NativeDiscordCarrierRowBinding } from "./discord-carrier-row-binding";
import type { NativeDiscordOverlayOpened } from "./overlay-state";
import {
  NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE,
  visibleOpenedMessagesForCarrierRows,
} from "./native-overlay-row-visibility";

const privateWords = "task3956 private words";

const opened: NativeDiscordOverlayOpened = {
  messageId: "peer-39560000000000000000000000000000",
  coverPointer: "ordinary cover text for task 3956",
  plaintext: privateWords,
  contextVerified: true,
  personToPersonE2ee: true,
  viewOnceConsumed: false,
  createdAt: 1_786_000_000,
  expiresAt: 1_786_003_600,
};

const visibleCarrierRow: NativeDiscordCarrierRowBinding = {
  messageId: opened.messageId,
  nativeLocatorSha256: "1".repeat(64),
  carrierSha256: "2".repeat(64),
  leftPx: 160,
  topPx: 320,
  widthPx: 480,
  heightPx: 36,
  backgroundColor: "rgb(49 51 56)",
  foregroundColor: "rgb(219 222 225)",
  fontFamily: "gg sans",
  fontSizePx: 16,
  fontWeight: 400,
  lineHeightPx: 20,
  letterSpacingPx: 0,
  zoom: 1,
  density: 1.25,
};

function screenRows(messages: readonly NativeDiscordOverlayOpened[], statusText: string): readonly string[] {
  return [
    ...messages.map((message) => message.plaintext),
    opened.coverPointer ?? "",
    statusText,
  ];
}

function privateWordRowCount(rows: readonly string[]): number {
  return rows.filter((row) => row.includes(privateWords)).length;
}

describe("native overlay missing cover row visibility", () => {
  it("shows private words only while the exact cover row can still be found", () => {
    const before = visibleOpenedMessagesForCarrierRows([opened], [visibleCarrierRow]);
    const beforeScreen = screenRows(before.visibleMessages, "1 private message received through OSL.");

    const after = visibleOpenedMessagesForCarrierRows([opened], []);
    const afterScreen = screenRows(after.visibleMessages, NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE);

    const beforePrivateRows = privateWordRowCount(beforeScreen);
    const afterPrivateRows = privateWordRowCount(afterScreen);
    const privateWordsElsewhereAfter = privateWordRowCount([
      opened.coverPointer ?? "",
      NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE,
    ]);

    console.info(`TASK3956_BEFORE_PRIVATE_ROWS=${beforePrivateRows}`);
    console.info(`TASK3956_AFTER_PRIVATE_ROWS=${afterPrivateRows}`);
    console.info(`TASK3956_PRIVATE_WORDS_ELSEWHERE_AFTER=${privateWordsElsewhereAfter}`);
    console.info(`TASK3956_STATUS=${JSON.stringify(NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE)}`);

    expect(beforePrivateRows).toBe(1);
    expect(afterPrivateRows).toBe(0);
    expect(privateWordsElsewhereAfter).toBe(0);
    expect(after.missingCoverRows).toBe(1);
    expect(afterScreen).toContain(NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE);
  });

  it("is wired into the receive path instead of being an unused model", () => {
    const source = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
    expect(source).toContain("visibleOpenedMessagesForCarrierRows(batch.messages, visibleCarrierRows)");
    expect(source).toContain("for (const message of rowVisibility.visibleMessages)");
    expect(source).toContain("incomingBubbles.get(binding.messageId) ?? outgoingBubbles.get(binding.messageId)");
    expect(source).toContain("if (opened > 0) paintBoundRows();");
    expect(source).toContain("status.textContent = NATIVE_DISCORD_MISSING_COVER_ROW_NOTICE");
  });
});
