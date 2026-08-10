/**
 * Local appearance colour preferences.  This module intentionally owns both
 * persistence and the small settings control so a caller can render it in any
 * Appearance surface and apply the chosen custom properties to its shell.
 */
export type AppearanceColourName = "accent" | "background";

export type AppearanceColours = Readonly<Record<AppearanceColourName, string>>;

export type AppearanceColourStorage = Pick<Storage, "getItem" | "setItem">;

export const appearanceAccentSwatches = ["#2ac0f0", "#7fdcf6", "#1899c9", "#35c46a"] as const;
export const appearanceBackgroundSwatches = ["#080c0d", "#0b0f14", "#0d0b12", "#0a0f0c", "#101014"] as const;

export const appearanceColourStorageKeys: Readonly<Record<AppearanceColourName, string>> = {
  accent: "osl-hub-appearance-accent",
  background: "osl-hub-appearance-background",
};

const defaults: AppearanceColours = {
  accent: appearanceAccentSwatches[0],
  background: appearanceBackgroundSwatches[0],
};

function normaliseHex(value: string | null): string | null {
  if (!value || !/^#[0-9a-f]{6}$/iu.test(value)) return null;
  return value.toLowerCase();
}

function colourFor(name: AppearanceColourName, storage: AppearanceColourStorage): string {
  return normaliseHex(storage.getItem(appearanceColourStorageKeys[name])) ?? defaults[name];
}

/** Reads the current persisted choices; safe to call again after an app restart. */
export function loadAppearanceColours(storage: AppearanceColourStorage): AppearanceColours {
  return { accent: colourFor("accent", storage), background: colourFor("background", storage) };
}

/** Saves a validated six-digit hex choice and returns whether it was accepted. */
export function saveAppearanceColour(
  name: AppearanceColourName,
  value: string,
  storage: AppearanceColourStorage,
): boolean {
  const colour = normaliseHex(value);
  if (!colour) return false;
  storage.setItem(appearanceColourStorageKeys[name], colour);
  return true;
}

/** Makes the preferences available to the rest of the interface as CSS variables. */
export function applyAppearanceColours(target: HTMLElement, colours: AppearanceColours): void {
  target.style.setProperty("--appearance-accent", colours.accent);
  target.style.setProperty("--appearance-background", colours.background);
}

function row(name: AppearanceColourName, swatches: readonly string[], selected: string): string {
  const label = name === "accent" ? "ACCENT" : "BACKGROUND";
  return `<section class="appearance-colour-row" data-appearance-colour-row="${name}" aria-labelledby="appearance-${name}-label">
    <div id="appearance-${name}-label" class="appearance-colour-label">${label}</div>
    <div class="appearance-colour-controls">
      <div class="appearance-colour-swatches" role="group" aria-label="${label} swatches">${swatches.map((colour) => `<button class="appearance-colour-swatch" type="button" data-appearance-colour-swatch="${name}" data-colour="${colour}" aria-label="Use ${colour}" aria-pressed="${selected === colour}" style="--swatch-colour:${colour}"></button>`).join("")}</div>
      <span class="appearance-colour-spacer" aria-hidden="true"></span>
      <input class="appearance-colour-hex" data-appearance-colour-hex="${name}" value="${selected}" aria-label="${label} hex colour" autocomplete="off" autocapitalize="off" spellcheck="false" inputmode="text" maxlength="7" pattern="#[0-9a-fA-F]{6}">
      <input class="appearance-colour-picker" data-appearance-colour-picker="${name}" type="color" value="${selected}" aria-label="Choose ${label.toLowerCase()} colour">
    </div>
  </section>`;
}

/** Markup only: screen placement and importing this module belong to the Appearance owner. */
export function appearanceColourRowsMarkup(colours: AppearanceColours): string {
  return `<div class="appearance-colours">${row("accent", appearanceAccentSwatches, colours.accent)}${row("background", appearanceBackgroundSwatches, colours.background)}</div>`;
}

function updateRow(root: ParentNode, name: AppearanceColourName, colour: string): void {
  root.querySelectorAll<HTMLInputElement>(`[data-appearance-colour-hex="${name}"]`).forEach((input) => { input.value = colour; });
  root.querySelectorAll<HTMLInputElement>(`[data-appearance-colour-picker="${name}"]`).forEach((input) => { input.value = colour; });
  root.querySelectorAll<HTMLButtonElement>(`[data-appearance-colour-swatch="${name}"]`).forEach((button) => {
    button.setAttribute("aria-pressed", String(button.dataset.colour === colour));
  });
}

/**
 * Connects swatches, the hex fields, and native colour pickers.  The callback
 * gives a screen owner the updated values if it needs to rerender surrounding UI.
 */
export function bindAppearanceColourRows(
  root: HTMLElement,
  storage: AppearanceColourStorage,
  onChange?: (colours: AppearanceColours) => void,
): void {
  let colours = loadAppearanceColours(storage);
  applyAppearanceColours(root.ownerDocument.documentElement, colours);

  const set = (name: AppearanceColourName, value: string): void => {
    if (!saveAppearanceColour(name, value, storage)) return;
    colours = { ...colours, [name]: normaliseHex(value)! };
    updateRow(root, name, colours[name]);
    applyAppearanceColours(root.ownerDocument.documentElement, colours);
    onChange?.(colours);
  };

  root.querySelectorAll<HTMLButtonElement>("[data-appearance-colour-swatch]").forEach((button) => button.addEventListener("click", () => {
    const name = button.dataset.appearanceColourSwatch as AppearanceColourName | undefined;
    if (name && button.dataset.colour) set(name, button.dataset.colour);
  }));
  root.querySelectorAll<HTMLInputElement>("[data-appearance-colour-hex]").forEach((input) => input.addEventListener("input", () => {
    const name = input.dataset.appearanceColourHex as AppearanceColourName | undefined;
    if (name) set(name, input.value);
  }));
  root.querySelectorAll<HTMLInputElement>("[data-appearance-colour-picker]").forEach((input) => input.addEventListener("input", () => {
    const name = input.dataset.appearanceColourPicker as AppearanceColourName | undefined;
    if (name) set(name, input.value);
  }));
}
