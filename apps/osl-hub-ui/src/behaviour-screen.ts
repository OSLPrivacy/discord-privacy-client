import "./behaviour-screen.css";

/**
 * TASK 3166 - the Behaviour screen.
 *
 * Shows the six behaviour settings written by tasks 3145, 3148, 3151, 3154,
 * 3157 and 3160 (`start_with_windows`, `follow_active_app_choice`,
 * `idle_lock_time_choice`, `ask_before_irreversible_actions`,
 * `alert_mode_choice`, `language`), each with a Reset button per task 3164.
 *
 * The module is pure markup + state, like `message-defaults.ts`: the caller
 * loads the six saved values with each setting's own direct read command
 * (`cmd_osl_get_start_with_windows_choice`, `cmd_osl_read_idle_lock_time_choice`,
 * `cmd_osl_get_ask_before_irreversible_actions_choice`,
 * `cmd_osl_read_alert_mode_choice`, `cmd_osl_get_language_choice`,
 * `cmd_osl_get_follow_active_app_choice`) and passes them into
 * `initialBehaviourScreen`; the caller owns the IPC calls and the Back
 * navigation. Pressing a Reset button only ever changes its own setting.
 */

export type OnOffChoice = "on" | "off";
export type AlertMode = "silent" | "quiet" | "normal";

export interface IdleLockTimeChoice {
  choice: string;
  seconds: number | null;
  label: string;
}

export interface BehaviourSettings {
  startWithWindows: OnOffChoice;
  idleLockTime: IdleLockTimeChoice;
  askBeforeIrreversibleActions: OnOffChoice;
  alertMode: AlertMode;
  language: string;
  followActiveApp: OnOffChoice;
}

/** Mirrors each setting's Rust-side default, per `crates/ipc/tests/task3163_behavior_setting_resets.rs`. */
export const BUILT_IN_BEHAVIOUR_SETTINGS: BehaviourSettings = Object.freeze({
  startWithWindows: "off",
  idleLockTime: Object.freeze({ choice: "seconds", seconds: 900, label: "900 seconds" }),
  askBeforeIrreversibleActions: "on",
  alertMode: "normal",
  language: "en",
  followActiveApp: "off",
}) as BehaviourSettings;

export type BehaviourSettingId =
  | "startWithWindows"
  | "idleLockTime"
  | "askBeforeIrreversibleActions"
  | "alertMode"
  | "language"
  | "followActiveApp";

export const BEHAVIOUR_SETTING_IDS: readonly BehaviourSettingId[] = Object.freeze([
  "startWithWindows",
  "idleLockTime",
  "askBeforeIrreversibleActions",
  "alertMode",
  "language",
  "followActiveApp",
]);

const SETTING_TITLES: Record<BehaviourSettingId, string> = {
  startWithWindows: "Start with Windows",
  idleLockTime: "Idle lock time",
  askBeforeIrreversibleActions: "Ask before irreversible actions",
  alertMode: "Alert mode",
  language: "Language",
  followActiveApp: "Follow the active app",
};

const SETTING_EXPLANATIONS: Record<BehaviourSettingId, string> = {
  startWithWindows: "Opens OSL automatically when Windows starts.",
  idleLockTime: "How long OSL waits with no activity before it locks itself.",
  askBeforeIrreversibleActions: "Asks for confirmation before actions that cannot be undone, such as burns.",
  alertMode: "How loudly OSL alerts you: silent, quiet, or normal.",
  language: "The language OSL's screens are shown in.",
  followActiveApp: "Moves the OSL window to follow whichever app is in front.",
};

export interface BehaviourScreenState {
  /** What is on disk right now, per the six direct read commands. */
  saved: BehaviourSettings;
}

export function initialBehaviourScreen(
  saved: BehaviourSettings = BUILT_IN_BEHAVIOUR_SETTINGS,
): BehaviourScreenState {
  return { saved: cloneSettings(saved) };
}

function cloneSettings(settings: BehaviourSettings): BehaviourSettings {
  return { ...settings, idleLockTime: { ...settings.idleLockTime } };
}

