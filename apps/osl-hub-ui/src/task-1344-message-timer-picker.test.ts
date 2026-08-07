import { describe, expect, it } from "vitest";
import {
  MESSAGE_TIMER_MAX_DAYS,
  MESSAGE_TIMER_MAX_SECONDS,
  isValidMessageTimerFields,
  messageTimerDurationWords,
  messageTimerPickerMarkup,
  messageTimerTotalSeconds,
  type MessageTimerFields,
} from "./message-timer-picker";

// A fixture screen: the composer's timer choice shown with all four fields,
// matching crates/ipc/src/message_expiry_dial.rs::MessageTimer.
const FIXTURE_SCREEN: MessageTimerFields = { days: 3, hours: 4, minutes: 5, seconds: 6 };
const THIRTY_DAY_SCREEN: MessageTimerFields = { days: MESSAGE_TIMER_MAX_DAYS, hours: 0, minutes: 0, seconds: 0 };
const OVER_MAX_SCREEN: MessageTimerFields = { days: MESSAGE_TIMER_MAX_DAYS, hours: 0, minutes: 0, seconds: 1 };

describe("message timer picker fixture screen", () => {
  it("shows all four fields: Days, Hours, Minutes, Seconds", () => {
    const markup = messageTimerPickerMarkup(FIXTURE_SCREEN);

    const fieldCount = (["days", "hours", "minutes", "seconds"] as const)
      .filter((key) => markup.includes(`id="message-timer-${key}"`)).length;
    const labelCount = ["Days", "Hours", "Minutes", "Seconds"]
      .filter((label) => markup.includes(`<span>${label}</span>`)).length;

    console.log(
      `TASK1344 fixture=fixture-screen field_count=${fieldCount} label_count=${labelCount} ` +
      `total_seconds=${messageTimerTotalSeconds(FIXTURE_SCREEN)}`,
    );

    expect(fieldCount).toBe(4);
    expect(labelCount).toBe(4);
    expect(markup).toContain('id="message-timer-days"');
    expect(markup).toContain('id="message-timer-hours"');
    expect(markup).toContain('id="message-timer-minutes"');
    expect(markup).toContain('id="message-timer-seconds"');
  });

  it("carries a 30-day maximum on the fixture screen", () => {
    const markup = messageTimerPickerMarkup(THIRTY_DAY_SCREEN);

    console.log(
      `TASK1344 fixture=thirty-day-screen max_days=${MESSAGE_TIMER_MAX_DAYS} ` +
      `max_seconds=${MESSAGE_TIMER_MAX_SECONDS} accepted_total_seconds=${messageTimerTotalSeconds(THIRTY_DAY_SCREEN)} ` +
      `valid=${isValidMessageTimerFields(THIRTY_DAY_SCREEN)}`,
    );

    expect(markup).toContain(`max="${MESSAGE_TIMER_MAX_DAYS}"`);
    expect(markup).toContain('data-max-days="30"');
    expect(isValidMessageTimerFields(THIRTY_DAY_SCREEN)).toBe(true);
    expect(messageTimerTotalSeconds(THIRTY_DAY_SCREEN)).toBe(MESSAGE_TIMER_MAX_SECONDS);
    expect(messageTimerDurationWords(THIRTY_DAY_SCREEN)).toBe("30 days");
  });

  it("rejects one second past the 30-day maximum instead of clamping it", () => {
    const markup = messageTimerPickerMarkup(OVER_MAX_SCREEN);

    console.log(
      `TASK1344 fixture=over-max-screen total_seconds=${messageTimerTotalSeconds(OVER_MAX_SCREEN)} ` +
      `valid=${isValidMessageTimerFields(OVER_MAX_SCREEN)}`,
    );

    expect(messageTimerTotalSeconds(OVER_MAX_SCREEN)).toBe(MESSAGE_TIMER_MAX_SECONDS + 1);
    expect(isValidMessageTimerFields(OVER_MAX_SCREEN)).toBe(false);
    expect(markup).toContain("message-timer-picker-error");
    expect(markup).toContain("30 days");
  });
});
