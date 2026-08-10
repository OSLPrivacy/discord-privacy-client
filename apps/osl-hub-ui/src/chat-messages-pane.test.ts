import { describe, expect, it } from "vitest";
import {
  CHAT_CORNER_ROUNDING,
  CHAT_SPACING,
  CHAT_TEXT_SIZE,
  DEFAULT_CHAT_MESSAGES_PREFERENCES,
  normaliseChatMessagesPreferences,
  saveChatMessagesPreferences,
  type ChatMessagesPreferences,
} from "./chat-messages-pane";

function checkStoredWalls(value: ChatMessagesPreferences): void {
  if (value.textSize < CHAT_TEXT_SIZE.min || value.textSize > CHAT_TEXT_SIZE.max) {
    throw new Error(`textSize=${value.textSize} outside ${CHAT_TEXT_SIZE.min}..${CHAT_TEXT_SIZE.max}`);
  }
  if (value.cornerRounding < CHAT_CORNER_ROUNDING.min || value.cornerRounding > CHAT_CORNER_ROUNDING.max) {
    throw new Error(`cornerRounding=${value.cornerRounding} outside ${CHAT_CORNER_ROUNDING.min}..${CHAT_CORNER_ROUNDING.max}`);
  }
  if (value.spacing < CHAT_SPACING.min || value.spacing > CHAT_SPACING.max) {
    throw new Error(`spacing=${value.spacing} outside ${CHAT_SPACING.min}..${CHAT_SPACING.max}`);
  }
}

describe("TASK 5065b messages pane walls", () => {
  it("stops text size, rounding, and spacing at their walls", () => {
    const low = normaliseChatMessagesPreferences({ textSize: 12 });
    const high = normaliseChatMessagesPreferences({ textSize: 21, cornerRounding: 21, spacing: 17 });
    console.log(`TASK5065B walls text_low=${low.textSize} text_high=${high.textSize} rounding_high=${high.cornerRounding} spacing_high=${high.spacing}`);
    expect(low.textSize).toBe(13);
    expect(high.textSize).toBe(20);
    expect(high.cornerRounding).toBe(20);
    expect(high.spacing).toBe(16);
  });

  it("keeps the previous colour when the custom hex is junk", () => {
    const previous = DEFAULT_CHAT_MESSAGES_PREFERENCES.messageColour;
    const afterJunk = normaliseChatMessagesPreferences({ messageColour: "not-a-colour" });
    console.log(`TASK5065B junk_hex=not-a-colour colour_unchanged=${afterJunk.messageColour === previous} colour=${afterJunk.messageColour}`);
    expect(afterJunk.messageColour).toBe(previous);
  });

  it("makes a throwaway stored 21px copy fail with the offending value named", () => {
    const throwaway = { ...DEFAULT_CHAT_MESSAGES_PREFERENCES, textSize: 21 };
    let message = "";
    try { checkStoredWalls(throwaway); } catch (error) { message = String(error); }
    console.log(`TASK5065B throwaway_check=red message=${message}`);
    expect(message).toContain("textSize=21");
    expect(() => checkStoredWalls(saveChatMessagesPreferences(throwaway, null))).not.toThrow();
  });
});
