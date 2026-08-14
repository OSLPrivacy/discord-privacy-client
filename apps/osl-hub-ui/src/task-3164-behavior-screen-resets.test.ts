import { describe, expect, it } from "vitest";
import {
  initialBehaviorScreenState,
  resetBehaviorSetting,
  behaviorScreenMarkup,
  BEHAVIOR_SETTINGS,
} from "./behavior-screen";

describe("Behaviour screen Reset buttons", () => {
  it("defines all six behavior settings", () => {
    expect(BEHAVIOR_SETTINGS).toEqual([
      "start_with_windows",
      "idle_lock_time_choice",
      "ask_before_irreversible_actions",
      "alert_mode_choice",
      "language",
      "follow_active_app_choice",
    ]);
  });

  it("renders all six settings in the screen markup", () => {
    const state = initialBehaviorScreenState();
    const markup = behaviorScreenMarkup(state);
    for (const setting of BEHAVIOR_SETTINGS) {
      expect(markup).toContain(`data-setting-id="${setting}"`);
    }
  });

  it("renders a Reset button for each setting", () => {
    const state = initialBehaviorScreenState();
    const markup = behaviorScreenMarkup(state);
    for (const setting of BEHAVIOR_SETTINGS) {
      expect(markup).toContain(`data-behavior-reset="${setting}"`);
    }
  });

  it("includes the Behaviour screen heading", () => {
    const state = initialBehaviorScreenState();
    const markup = behaviorScreenMarkup(state);
    expect(markup).toContain(">Behaviour</h1>");
  });

  it("has a Reset button for each of the six settings", () => {
    const state = initialBehaviorScreenState();
    const markup = behaviorScreenMarkup(state);
    const resetButtons = [...markup.matchAll(/data-behavior-reset="([^"]+)"/gu)];
    expect(resetButtons.length).toBe(6);
    const buttonSettings = resetButtons.map((m) => m[1]);
    expect(buttonSettings).toEqual(BEHAVIOR_SETTINGS);
  });

  it("resets only the targeted setting when Reset is pressed", () => {
    const start = initialBehaviorScreenState({
      start_with_windows: "on",
      idle_lock_time_choice: "seconds:1800",
      ask_before_irreversible_actions: "off",
      alert_mode_choice: "silent",
      language: "en",
      follow_active_app_choice: "on",
    });

    // Reset just one setting
    const afterReset = resetBehaviorSetting(start, "alert_mode_choice");

    // The reset setting should change to its default
    expect(afterReset.alert_mode_choice).toBe("normal");

    // All other settings should remain unchanged
    expect(afterReset.start_with_windows).toBe("on");
    expect(afterReset.idle_lock_time_choice).toBe("seconds:1800");
    expect(afterReset.ask_before_irreversible_actions).toBe("off");
    expect(afterReset.language).toBe("en");
    expect(afterReset.follow_active_app_choice).toBe("on");
  });

  it("includes default values for each setting", () => {
    const state = initialBehaviorScreenState();
    // Verify defaults match what the backend defines
    expect(state.start_with_windows).toBe("off");
    expect(state.idle_lock_time_choice).toBe("seconds:900:900 seconds");
    expect(state.ask_before_irreversible_actions).toBe("on");
    expect(state.alert_mode_choice).toBe("normal");
    expect(state.language).toBe("en");
    expect(state.follow_active_app_choice).toBe("off");
  });

  it("renders each setting value correctly", () => {
    const customState = initialBehaviorScreenState({
      start_with_windows: "on",
      idle_lock_time_choice: "seconds:300",
      ask_before_irreversible_actions: "off",
      alert_mode_choice: "silent",
      language: "de",
      follow_active_app_choice: "on",
    });

    const markup = behaviorScreenMarkup(customState);
    expect(markup).toContain("on"); // for start_with_windows
    expect(markup).toContain("seconds:300"); // for idle_lock_time_choice
    expect(markup).toContain("off"); // for ask_before_irreversible_actions
    expect(markup).toContain("silent"); // for alert_mode_choice
    expect(markup).toContain("de"); // for language
  });

  it("pressing one Reset changes only its own setting and leaves the other five unchanged", () => {
    // This is the core finish-line test
    const initial = initialBehaviorScreenState({
      start_with_windows: "on",
      idle_lock_time_choice: "seconds:1800",
      ask_before_irreversible_actions: "off",
      alert_mode_choice: "silent",
      language: "de",
      follow_active_app_choice: "on",
    });

    // Verify initial state
    expect(initial.start_with_windows).toBe("on");
    expect(initial.idle_lock_time_choice).toBe("seconds:1800");
    expect(initial.ask_before_irreversible_actions).toBe("off");
    expect(initial.alert_mode_choice).toBe("silent");
    expect(initial.language).toBe("de");
    expect(initial.follow_active_app_choice).toBe("on");

    // Reset start_with_windows
    const afterFirstReset = resetBehaviorSetting(initial, "start_with_windows");
    expect(afterFirstReset.start_with_windows).toBe("off"); // reset to default
    expect(afterFirstReset.idle_lock_time_choice).toBe("seconds:1800"); // unchanged
    expect(afterFirstReset.ask_before_irreversible_actions).toBe("off"); // unchanged
    expect(afterFirstReset.alert_mode_choice).toBe("silent"); // unchanged
    expect(afterFirstReset.language).toBe("de"); // unchanged
    expect(afterFirstReset.follow_active_app_choice).toBe("on"); // unchanged

    // Reset language
    const afterSecondReset = resetBehaviorSetting(afterFirstReset, "language");
    expect(afterSecondReset.start_with_windows).toBe("off"); // still reset
    expect(afterSecondReset.idle_lock_time_choice).toBe("seconds:1800"); // unchanged
    expect(afterSecondReset.ask_before_irreversible_actions).toBe("off"); // unchanged
    expect(afterSecondReset.alert_mode_choice).toBe("silent"); // unchanged
    expect(afterSecondReset.language).toBe("en"); // reset to default
    expect(afterSecondReset.follow_active_app_choice).toBe("on"); // unchanged
  });
});
