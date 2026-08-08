import { describe, expect, it } from "vitest";
import {
  BEHAVIOUR_SETTING_IDS,
  BUILT_IN_BEHAVIOUR_SETTINGS,
  behaviourEventForAction,
  behaviourScreenMarkup,
  displayValue,
  initialBehaviourScreen,
  resetBehaviourSetting,
} from "./behaviour-screen";

/**
 * The saved/read pairs printed by
 * `cargo test -p ipc --test task3166_behaviour_screen_direct_reads` for each
 * of the six direct read commands, after saving a non-default value through
 * each setting's own direct save/set command and reading it back from a
 * fresh AppState loaded from the saved preferences file:
 *
 *   TASK3166 start_with_windows.read=on
 *   TASK3166 idle_lock_time_choice.read=never:none:never
 *   TASK3166 ask_before_irreversible_actions.read=off
 *   TASK3166 alert_mode_choice.read=quiet
 *   TASK3166 language.read=es
 *   TASK3166 follow_active_app_choice.read=on
 *
 * These are used verbatim below so the screen's rendered values are proven
 * to match what the direct read commands return, not just plausible.
 */
const DIRECT_READ_SETTINGS = {
  startWithWindows: "on" as const,
  idleLockTime: { choice: "never", seconds: null, label: "never" },
  askBeforeIrreversibleActions: "off" as const,
  alertMode: "quiet" as const,
  language: "es",
  followActiveApp: "on" as const,
};

describe("TASK 3166 Behaviour screen", () => {
  it("starts from the same six built-in defaults as the Rust reset commands (task 3163/3164)", () => {
    expect(BUILT_IN_BEHAVIOUR_SETTINGS).toEqual({
      startWithWindows: "off",
      idleLockTime: { choice: "seconds", seconds: 900, label: "900 seconds" },
      askBeforeIrreversibleActions: "on",
      alertMode: "normal",
      language: "en",
      followActiveApp: "off",
    });
  });

  it("lists exactly the six settings from tasks 3145, 3148, 3151, 3154, 3157, 3160", () => {
    expect(BEHAVIOUR_SETTING_IDS).toEqual([
      "startWithWindows",
      "idleLockTime",
      "askBeforeIrreversibleActions",
      "alertMode",
      "language",
      "followActiveApp",
    ]);
  });

  it("shows all six settings in the screen markup", () => {
    const state = initialBehaviourScreen();
    const markup = behaviourScreenMarkup(state);
    for (const id of BEHAVIOUR_SETTING_IDS) {
      expect(markup).toContain(`data-setting-id="${id}"`);
      expect(markup).toContain(`data-behaviour-value="${id}"`);
    }
  });

  it("renders a Reset button for each of the six settings", () => {
    const state = initialBehaviourScreen();
    const markup = behaviourScreenMarkup(state);
    for (const id of BEHAVIOUR_SETTING_IDS) {
      expect(markup).toContain(`data-behaviour-action="reset" data-behaviour-setting="${id}"`);
    }
  });

  it("has a Back control", () => {
    const markup = behaviourScreenMarkup(initialBehaviourScreen());
    expect(markup).toContain('data-behaviour-action="back"');
    expect(markup).toContain(">Back<");
  });

  it("renders exactly what the direct read commands returned, for every setting", () => {
    const state = initialBehaviourScreen(DIRECT_READ_SETTINGS);
    const markup = behaviourScreenMarkup(state);

    expect(displayValue(state.saved, "startWithWindows")).toBe("On");
    expect(displayValue(state.saved, "idleLockTime")).toBe("Never");
    expect(displayValue(state.saved, "askBeforeIrreversibleActions")).toBe("Off");
    expect(displayValue(state.saved, "alertMode")).toBe("Quiet");
    expect(displayValue(state.saved, "language")).toBe("Espanol");
    expect(displayValue(state.saved, "followActiveApp")).toBe("On");

    expect(markup).toContain('data-behaviour-value="startWithWindows">On<');
    expect(markup).toContain('data-behaviour-value="idleLockTime">Never<');
    expect(markup).toContain('data-behaviour-value="askBeforeIrreversibleActions">Off<');
    expect(markup).toContain('data-behaviour-value="alertMode">Quiet<');
    expect(markup).toContain('data-behaviour-value="language">Espanol<');
    expect(markup).toContain('data-behaviour-value="followActiveApp">On<');
  });

  it("shows the built-in idle lock time label (900 seconds) exactly as the read command's label field", () => {
    const state = initialBehaviourScreen();
    expect(displayValue(state.saved, "idleLockTime")).toBe("900 seconds");
  });

  it("resets only the targeted setting when Reset is pressed, leaving the other five unchanged", () => {
    const state = initialBehaviourScreen(DIRECT_READ_SETTINGS);
    const afterReset = resetBehaviourSetting(state, "startWithWindows");

    expect(afterReset.saved.startWithWindows).toBe(BUILT_IN_BEHAVIOUR_SETTINGS.startWithWindows);
    expect(afterReset.saved.idleLockTime).toEqual(DIRECT_READ_SETTINGS.idleLockTime);
    expect(afterReset.saved.askBeforeIrreversibleActions).toBe(
      DIRECT_READ_SETTINGS.askBeforeIrreversibleActions,
    );
    expect(afterReset.saved.alertMode).toBe(DIRECT_READ_SETTINGS.alertMode);
    expect(afterReset.saved.language).toBe(DIRECT_READ_SETTINGS.language);
    expect(afterReset.saved.followActiveApp).toBe(DIRECT_READ_SETTINGS.followActiveApp);
  });

  it("pressing each Reset in turn changes only its own setting each time (finish-line proof)", () => {
    let state = initialBehaviourScreen(DIRECT_READ_SETTINGS);
    for (const id of BEHAVIOUR_SETTING_IDS) {
      const before = state;
      state = resetBehaviourSetting(state, id);
      for (const other of BEHAVIOUR_SETTING_IDS) {
        if (other === id) continue;
        expect(state.saved[other]).toEqual(before.saved[other]);
      }
      expect(state.saved[id]).toEqual(BUILT_IN_BEHAVIOUR_SETTINGS[id]);
    }
    expect(state.saved).toEqual(BUILT_IN_BEHAVIOUR_SETTINGS);
  });

  it("maps clicked controls to events and everything else to null", () => {
    expect(behaviourEventForAction("back", null)).toEqual({ kind: "back" });
    expect(behaviourEventForAction("reset", "language")).toEqual({
      kind: "reset",
      settingId: "language",
    });
    expect(behaviourEventForAction("reset", "not-a-setting")).toBeNull();
    expect(behaviourEventForAction(null, null)).toBeNull();
  });
});
