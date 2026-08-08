export const appearanceStorageKey = "osl-hub-appearance-v1";

export const accentChoices = ["cyan", "violet", "coral"] as const;
export const backgroundChoices = ["midnight", "slate", "paper"] as const;
export const avatarChoices = ["orbit", "wave", "spark"] as const;
export const windowPositionChoices = ["center", "remember"] as const;

export type AccentChoice = typeof accentChoices[number];
export type BackgroundChoice = typeof backgroundChoices[number];
export type AvatarChoice = typeof avatarChoices[number];
export type WindowPositionChoice = typeof windowPositionChoices[number];

export interface AppearancePreferences {
  accent: AccentChoice;
  background: BackgroundChoice;
  avatar: AvatarChoice;
  windowPosition: WindowPositionChoice;
  keepInTray: boolean;
  sounds: boolean;
}

export const defaultAppearancePreferences: AppearancePreferences = Object.freeze({
  accent: "cyan",
  background: "midnight",
  avatar: "orbit",
  windowPosition: "center",
  keepInTray: true,
  sounds: true,
});

type AppearanceStorage = Pick<Storage, "getItem" | "setItem">;

function isOneOf<T extends readonly string[]>(value: unknown, choices: T): value is T[number] {
  return typeof value === "string" && choices.includes(value);
}

/** Reject partial or foreign records so a reset has one unambiguous shape. */
export function parseAppearancePreferences(raw: string | null): AppearancePreferences {
  if (!raw) return { ...defaultAppearancePreferences };
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (Object.keys(value).sort().join(",") !== "accent,avatar,background,keepInTray,sounds,windowPosition"
      || !isOneOf(value.accent, accentChoices)
      || !isOneOf(value.background, backgroundChoices)
      || !isOneOf(value.avatar, avatarChoices)
      || !isOneOf(value.windowPosition, windowPositionChoices)
      || typeof value.keepInTray !== "boolean"
      || typeof value.sounds !== "boolean") return { ...defaultAppearancePreferences };
    return value as unknown as AppearancePreferences;
  } catch {
    return { ...defaultAppearancePreferences };
  }
}

export function loadAppearancePreferences(storage: Pick<Storage, "getItem">): AppearancePreferences {
  return parseAppearancePreferences(storage.getItem(appearanceStorageKey));
}

export function saveAppearancePreferences(storage: AppearanceStorage, preferences: AppearancePreferences): AppearancePreferences {
  const saved = parseAppearancePreferences(JSON.stringify(preferences));
  storage.setItem(appearanceStorageKey, JSON.stringify(saved));
  return saved;
}

export function resetAppearancePreferences(storage: AppearanceStorage): AppearancePreferences {
  return saveAppearancePreferences(storage, { ...defaultAppearancePreferences });
}

/**
 * Holds the unsaved Appearance screen state.  Preview changes only replace this
 * in-memory draft; the one storage write is deliberately owned by `save`.
 */
export interface AppearancePreferencesEditor {
  preview(next: AppearancePreferences): AppearancePreferences;
  cancel(): AppearancePreferences;
  save(storage: AppearanceStorage): AppearancePreferences;
  reset(storage: AppearanceStorage): AppearancePreferences;
}

export function createAppearancePreferencesEditor(initial: AppearancePreferences): AppearancePreferencesEditor {
  let saved = parseAppearancePreferences(JSON.stringify(initial));
  let draft = { ...saved };
  return {
    preview(next) {
      draft = parseAppearancePreferences(JSON.stringify(next));
      return { ...draft };
    },
    cancel() {
      draft = { ...saved };
      return { ...draft };
    },
    save(storage) {
      saved = saveAppearancePreferences(storage, draft);
      draft = { ...saved };
      return { ...saved };
    },
    reset(storage) {
      saved = resetAppearancePreferences(storage);
      draft = { ...saved };
      return { ...saved };
    },
  };
}

function label(value: string): string {
  return value[0].toUpperCase() + value.slice(1);
}

/** The preview is deliberately a message row, never a skeleton or placeholder. */
export function appearanceSettingsMarkup(preferences: AppearancePreferences): string {
  const choiceButtons = (field: "accent" | "background" | "avatar", choices: readonly string[]) => choices
    .map((choice) => `<button type="button" class="appearance-choice ${preferences[field] === choice ? "selected" : ""}" data-appearance-${field}="${choice}" aria-pressed="${preferences[field] === choice}">${field === "avatar" ? `<span class="appearance-avatar avatar-${choice}" aria-hidden="true">${choice === "orbit" ? "◉" : choice === "wave" ? "≈" : "✦"}</span>` : `<span class="appearance-swatch ${field}-${choice}" aria-hidden="true"></span>`}${label(choice)}</button>`)
    .join("");
  return `<section class="appearance-settings" data-appearance-settings data-accent="${preferences.accent}" data-background="${preferences.background}" data-avatar="${preferences.avatar}" aria-labelledby="appearance-title"><header><h2 id="appearance-title">Appearance</h2><p>Make this device feel like yours. Changes appear in the preview straight away.</p></header><div class="appearance-layout"><div class="appearance-controls"><fieldset><legend>Accent colour</legend><div class="appearance-choices">${choiceButtons("accent", accentChoices)}</div></fieldset><fieldset><legend>Background</legend><div class="appearance-choices">${choiceButtons("background", backgroundChoices)}</div></fieldset><fieldset><legend>Your avatar</legend><div class="appearance-choices">${choiceButtons("avatar", avatarChoices)}</div></fieldset><fieldset class="appearance-window-options"><legend>Window</legend><label>Position<select data-appearance-window-position><option value="center" ${preferences.windowPosition === "center" ? "selected" : ""}>Centre each time</option><option value="remember" ${preferences.windowPosition === "remember" ? "selected" : ""}>Remember last position</option></select></label><label><input type="checkbox" data-appearance-tray ${preferences.keepInTray ? "checked" : ""}/> Keep OSL in the tray when closed</label><label><input type="checkbox" data-appearance-sounds ${preferences.sounds ? "checked" : ""}/> Play message sounds</label></fieldset><p class="appearance-presets"><strong>Message presets</strong><span>Text size: Medium · Rounding: Soft · Spacing: Normal</span></p><div class="appearance-actions"><button class="button primary" type="button" data-save-appearance>Save changes</button><button class="button compact" type="button" data-cancel-appearance>Cancel</button><button class="button compact" type="button" data-reset-appearance>Reset Appearance</button></div></div><aside class="appearance-preview" aria-label="Live message preview" data-live-preview><small>LIVE PREVIEW</small><div class="preview-window"><header><span class="appearance-avatar avatar-${preferences.avatar}" aria-hidden="true">${preferences.avatar === "orbit" ? "◉" : preferences.avatar === "wave" ? "≈" : "✦"}</span><span><strong>You</strong><small>online</small></span></header><div class="preview-message-row"><span class="appearance-avatar avatar-${preferences.avatar}" aria-hidden="true">${preferences.avatar === "orbit" ? "◉" : preferences.avatar === "wave" ? "≈" : "✦"}</span><p><strong>You</strong><span>Let’s keep this conversation here.</span><time>Now</time></p></div></div></aside></div></section>`;
}
