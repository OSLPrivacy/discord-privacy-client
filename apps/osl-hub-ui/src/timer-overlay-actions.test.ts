import { describe, expect, it } from "vitest";
import {
  createTimerOverlaySaveActions,
  timerOverlaySaveDurationSeconds,
  TIMER_OVERLAY_MAX_DAYS,
} from "./timer-overlay-actions";
import { defaultTimerOverlayState } from "./timer-overlay";

describe("TASK 0554 connect timer overlay save", () => {
  it("saves 90 seconds as the send timer value via a direct UI command", () => {
    const actions = createTimerOverlaySaveActions(defaultTimerOverlayState());
    actions.setField("minutes", "01");
    actions.setField("seconds", "30");
    const result = actions.save();
    console.log(
      `TASK0554_SAVE minutes=${actions.getState().minutes} seconds=${actions.getState().seconds} ok=${result.ok} send_timer_value_seconds=${actions.getSendTimerValueSeconds()}`,
    );
    expect(result).toEqual({ ok: true, durationSeconds: 90 });
    expect(actions.getSendTimerValueSeconds()).toBe(90);
    expect(actions.getValidationError()).toBeNull();
  });

  it("rejects 31 days via a direct UI command and reports the validation error", () => {
    const actions = createTimerOverlaySaveActions(defaultTimerOverlayState());
    actions.setField("days", "31");
    const result = actions.save();
    console.log(
      `TASK0554_SAVE days=${actions.getState().days} ok=${result.ok} error=${actions.getValidationError()} send_timer_value_seconds=${actions.getSendTimerValueSeconds()}`,
    );
    expect(result.ok).toBe(false);
    expect(result.ok === false && result.error).toBe(
      "OSL timer picker days must be between 00 and 30",
    );
    expect(actions.getSendTimerValueSeconds()).toBeNull();
    expect(actions.getValidationError()).toBe(
      "OSL timer picker days must be between 00 and 30",
    );
  });

  it("accepts the day-30 boundary (max), matching TIMER_OVERLAY_MAX_DAYS", () => {
    expect(TIMER_OVERLAY_MAX_DAYS).toBe(30);
    const state = { ...defaultTimerOverlayState(), days: "30" };
    const result = timerOverlaySaveDurationSeconds(state);
    console.log(`TASK0554_SAVE days=30 ok=${result.ok} durationSeconds=${result.ok ? result.durationSeconds : "n/a"}`);
    expect(result).toEqual({ ok: true, durationSeconds: 30 * 86_400 });
  });

  it("a failed save clears a previously saved send timer value", () => {
    const actions = createTimerOverlaySaveActions(defaultTimerOverlayState());
    actions.setField("seconds", "45");
    expect(actions.save().ok).toBe(true);
    expect(actions.getSendTimerValueSeconds()).toBe(45);

    actions.setField("days", "31");
    const rejected = actions.save();
    console.log(
      `TASK0554_SAVE_CLEARED ok=${rejected.ok} send_timer_value_seconds=${actions.getSendTimerValueSeconds()}`,
    );
    expect(rejected.ok).toBe(false);
    expect(actions.getSendTimerValueSeconds()).toBeNull();
  });

  it("rejects an all-zero save (duration must be at least one second)", () => {
    const result = timerOverlaySaveDurationSeconds(defaultTimerOverlayState());
    expect(result).toEqual({
      ok: false,
      error: "OSL timer picker duration must be at least 01 second",
    });
  });
});
