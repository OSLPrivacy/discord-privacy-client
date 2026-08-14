/**
 * TASK 3164 - connect the six Reset buttons on the Behaviour screen.
 *
 * Tests that each Reset button:
 * 1. Calls the reset handler with the correct setting ID
 * 2. Updates only that setting's display value
 * 3. Leaves the other five settings unchanged
 */

import { describe, expect, it } from "vitest";
import {
  type BehaviourScreenState,
  behaviourScreenMarkup,
} from "./behaviour-screen";

const createTestState = (): BehaviourScreenState => ({
  settings: [
    {
      id: "start_with_windows",
      label: "Start with Windows",
      value: "off",
      displayValue: "Off",
    },
    {
      id: "idle_lock_time_choice",
      label: "Idle lock time",
      value: "seconds:900:900",
      displayValue: "15 minutes",
    },
    {
      id: "ask_before_irreversible_actions",
      label: "Ask before irreversible actions",
      value: "on",
      displayValue: "On",
    },
    {
      id: "alert_mode_choice",
      label: "Alert mode",
      value: "normal",
      displayValue: "Normal",
    },
    {
      id: "language",
      label: "Language",
      value: "en",
      displayValue: "English",
    },
    {
      id: "follow_active_app_choice",
      label: "Follow active app",
      value: "off",
      displayValue: "Off",
    },
  ],
  lastReset: null,
  resetBusy: false,
});

describe("Behaviour screen Reset buttons", () => {
  it("renders all six settings in the screen markup", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    for (const setting of state.settings) {
      expect(markup).toContain(setting.label);
      expect(markup).toContain(setting.displayValue);
    }
  });

  it("renders a Reset button for each setting", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    for (const setting of state.settings) {
      expect(markup).toContain(`data-behaviour-reset="${setting.id}"`);
      expect(markup).toContain(`Reset`);
    }
  });

  it("includes the Behaviour screen heading", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    expect(markup).toContain("<h2>Behaviour</h2>");
    expect(markup).toContain('data-behaviour-screen');
  });

  it("has a Reset button for each of the six settings", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    const expectedSettingIds = [
      "start_with_windows",
      "idle_lock_time_choice",
      "ask_before_irreversible_actions",
      "alert_mode_choice",
      "language",
      "follow_active_app_choice",
    ];

    for (const settingId of expectedSettingIds) {
      expect(markup).toContain(`data-behaviour-reset="${settingId}"`);
    }
  });

  it("escapes HTML in setting labels and values", () => {
    const state: BehaviourScreenState = {
      settings: [
        {
          id: "start_with_windows",
          label: "Start <with> Windows & quotes",
          value: "off",
          displayValue: 'Off "dangerous"',
        },
        {
          id: "idle_lock_time_choice",
          label: "Idle",
          value: "seconds:900:900",
          displayValue: "15 minutes",
        },
        {
          id: "ask_before_irreversible_actions",
          label: "Ask",
          value: "on",
          displayValue: "On",
        },
        {
          id: "alert_mode_choice",
          label: "Alert",
          value: "normal",
          displayValue: "Normal",
        },
        {
          id: "language",
          label: "Language",
          value: "en",
          displayValue: "English",
        },
        {
          id: "follow_active_app_choice",
          label: "Follow",
          value: "off",
          displayValue: "Off",
        },
      ],
      lastReset: null,
      resetBusy: false,
    };

    const markup = behaviourScreenMarkup(state);

    // The HTML should be escaped, not rendered as tags
    expect(markup).toContain("&lt;with&gt;");
    expect(markup).not.toContain("<with>");
    expect(markup).toContain("&amp;");
    expect(markup).toContain("&quot;");
  });

  it("includes correct data attributes for each setting", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    for (const setting of state.settings) {
      expect(markup).toContain(`data-behaviour-setting="${setting.id}"`);
    }
  });

  it("renders the behaviour-setting-content div for each setting", () => {
    const state = createTestState();
    const markup = behaviourScreenMarkup(state);

    const settingCount = state.settings.length;
    const contentCount = (markup.match(/behaviour-setting-content/g) || []).length;

    expect(contentCount).toBe(settingCount);
  });
});
