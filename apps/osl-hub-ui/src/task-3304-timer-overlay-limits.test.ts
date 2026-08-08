import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import {
  TIMER_OVERLAY_CHOICES,
  TIMER_OVERLAY_POLICIES,
  timerOverlayMarkup,
} from "./timer-overlay";
import { timerOverlaySaveDurationSeconds, timerOverlayStateForDuration } from "./timer-overlay-actions";

function renderedChoice(markup: string, seconds: number): string {
  const match = markup.match(new RegExp(`<button[^>]*data-timer-choice-seconds="${seconds}"[^>]*>[^<]+</button>`, "u"));
  expect(match, `choice ${seconds} must be rendered`).not.toBeNull();
  return match![0];
}

describe("TASK 3304 show the real timer limit before choosing", () => {
  it("opens the existing Messenger timer at a 10-minute maximum and greys every longer choice", () => {
    const policy = TIMER_OVERLAY_POLICIES.Messenger;
    const markup = timerOverlayMarkup("Messenger");
    const enabled = TIMER_OVERLAY_CHOICES.filter((choice) => !renderedChoice(markup, choice.seconds).includes(" disabled"));
    const longer = TIMER_OVERLAY_CHOICES.filter((choice) => choice.seconds > policy.maxSeconds);
    const greyedLonger = longer.filter((choice) => {
      const rendered = renderedChoice(markup, choice.seconds);
      return rendered.includes(" disabled") && rendered.includes("timer-overlay-choice-disabled");
    });

    console.log(
      `TASK3304 app=Messenger max_label=${policy.maxLabel} max_seconds=${policy.maxSeconds} `
      + `enabled_max=${enabled.at(-1)?.label} longer_choices=${longer.length} greyed_longer=${greyedLonger.length}`,
    );

    expect(markup).toContain('role="dialog"');
    expect(markup).toContain('data-timer-app="Messenger"');
    expect(markup).toContain("supports timers up to <strong>10 minutes</strong>");
    expect(enabled.at(-1)?.label).toBe("10 minutes");
    expect(greyedLonger).toHaveLength(longer.length);
    const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
    expect(styles).toMatch(/\.timer-overlay-choice-disabled,[^}]+filter:\s*grayscale\(1\);[^}]+opacity:\s*\.42;/su);
    expect(timerOverlaySaveDurationSeconds(timerOverlayStateForDuration(10 * 60), "Messenger")).toEqual({ ok: true, durationSeconds: 600 });
    expect(timerOverlaySaveDurationSeconds(timerOverlayStateForDuration(60 * 60), "Messenger").ok).toBe(false);
  });

  it("opens that same Discord timer at 30 days with no greyed choice", () => {
    const policy = TIMER_OVERLAY_POLICIES.Discord;
    const markup = timerOverlayMarkup("Discord");
    const greyed = TIMER_OVERLAY_CHOICES.filter((choice) => renderedChoice(markup, choice.seconds).includes(" disabled"));

    console.log(
      `TASK3304 app=Discord max_label=${policy.maxLabel} max_seconds=${policy.maxSeconds} `
      + `choice_count=${TIMER_OVERLAY_CHOICES.length} greyed_choices=${greyed.length}`,
    );

    expect(markup).toContain('role="dialog"');
    expect(markup).toContain('data-timer-app="Discord"');
    expect(markup).toContain("supports timers up to <strong>30 days</strong>");
    expect(TIMER_OVERLAY_CHOICES.at(-1)?.label).toBe("30 days");
    expect(greyed).toHaveLength(0);
    expect(timerOverlaySaveDurationSeconds(timerOverlayStateForDuration(30 * 24 * 60 * 60), "Discord")).toEqual({
      ok: true,
      durationSeconds: 2_592_000,
    });
  });
});
