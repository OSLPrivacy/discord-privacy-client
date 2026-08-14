/**
 * The Behaviour screen: displays the six behaviour settings and their Reset buttons.
 *
 * The six settings are:
 * - start_with_windows (on/off)
 * - idle_lock_time_choice (with choice, seconds, and label)
 * - ask_before_irreversible_actions (on/off)
 * - alert_mode_choice (silent/quiet/normal)
 * - language (language code)
 * - follow_active_app_choice (on/off)
 *
 * Each setting displays its current value and has a Reset button that restores
 * only that setting to its starting value without affecting the others.
 */

export type BehaviourSettingId =
  | "start_with_windows"
  | "idle_lock_time_choice"
  | "ask_before_irreversible_actions"
  | "alert_mode_choice"
  | "language"
  | "follow_active_app_choice";

export const BEHAVIOUR_SETTING_IDS: readonly BehaviourSettingId[] = [
  "start_with_windows",
  "idle_lock_time_choice",
  "ask_before_irreversible_actions",
  "alert_mode_choice",
  "language",
  "follow_active_app_choice",
] as const;

export interface BehaviourSettingDisplay {
  readonly id: BehaviourSettingId;
  readonly label: string;
  readonly value: string;
  readonly displayValue: string;
}

export interface BehaviourScreenState {
  readonly settings: readonly BehaviourSettingDisplay[];
  readonly lastReset: BehaviourSettingId | null;
  readonly resetBusy: boolean;
}

export interface BehaviourScreenHandlers {
  readonly onResetSetting?: (settingId: BehaviourSettingId) => void;
  readonly onStateChanged?: (state: BehaviourScreenState) => void;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export function behaviourSettingMarkup(
  setting: BehaviourSettingDisplay,
): string {
  return `<div class="behaviour-setting" data-behaviour-setting="${escapeHtml(setting.id)}">
    <div class="behaviour-setting-content">
      <div class="behaviour-setting-info">
        <strong>${escapeHtml(setting.label)}</strong>
        <code class="behaviour-setting-value">${escapeHtml(setting.displayValue)}</code>
      </div>
      <button class="button compact" type="button" data-behaviour-reset="${escapeHtml(setting.id)}" aria-label="Reset ${escapeHtml(setting.label)}">Reset</button>
    </div>
  </div>`;
}

export function behaviourScreenMarkup(state: BehaviourScreenState): string {
  const settingsMarkup = state.settings
    .map((setting) => behaviourSettingMarkup(setting))
    .join("");

  return `<section class="behaviour-screen" data-behaviour-screen>
    <h2>Behaviour</h2>
    <div class="behaviour-settings">
      ${settingsMarkup}
    </div>
  </section>`;
}

export interface BehaviourScreenController {
  updateSetting(
    settingId: BehaviourSettingId,
    displayValue: string,
    value: string,
  ): void;
  unmount(): void;
}

export function attachBehaviourScreen(
  mount: HTMLElement,
  initialState: BehaviourScreenState,
  handlers: BehaviourScreenHandlers,
): BehaviourScreenController {
  let state = initialState;

  function draw(): void {
    const newMarkup = behaviourScreenMarkup(state);
    mount.innerHTML = newMarkup;
    bindResetButtons();
    handlers.onStateChanged?.(state);
  }

  function bindResetButtons(): void {
    const resetButtons = mount.querySelectorAll<HTMLButtonElement>(
      "[data-behaviour-reset]",
    );
    for (const button of resetButtons) {
      button.addEventListener("click", () => {
        const settingId = button.dataset.behaviourReset as BehaviourSettingId;
        if (BEHAVIOUR_SETTING_IDS.includes(settingId)) {
          state = { ...state, lastReset: settingId, resetBusy: true };
          draw();
          handlers.onResetSetting?.(settingId);
        }
      });
    }
  }

  function updateSetting(
    settingId: BehaviourSettingId,
    displayValue: string,
    value: string,
  ): void {
    const settingIndex = state.settings.findIndex((s) => s.id === settingId);
    if (settingIndex >= 0) {
      const updatedSettings = [...state.settings];
      updatedSettings[settingIndex] = {
        ...updatedSettings[settingIndex],
        value,
        displayValue,
      };
      state = {
        ...state,
        settings: updatedSettings,
        lastReset: null,
        resetBusy: false,
      };
      draw();
    }
  }

  function unmount(): void {
    mount.innerHTML = "";
  }

  draw();

  return {
    updateSetting,
    unmount,
  };
}
