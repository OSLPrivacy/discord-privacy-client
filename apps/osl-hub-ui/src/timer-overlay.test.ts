import { describe, expect, it } from "vitest";
import {
  defaultTimerOverlayState,
  TIMER_OVERLAY_FIELDS,
  timerOverlayMarkup,
} from "./timer-overlay";

describe("defaultTimerOverlayState", () => {
  it("starts every field at two-digit zero, matching security.rs::default_timer_picker_state", () => {
    expect(defaultTimerOverlayState()).toEqual({
      days: "00",
      hours: "00",
      minutes: "00",
      seconds: "00",
    });
  });
});

describe("timerOverlayMarkup", () => {
  it("renders exactly Days, Hours, Minutes, and Seconds fields in that order", () => {
    expect(TIMER_OVERLAY_FIELDS.map((field) => field.label)).toEqual([
      "Days",
      "Hours",
      "Minutes",
      "Seconds",
    ]);
  });

  it("renders a greyed-out scrim behind the panel", () => {
    const markup = timerOverlayMarkup();
    expect(markup).toContain('class="timer-overlay-scrim"');
    expect(markup).toContain('class="timer-overlay-panel"');
  });

  it("defaults Days to 00 in the rendered markup", () => {
    const markup = timerOverlayMarkup();
    expect(markup).toContain('id="timer-overlay-days"');
    expect(markup).toMatch(/id="timer-overlay-days"[^]*?value="00"/u);
  });

  it("reflects a non-default state for every field", () => {
    const markup = timerOverlayMarkup({ days: "07", hours: "12", minutes: "34", seconds: "56" });
    expect(markup).toMatch(/id="timer-overlay-days"[^]*?value="07"/u);
    expect(markup).toMatch(/id="timer-overlay-hours"[^]*?value="12"/u);
    expect(markup).toMatch(/id="timer-overlay-minutes"[^]*?value="34"/u);
    expect(markup).toMatch(/id="timer-overlay-seconds"[^]*?value="56"/u);
  });
});