/** Pure. Resetting one setting never touches the other five. */
export function resetBehaviourSetting(
  state: BehaviourScreenState,
  settingId: BehaviourSettingId,
): BehaviourScreenState {
  const next = cloneSettings(state.saved);
  if (settingId === "idleLockTime") {
    next.idleLockTime = { ...BUILT_IN_BEHAVIOUR_SETTINGS.idleLockTime };
  } else {
    (next[settingId] as unknown) = BUILT_IN_BEHAVIOUR_SETTINGS[settingId];
  }
  return { saved: next };
}

function onOffWords(choice: OnOffChoice): string {
  return choice === "on" ? "On" : "Off";
}

function alertModeWords(mode: AlertMode): string {
  return mode.charAt(0).toUpperCase() + mode.slice(1);
}

function idleLockTimeWords(choice: IdleLockTimeChoice): string {
  return choice.choice === "never" ? "Never" : choice.label;
}

function languageWords(language: string): string {
  const names: Record<string, string> = { en: "English", es: "Espanol" };
  return names[language] ?? language;
}

/**
 * The exact display value for one setting. This is the same string the
 * screen renders next to that setting's title, and it must equal what the
 * setting's own direct read command returns (after formatting).
 */
export function displayValue(settings: BehaviourSettings, settingId: BehaviourSettingId): string {
  switch (settingId) {
    case "startWithWindows":
      return onOffWords(settings.startWithWindows);
    case "idleLockTime":
      return idleLockTimeWords(settings.idleLockTime);
    case "askBeforeIrreversibleActions":
      return onOffWords(settings.askBeforeIrreversibleActions);
    case "alertMode":
      return alertModeWords(settings.alertMode);
    case "language":
      return languageWords(settings.language);
    case "followActiveApp":
      return onOffWords(settings.followActiveApp);
    default:
      return "";
  }
}

export type BehaviourScreenEvent =
  | { kind: "reset"; settingId: BehaviourSettingId }
  | { kind: "back" };

/**
 * Maps a clicked control (`data-behaviour-action` / `data-behaviour-setting`)
 * to an event, so the wiring layer stays one listener.
 */
export function behaviourEventForAction(
  action: string | undefined | null,
  settingId: string | undefined | null,
): BehaviourScreenEvent | null {
  if (action === "back") return { kind: "back" };
  if (
    action === "reset" &&
    settingId &&
    (BEHAVIOUR_SETTING_IDS as readonly string[]).includes(settingId)
  ) {
    return { kind: "reset", settingId: settingId as BehaviourSettingId };
  }
  return null;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

function settingRow(settings: BehaviourSettings, settingId: BehaviourSettingId): string {
  const title = escapeHtml(SETTING_TITLES[settingId]);
  const explanation = escapeHtml(SETTING_EXPLANATIONS[settingId]);
  const value = escapeHtml(displayValue(settings, settingId));
  return `<div class="behaviour-row" data-setting-id="${settingId}">
    <div class="behaviour-row-text">
      <span class="behaviour-row-title">${title}</span>
      <small class="behaviour-row-explain">${explanation}</small>
    </div>
    <strong class="behaviour-row-value" data-behaviour-value="${settingId}">${value}</strong>
    <button class="button ghost behaviour-reset" type="button" data-behaviour-action="reset" data-behaviour-setting="${settingId}">Reset</button>
  </div>`;
}

export function behaviourScreenMarkup(state: BehaviourScreenState): string {
  const rows = BEHAVIOUR_SETTING_IDS.map((id) => settingRow(state.saved, id)).join("");
  return `<section class="behaviour-screen" aria-labelledby="behaviour-heading">
    <h1 id="behaviour-heading" tabindex="-1" class="behaviour-title">Behaviour</h1>
    <p class="behaviour-intro">These six settings control how OSL behaves. Each has its own Reset button, which puts only that setting back to its built-in value.</p>
    <div class="behaviour-rows">
      ${rows}
    </div>
    <div class="behaviour-actions">
      <button class="button ghost" type="button" id="behaviour-back" data-behaviour-action="back">Back</button>
    </div>
  </section>`;
}
