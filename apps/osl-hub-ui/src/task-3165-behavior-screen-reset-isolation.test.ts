import { describe, expect, it } from "vitest";
import {
  BEHAVIOR_SETTINGS,
  BehaviorScreenState,
  initialBehaviorScreenState,
  resetBehaviorSetting,
} from "./behavior-screen";

function countAwayFromStart(
  starting: BehaviorScreenState,
  current: BehaviorScreenState,
): number {
  return BEHAVIOR_SETTINGS.filter((setting) => current[setting] !== starting[setting]).length;
}

describe("Behaviour screen Reset isolation (each Reset returns only its own setting)", () => {
  it("count away from start goes 6,5,4,3,2,1,0 and no reset touches another setting", () => {
    const starting = initialBehaviorScreenState();

    // Change all six settings away from their starting (default) values.
    const changed = initialBehaviorScreenState({
      start_with_windows: "on",
      idle_lock_time_choice: "seconds:60:60 seconds",
      ask_before_irreversible_actions: "off",
      alert_mode_choice: "silent",
      language: "fr",
      follow_active_app_choice: "on",
    });

    for (const setting of BEHAVIOR_SETTINGS) {
      expect(changed[setting]).not.toBe(starting[setting]);
    }
    expect(countAwayFromStart(starting, changed)).toBe(6);

    const counts: number[] = [countAwayFromStart(starting, changed)];
    let current = changed;

    for (const setting of BEHAVIOR_SETTINGS) {
      const before = current;
      current = resetBehaviorSetting(current, setting);

      // The setting just reset must now match its starting value.
      expect(current[setting]).toBe(starting[setting]);

      // No setting other than the one just reset changed from the previous step.
      for (const other of BEHAVIOR_SETTINGS) {
        if (other === setting) continue;
        expect(current[other]).toBe(before[other]);
      }

      counts.push(countAwayFromStart(starting, current));
    }

    expect(counts).toEqual([6, 5, 4, 3, 2, 1, 0]);
  });
});
