/**
 * The device-local Look preferences. These deliberately change only CSS
 * variables on OSL's own root element: a look never reaches a carrier, an
 * account, or another device.
 */
export type LookMode = "light" | "dark" | "computer";
export type NamedLook = "none" | "midnight" | "paper" | "signal";
export type LookAccent = "cyan" | "violet" | "amber";
export type LookCorners = "square" | "soft";
export type LookText = "comfortable" | "large";
export type LookSpacing = "compact" | "relaxed";

export interface LookState {
  readonly mode: LookMode;
  readonly named: NamedLook;
  readonly accent: LookAccent;
  readonly corners: LookCorners;
  readonly glow: boolean;
  readonly text: LookText;
  readonly spacing: LookSpacing;
}

type LookStorage = Pick<Storage, "getItem" | "setItem">;

export const lookStorageKey = "osl-hub-look-v1";

export const defaultLookState: LookState = {
  mode: "computer",
  named: "none",
  accent: "cyan",
  corners: "square",
  glow: false,
  text: "comfortable",
  spacing: "compact",
};

const modes: readonly LookMode[] = ["light", "dark", "computer"];
const namedLooks: readonly NamedLook[] = ["none", "midnight", "paper", "signal"];
const accents: readonly LookAccent[] = ["cyan", "violet", "amber"];
const corners: readonly LookCorners[] = ["square", "soft"];
const textSizes: readonly LookText[] = ["comfortable", "large"];
const spacings: readonly LookSpacing[] = ["compact", "relaxed"];

function has<T extends string>(items: readonly T[], value: unknown): value is T {
  return typeof value === "string" && items.includes(value as T);
}

/** Read a bounded, data-only preference; invalid or old data returns defaults. */
export function parseLookState(raw: string | null): LookState {
  if (raw === null || new TextEncoder().encode(raw).length > 2_048) return defaultLookState;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (!has(modes, value.mode) || !has(namedLooks, value.named) || !has(accents, value.accent)
      || !has(corners, value.corners) || typeof value.glow !== "boolean"
      || !has(textSizes, value.text) || !has(spacings, value.spacing)) return defaultLookState;
    return { mode: value.mode, named: value.named, accent: value.accent, corners: value.corners, glow: value.glow, text: value.text, spacing: value.spacing };
  } catch {
    return defaultLookState;
  }
}

export function loadLookState(storage: LookStorage): LookState {
  return parseLookState(storage.getItem(lookStorageKey));
}

export function saveLookState(storage: LookStorage, state: LookState): void {
  storage.setItem(lookStorageKey, JSON.stringify(state));
}

const namedLookVars: Record<NamedLook, Readonly<Record<string, string>>> = {
  none: {},
  midnight: { "--look-named-bg": "#0b1020", "--look-named-panel": "#121a30", "--look-named-text": "#ecf2ff", "--look-named-muted": "#a7b6d5" },
  paper: { "--look-named-bg": "#f4f0e8", "--look-named-panel": "#fffdf7", "--look-named-text": "#263239", "--look-named-muted": "#647177" },
  signal: { "--look-named-bg": "#111516", "--look-named-panel": "#182023", "--look-named-text": "#eef7f8", "--look-named-muted": "#a2b8bb" },
};

const accentVars: Record<LookAccent, string> = { cyan: "#06b6d4", violet: "#8b5cf6", amber: "#f59e0b" };

