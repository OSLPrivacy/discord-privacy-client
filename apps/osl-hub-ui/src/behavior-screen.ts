/**
 * The Behaviour screen displays six application preferences, each with its own Reset button.
 * When a Reset button is pressed, only that one setting reverts to its default value.
 * The six settings are:
 * - start_with_windows
 * - idle_lock_time_choice
 * - ask_before_irreversible_actions
 * - alert_mode_choice
 * - language
 * - follow_active_app_choice
 */

export const BEHAVIOR_SETTINGS = [
  "start_with_windows",
  "idle_lock_time_choice",
  "ask_before_irreversible_actions",
  "alert_mode_choice",
  "language",
  "follow_active_app_choice",
] as const;

export type BehaviorSetting = typeof BEHAVIOR_SETTINGS[number];

/**
 * Defaults for each behavior setting, matching the backend definitions
 * (crates/ipc/src/app_preferences.rs).
 */
const DEFAULTS: Record<BehaviorSetting, string> = {
  start_with_windows: "off",
  idle_lock_time_choice: "seconds:900:900 seconds",
  ask_before_irreversible_actions: "on",
  alert_mode_choice: "normal",
  language: "en",
  follow_active_app_choice: "off",
};

export interface BehaviorScreenState {
  start_with_windows: string;
  idle_lock_time_choice: string;
  ask_before_irreversible_actions: string;
  alert_mode_choice: string;
  language: string;
  follow_active_app_choice: string;
}

export function initialBehaviorScreenState(overrides?: Partial<BehaviorScreenState>): BehaviorScreenState {
  return {
    start_with_windows: overrides?.start_with_windows ?? DEFAULTS.start_with_windows,
    idle_lock_time_choice: overrides?.idle_lock_time_choice ?? DEFAULTS.idle_lock_time_choice,
    ask_before_irreversible_actions: overrides?.ask_before_irreversible_actions ?? DEFAULTS.ask_before_irreversible_actions,
    alert_mode_choice: overrides?.alert_mode_choice ?? DEFAULTS.alert_mode_choice,
    language: overrides?.language ?? DEFAULTS.language,
    follow_active_app_choice: overrides?.follow_active_app_choice ?? DEFAULTS.follow_active_app_choice,
  };
}

/**
 * Reset one behavior setting to its default value.
 * All other settings remain unchanged.
 */
export function resetBehaviorSetting(
  state: BehaviorScreenState,
  setting: BehaviorSetting,
): BehaviorScreenState {
  return {
    ...state,
    [setting]: DEFAULTS[setting],
  };
}

const SETTING_LABELS: Record<BehaviorSetting, string> = {
  start_with_windows: "Start with Windows",
  idle_lock_time_choice: "Idle lock time",
  ask_before_irreversible_actions: "Ask before irreversible actions",
  alert_mode_choice: "Alert mode",
  language: "Language",
  follow_active_app_choice: "Follow active app",
};

function escapeHtml(text: string): string {
  const div = typeof document !== "undefined" ? document.createElement("div") : null;
  if (div) {
    div.textContent = text;
    return div.innerHTML;
  }
  return text
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

export function behaviorScreenMarkup(state: BehaviorScreenState): string {
  const settingRows = BEHAVIOR_SETTINGS.map((settingId) => {
    const label = SETTING_LABELS[settingId];
    const value = state[settingId];
    return `<div class="behavior-setting-row" data-setting-id="${escapeHtml(settingId)}">
      <div class="behavior-setting-content">
        <strong>${escapeHtml(label)}</strong>
        <span class="behavior-setting-value">${escapeHtml(value)}</span>
      </div>
      <button class="button compact" data-behavior-reset="${escapeHtml(settingId)}" type="button">Reset</button>
    </div>`;
  }).join("");

  return `<section class="behavior-screen" aria-labelledby="behavior-title">
    <h1 id="behavior-title" tabindex="-1" class="behavior-title">Behaviour</h1>
    <p class="behavior-lead">Each setting has its own Reset button. Pressing Reset returns only that one setting to its default value.</p>
    <div class="behavior-settings">
      ${settingRows}
    </div>
  </section>`;
}

export interface BehaviorScreenHandle {
  state(): BehaviorScreenState;
}

export interface MountBehaviorScreenOptions {
  state?: BehaviorScreenState;
  onResetSetting?: (setting: BehaviorSetting) => void;
}

/**
 * Mounts the behavior screen with event handling.
 * Redraws on every change; diffing would cost more than redrawing a simple grid.
 */
export function mountBehaviorScreen(
  root: HTMLElement,
  { state = initialBehaviorScreenState(), onResetSetting }: MountBehaviorScreenOptions = {},
): BehaviorScreenHandle {
  let current = state;

  const draw = (): void => {
    root.innerHTML = behaviorScreenMarkup(current);
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target) return;
    const settingId = target.dataset.behaviorReset as BehaviorSetting | undefined;
    if (!settingId || !BEHAVIOR_SETTINGS.includes(settingId)) return;
    current = resetBehaviorSetting(current, settingId);
    onResetSetting?.(settingId);
    draw();
  });

  draw();

  return {
    state: () => current,
  };
}
