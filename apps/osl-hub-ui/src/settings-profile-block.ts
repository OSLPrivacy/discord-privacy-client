import {
  OslProfilePaneState,
  type ScopedProfileRecord,
} from "./osl-profile-pane";
import "./settings-profile-block.css";

/** The Appearance page's compact editor deliberately uses the global OSL record only. */
export const SETTINGS_PROFILE_STORAGE_KEY = "osl-settings-profile-v1";
export const SETTINGS_PROFILE_VIBE_LIMIT = 40;
export const SETTINGS_PROFILE_AVATAR_COLOURS = ["#2ac0f0", "#35c46a", "#b48cf2", "#f2a35c"] as const;
export const SETTINGS_PROFILE_BACKGROUNDS = [
  "#0d1114",
  "linear-gradient(160deg, #0e2a36 0%, #0d1114 65%)",
  "linear-gradient(160deg, #1d1533 0%, #0d1114 65%)",
  "linear-gradient(160deg, #0e2b1d 0%, #0d1114 65%)",
  "linear-gradient(160deg, #33121a 0%, #0d1114 65%)",
] as const;

export interface SettingsProfileStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface SettingsProfileBlockOptions {
  storage?: SettingsProfileStorage;
  /** Called after the shared global record has been changed and persisted. */
  onProfileChange?: (record: ScopedProfileRecord) => void;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

function cloneGlobal(record: ScopedProfileRecord): ScopedProfileRecord {
  return { ...record, scope: { kind: "global" } };
}

function globalRecord(state: OslProfilePaneState): ScopedProfileRecord {
  const record = state.record("global");
  if (!record || record.scope.kind !== "global") throw new Error("Settings profile needs the global OSL profile record.");
  return record;
}

function defaultStorage(): SettingsProfileStorage | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

function isSavedGlobalProfile(value: unknown): value is Omit<ScopedProfileRecord, "scope"> {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return typeof record.displayName === "string"
    && typeof record.aboutLine === "string"
    && typeof record.status === "string"
    && typeof record.cardBackground === "string"
    && (typeof record.avatar === "string" || record.avatar === null)
    && typeof record.colour === "string";
}

/** Reads a previous Appearance edit. Invalid storage is ignored rather than becoming profile data. */
export function loadSettingsProfile(storage: Pick<SettingsProfileStorage, "getItem"> | undefined = defaultStorage()): Omit<ScopedProfileRecord, "scope"> | null {
  if (!storage) return null;
  try {
    const saved: unknown = JSON.parse(storage.getItem(SETTINGS_PROFILE_STORAGE_KEY) ?? "null");
    return isSavedGlobalProfile(saved) ? saved : null;
  } catch {
    return null;
  }
}

/** Writes exactly the global record, making the block survive a page/app restart. */
export function saveSettingsProfile(record: ScopedProfileRecord, storage: Pick<SettingsProfileStorage, "setItem"> | undefined = defaultStorage()): void {
  if (!storage || record.scope.kind !== "global") return;
  const { scope: _scope, ...saved } = record;
  storage.setItem(SETTINGS_PROFILE_STORAGE_KEY, JSON.stringify(saved));
}

/** Hydrates the global profile record before rendering an Appearance restart. */
export function restoreSettingsProfile(state: OslProfilePaneState, storage: Pick<SettingsProfileStorage, "getItem"> | undefined = defaultStorage()): boolean {
  const saved = loadSettingsProfile(storage);
  if (!saved) return false;
  state.setField("global", "displayName", saved.displayName);
  state.setField("global", "aboutLine", saved.aboutLine);
  state.setField("global", "status", saved.status);
  state.setField("global", "cardBackground", saved.cardBackground);
  state.setField("global", "colour", saved.colour);
  if (saved.avatar === null) state.removeAvatar("global");
  else state.uploadAvatar("global", saved.avatar);
  return true;
}

export function settingsProfileVibeLeft(vibe: string): number {
  return Math.max(0, SETTINGS_PROFILE_VIBE_LIMIT - Array.from(vibe).length);
}

function checked(value: boolean): string { return value ? " checked" : ""; }

function avatarPreview(record: ScopedProfileRecord): string {
  if (record.avatar) return `<img src="${escapeHtml(record.avatar)}" alt="${escapeHtml(record.displayName)} avatar"/>`;
  return `<span aria-hidden="true">${escapeHtml(Array.from(record.displayName.trim())[0]?.toUpperCase() ?? "?")}</span>`;
}

/** Renders the YOUR PROFILE group; it intentionally does not include friends-visible accounts. */
export function settingsProfileBlockMarkup(record: ScopedProfileRecord): string {
  if (record.scope.kind !== "global") throw new Error("Settings profile markup only accepts the global OSL profile record.");
  const vibe = Array.from(record.status).slice(0, SETTINGS_PROFILE_VIBE_LIMIT).join("");
  return [
    `<section class="settings-profile-block" aria-labelledby="settings-profile-heading">`,
    `<div class="settings-profile-block-heading"><h3 id="settings-profile-heading">YOUR PROFILE</h3><p>How you appear on a friend's card.</p></div>`,
    `<label class="settings-profile-field"><span>Display name</span><input type="text" maxlength="64" value="${escapeHtml(record.displayName)}" data-settings-profile-display-name/></label>`,
    `<label class="settings-profile-field"><span class="settings-profile-label-line">Vibe <small data-settings-profile-vibe-count>${settingsProfileVibeLeft(vibe)} left</small></span><input type="text" maxlength="40" placeholder="what's your vibe right now" value="${escapeHtml(vibe)}" data-settings-profile-vibe/></label>`,
    `<fieldset class="settings-profile-choice-group"><legend>Avatar colour</legend><div class="settings-profile-colours">${SETTINGS_PROFILE_AVATAR_COLOURS.map((colour) => `<label class="settings-profile-colour" title="Avatar colour ${colour}"><input type="radio" name="settings-profile-colour" value="${colour}" data-settings-profile-colour aria-label="Avatar colour ${colour}"${checked(record.colour.toLowerCase() === colour)}/><span style="--settings-profile-swatch:${colour}" aria-hidden="true"></span></label>`).join("")}</div></fieldset>`,
    `<div class="settings-profile-avatar"><div class="settings-profile-avatar-preview" style="--settings-profile-avatar-colour:${escapeHtml(record.colour)}">${avatarPreview(record)}</div><div><strong>Avatar image</strong><div class="settings-profile-avatar-actions"><label class="settings-profile-upload">Upload<input type="file" accept="image/png,image/jpeg,image/gif,.png,.jpg,.jpeg,.gif" data-settings-profile-avatar-upload/></label><button type="button" data-settings-profile-avatar-remove${record.avatar ? "" : " disabled"}>Remove</button><small>PNG, JPG, or GIF</small></div></div></div>`,
    `<fieldset class="settings-profile-choice-group"><legend>Profile background</legend><div class="settings-profile-backgrounds">${SETTINGS_PROFILE_BACKGROUNDS.map((background, index) => `<label class="settings-profile-background" title="Profile background ${index + 1}"><input type="radio" name="settings-profile-background" value="${escapeHtml(background)}" data-settings-profile-background aria-label="Profile background ${index + 1}"${checked(record.cardBackground === background)}/><span style="--settings-profile-background:${escapeHtml(background)}" aria-hidden="true"></span></label>`).join("")}</div><label class="settings-profile-custom-hex"><span>Custom hex</span><input type="text" inputmode="text" maxlength="7" autocomplete="off" spellcheck="false" placeholder="#0d1114" value="${/^#[0-9a-f]{6}$/iu.test(record.cardBackground) ? escapeHtml(record.cardBackground) : ""}" data-settings-profile-background-hex/></label></fieldset>`,
    `</section>`,
  ].join("");
}

function avatarDataUrl(file: File): Promise<string> {
  if (!["image/png", "image/jpeg", "image/gif"].includes(file.type)) {
    return Promise.reject(new Error("Avatar image must be PNG, JPG, or GIF."));
  }
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.addEventListener("load", () => typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("The avatar could not be read.")));
    reader.addEventListener("error", () => reject(new Error("The avatar could not be read.")));
    reader.readAsDataURL(file);
  });
}