/** Apply only the small, owned set of Look variables and clear stale values. */
export function applyLookState(root: HTMLElement, state: LookState): void {
  for (const key of ["--look-named-bg", "--look-named-panel", "--look-named-text", "--look-named-muted"] as const) {
    const value = namedLookVars[state.named][key];
    if (value) root.style.setProperty(key, value);
    else root.style.removeProperty(key);
  }
  root.style.setProperty("--look-accent", accentVars[state.accent]);
  root.dataset.lookNamed = state.named;
  root.dataset.lookCorners = state.corners;
  root.dataset.lookGlow = String(state.glow);
  root.dataset.lookText = state.text;
  root.dataset.lookSpacing = state.spacing;
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

function choice(group: string, value: string, label: string, detail: string, selected: boolean, extra = ""): string {
  return `<button class="look-choice ${selected ? "selected" : ""} ${extra}" type="button" data-look-${group}="${escapeHtml(value)}" aria-pressed="${selected}"><strong>${escapeHtml(label)}</strong><small>${escapeHtml(detail)}</small></button>`;
}

/** The complete Look surface. Every control prints its explanation in one line. */
export function lookScreenMarkup(state: LookState): string {
  const modeChoices = [
    ["light", "Light", "A bright OSL window."],
    ["dark", "Dark", "A dark OSL window."],
    ["computer", "Computer", "Follow computer setting."],
  ] as const;
  const namedChoices = [
    ["midnight", "Midnight", "Deep blue for low-light work."],
    ["paper", "Paper", "Warm white, soft contrast."],
    ["signal", "Signal", "Cool charcoal, crisp accent."],
  ] as const;
  return `<section class="look-screen" aria-labelledby="look-title" data-look-screen>
    <header class="look-heading"><p class="eyebrow">Settings</p><h1 id="look-title">Look</h1><p>Change how OSL appears on this device. These choices never affect messages or other people.</p></header>
    <div class="look-layout">
      <section class="look-group look-mode-group" aria-labelledby="look-mode-title"><h3 id="look-mode-title">Theme</h3><p class="look-group-note">Choose the base brightness.</p><div class="look-choice-row">${modeChoices.map(([value, label, detail]) => choice("mode", value, label, detail, state.mode === value)).join("")}</div></section>
      <section class="look-group look-named-group" aria-labelledby="look-named-title"><h3 id="look-named-title">Named looks</h3><p class="look-group-note">Start with a ready-made palette.</p><div class="look-choice-row">${namedChoices.map(([value, label, detail]) => choice("named", value, label, detail, state.named === value, `look-named-${value}`)).join("")}</div></section>
      <section class="look-group" aria-labelledby="look-accent-title"><h3 id="look-accent-title">Accent</h3><p class="look-group-note">Use this color for selected controls.</p><div class="look-choice-row look-choice-row-small">${([ ["cyan", "Cyan", "Clear blue emphasis."], ["violet", "Violet", "Soft purple emphasis."], ["amber", "Amber", "Warm gold emphasis."] ] as const).map(([value, label, detail]) => choice("accent", value, label, detail, state.accent === value, `look-accent-${value}`)).join("")}</div></section>
      <section class="look-group look-pair-group" aria-labelledby="look-shape-title"><h3 id="look-shape-title">Corners and glow</h3><p class="look-group-note">Set the edge shape and highlight.</p><div class="look-pairs"><div class="look-inline-choices"><span>Corners</span>${choice("corners", "square", "Square", "Sharp edges.", state.corners === "square")}${choice("corners", "soft", "Soft", "Rounded edges.", state.corners === "soft")}</div><div class="look-inline-choices"><span>Glow</span>${choice("glow", "off", "Off", "No colored halo.", !state.glow)}${choice("glow", "on", "On", "Subtle accent halo.", state.glow)}</div></div></section>
      <section class="look-group look-pair-group" aria-labelledby="look-reading-title"><h3 id="look-reading-title">Window size</h3><p class="look-group-note">Choose how much fits in the OSL window.</p><div class="look-pairs"><div class="look-inline-choices"><span>Text</span>${choice("text", "comfortable", "Comfortable", "Standard size.", state.text === "comfortable")}${choice("text", "large", "Large", "Larger text.", state.text === "large")}</div><div class="look-inline-choices"><span>Spacing</span>${choice("spacing", "compact", "Compact", "Controls closer.", state.spacing === "compact")}${choice("spacing", "relaxed", "Relaxed", "More room.", state.spacing === "relaxed")}</div></div></section>
      <section class="look-reset"><div><strong>Reset look</strong><small>Return every look choice to OSL’s defaults.</small></div><button class="button" type="button" data-look-reset>Reset</button></section>
      <section class="look-save"><div><strong>Save these choices</strong><small>Keep this look on this device.</small></div><button class="button primary" type="button" data-look-save>Save</button></section>
    </div>
  </section>`;
}
