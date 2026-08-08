export type WindowPosition = "center" | "last" | "top-left";

export type WindowSoundsSettings = {
  position: WindowPosition;
  rememberPlace: boolean;
  movement: boolean;
  trayPicture: boolean;
  sounds: boolean;
  muted: boolean;
  quietHours: boolean;
};

export const windowSoundsSettingsStorageKey = "osl-window-sounds-settings-v1";

export const defaultWindowSoundsSettings: WindowSoundsSettings = {
  position: "center",
  rememberPlace: true,
  movement: true,
  trayPicture: true,
  sounds: true,
  muted: false,
  quietHours: true,
};

const positions: readonly WindowPosition[] = ["center", "last", "top-left"];

export function parseWindowSoundsSettings(raw: unknown): WindowSoundsSettings {
  if (!raw || typeof raw !== "object") return { ...defaultWindowSoundsSettings };
  const candidate = raw as Partial<WindowSoundsSettings>;
  return {
    position: positions.includes(candidate.position as WindowPosition) ? candidate.position as WindowPosition : defaultWindowSoundsSettings.position,
    rememberPlace: typeof candidate.rememberPlace === "boolean" ? candidate.rememberPlace : defaultWindowSoundsSettings.rememberPlace,
    movement: typeof candidate.movement === "boolean" ? candidate.movement : defaultWindowSoundsSettings.movement,
    trayPicture: typeof candidate.trayPicture === "boolean" ? candidate.trayPicture : defaultWindowSoundsSettings.trayPicture,
    sounds: typeof candidate.sounds === "boolean" ? candidate.sounds : defaultWindowSoundsSettings.sounds,
    muted: typeof candidate.muted === "boolean" ? candidate.muted : defaultWindowSoundsSettings.muted,
    quietHours: typeof candidate.quietHours === "boolean" ? candidate.quietHours : defaultWindowSoundsSettings.quietHours,
  };
}

export function loadWindowSoundsSettings(storage: Pick<Storage, "getItem"> = localStorage): WindowSoundsSettings {
  try { return parseWindowSoundsSettings(JSON.parse(storage.getItem(windowSoundsSettingsStorageKey) ?? "null")); }
  catch { return { ...defaultWindowSoundsSettings }; }
}

export function saveWindowSoundsSettings(settings: WindowSoundsSettings, storage: Pick<Storage, "setItem"> = localStorage): void {
  storage.setItem(windowSoundsSettingsStorageKey, JSON.stringify(settings));
}

export function windowSoundsSettingsMarkup(settings: WindowSoundsSettings): string {
  const position = (value: WindowPosition, title: string, detail: string) => `<label class="window-position-choice"><input type="radio" name="window-position" value="${value}" ${settings.position === value ? "checked" : ""}/><span><strong>${title}</strong><small>${detail}</small></span></label>`;
  const toggle = (id: keyof Omit<WindowSoundsSettings, "position">, title: string, detail: string, checked: boolean, disabled = false) => `<label class="setting-line interactive" data-window-sound-setting="${id}"><span><strong>${title}</strong><small>${detail}</small></span><input id="window-sound-${id}" type="checkbox" ${checked ? "checked" : ""} ${disabled ? "disabled" : ""}/></label>`;
  const soundDisabled = !settings.sounds;
  const state = (on: boolean) => on ? "On" : "Off";
  return `<section class="window-sounds-settings" aria-labelledby="window-sounds-title" data-window-sounds-settings>
    <header><p class="eyebrow">This device</p><h2 id="window-sounds-title">Window &amp; sounds</h2><p>These choices stay on this device. They do not change a message, account, or another person’s alerts.</p></header>
    <fieldset class="window-position-options"><legend>Open OSL</legend>${position("center", "Centre of this screen", "Start in a predictable place.")}${position("last", "Last place", "Use the last saved location on this display.")}${position("top-left", "Top-left corner", "Keep the window tucked to a fixed edge.")}</fieldset>
    <div class="settings-list" aria-label="Window behaviour">
      ${toggle("rememberPlace", "Remember window place", "Save the location you choose when OSL closes.", settings.rememberPlace)}
      ${toggle("movement", "Allow window movement", "Let OSL move its own window when a protected view needs room.", settings.movement)}
      ${toggle("trayPicture", "Show picture in tray", "Show the OSL mark beside local tray alerts.", settings.trayPicture)}
    </div>
    <h3>Sound</h3><div class="settings-list" aria-label="Sound behaviour">
      ${toggle("sounds", "Play notification sounds", "Play a local sound for alerts that are allowed through.", settings.sounds)}
      ${toggle("muted", "Mute all OSL sounds", "Keep alerts visible while silencing OSL on this device.", settings.muted, soundDisabled)}
      ${toggle("quietHours", "Quiet hours · 22:00–07:00", "Silence OSL sounds overnight; visual alerts remain available.", settings.quietHours, soundDisabled || settings.muted)}
    </div>
    <footer class="window-sounds-footer"><span aria-live="polite">Saved · Position: ${settings.position === "last" ? "Last place" : settings.position === "top-left" ? "Top-left" : "Centre"} · Sounds: ${settings.muted ? "Muted" : state(settings.sounds)} · Quiet hours: ${state(settings.quietHours)}</span><button class="button compact" id="reset-window-sounds" type="button">Reset controls</button></footer>
  </section>`;
}