/** Mounts the profile group and keeps the shared global record and local restart storage in sync. */
export function attachSettingsProfileBlock(mount: HTMLElement, state: OslProfilePaneState, options: SettingsProfileBlockOptions = {}): () => void {
  const storage = options.storage ?? defaultStorage();
  restoreSettingsProfile(state, storage);
  let active = true;
  const persist = (): void => {
    const record = cloneGlobal(globalRecord(state));
    saveSettingsProfile(record, storage);
    options.onProfileChange?.(record);
  };
  const draw = (): void => { if (active) mount.innerHTML = settingsProfileBlockMarkup(globalRecord(state)); };
  const input = (event: Event): void => {
    const target = event.target as HTMLInputElement | null;
    if (!target) return;
    if (target.matches("[data-settings-profile-display-name]")) state.setField("global", "displayName", target.value);
    else if (target.matches("[data-settings-profile-vibe]")) {
      state.setField("global", "status", Array.from(target.value).slice(0, SETTINGS_PROFILE_VIBE_LIMIT).join(""));
      const counter = mount.querySelector<HTMLElement>("[data-settings-profile-vibe-count]");
      if (counter) counter.textContent = `${settingsProfileVibeLeft(target.value)} left`;
    } else return;
    persist();
  };
  const change = (event: Event): void => {
    const target = event.target as HTMLInputElement | null;
    if (!target) return;
    if (target.matches("[data-settings-profile-colour]")) state.setField("global", "colour", target.value);
    else if (target.matches("[data-settings-profile-background]")) state.setField("global", "cardBackground", target.value);
    else if (target.matches("[data-settings-profile-background-hex]")) {
      if (!/^#[0-9a-f]{6}$/iu.test(target.value)) return;
      state.setField("global", "cardBackground", target.value.toLowerCase());
    } else if (target.matches("[data-settings-profile-avatar-upload]")) {
      const file = target.files?.[0];
      if (!file) return;
      void avatarDataUrl(file).then((avatar) => {
        if (!active) return;
        state.uploadAvatar("global", avatar);
        persist();
        draw();
      }).catch(() => { target.value = ""; });
      return;
    } else return;
    persist();
    draw();
  };
  const click = (event: Event): void => {
    if (!(event.target as HTMLElement | null)?.closest("[data-settings-profile-avatar-remove]")) return;
    state.removeAvatar("global");
    persist();
    draw();
  };
  mount.addEventListener("input", input);
  mount.addEventListener("change", change);
  mount.addEventListener("click", click);
  draw();
  return () => {
    active = false;
    mount.removeEventListener("input", input);
    mount.removeEventListener("change", change);
    mount.removeEventListener("click", click);
  };
}
