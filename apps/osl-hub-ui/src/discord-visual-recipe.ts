export const DISCORD_VISUAL_RECIPE_VERSION = "osl-discord-visual-recipe-v1" as const;

export type DiscordVisualThemePack = "dark" | "light" | "midnight" | "ash";
export type DiscordVisualDensity = "cozy" | "compact";

/**
 * Content-free presentation measurements only. This contract cannot carry
 * message text, account identity, selectors, paths, tokens, or native handles.
 * A future native sampler may populate it without changing Send authority.
 */
export interface DiscordVisualRecipe {
  version: typeof DISCORD_VISUAL_RECIPE_VERSION;
  theme: DiscordVisualThemePack;
  density: DiscordVisualDensity;
  zoom: number;
  dpiScale: number;
  messageColumnWidthPx: number;
  composerWidthPx: number;
  composerMinHeightPx: number;
  lineHeightPx: number;
  averageGraphemeWidthPx: number;
  highContrast: boolean;
  reducedMotion: boolean;
}

const exactKeys = [
  "version",
  "theme",
  "density",
  "zoom",
  "dpiScale",
  "messageColumnWidthPx",
  "composerWidthPx",
  "composerMinHeightPx",
  "lineHeightPx",
  "averageGraphemeWidthPx",
  "highContrast",
  "reducedMotion",
] as const;

const bounded = (value: unknown, minimum: number, maximum: number): value is number =>
  typeof value === "number" && Number.isFinite(value) && value >= minimum && value <= maximum;

export function parseDiscordVisualRecipe(value: unknown): DiscordVisualRecipe | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  const expected = [...exactKeys].sort();
  if (keys.length !== expected.length || keys.some((key, index) => key !== expected[index])) return null;
  if (record.version !== DISCORD_VISUAL_RECIPE_VERSION
    || !(["dark", "light", "midnight", "ash"] as const).includes(record.theme as DiscordVisualThemePack)
    || !(["cozy", "compact"] as const).includes(record.density as DiscordVisualDensity)
    || !bounded(record.zoom, 0.75, 2)
    || !bounded(record.dpiScale, 0.75, 4)
    || !bounded(record.messageColumnWidthPx, 320, 8_192)
    || !bounded(record.composerWidthPx, 320, 8_192)
    || Number(record.composerWidthPx) > Number(record.messageColumnWidthPx)
    || !bounded(record.composerMinHeightPx, 40, 320)
    || !bounded(record.lineHeightPx, 14, 64)
    || !bounded(record.averageGraphemeWidthPx, 3, 32)
    || typeof record.highContrast !== "boolean"
    || typeof record.reducedMotion !== "boolean") return null;
  return { ...record } as unknown as DiscordVisualRecipe;
}

export function defaultDiscordVisualRecipe(): DiscordVisualRecipe {
  return {
    version: DISCORD_VISUAL_RECIPE_VERSION,
    theme: "dark",
    density: "cozy",
    zoom: 1,
    dpiScale: 1,
    messageColumnWidthPx: 640,
    composerWidthPx: 640,
    composerMinHeightPx: 76,
    lineHeightPx: 20,
    averageGraphemeWidthPx: 7.84,
    highContrast: false,
    reducedMotion: false,
  };
}

interface DiscordVisualThemeTokens {
  background: string;
  rowHover: string;
  text: string;
  muted: string;
  strong: string;
  card: string;
  composer: string;
  brand: string;
  cue: string;
  colorScheme: "dark" | "light";
}

const themeTokens: Readonly<Record<DiscordVisualThemePack, DiscordVisualThemeTokens>> = {
  dark: {
    background: "#313338",
    rowHover: "#2e3035",
    text: "#dbdee1",
    muted: "#b5bac1",
    strong: "#f2f3f5",
    card: "#2b2d31",
    composer: "#383a40",
    brand: "#5865f2",
    cue: "rgba(73, 214, 255, .42)",
    colorScheme: "dark",
  },
  light: {
    background: "#ffffff",
    rowHover: "#f2f3f5",
    text: "#313338",
    muted: "#5c5e66",
    strong: "#060607",
    card: "#f2f3f5",
    composer: "#ebedef",
    brand: "#5865f2",
    cue: "rgba(0, 126, 167, .52)",
    colorScheme: "light",
  },
  midnight: {
    background: "#111214",
    rowHover: "#17191c",
    text: "#dbdee1",
    muted: "#b5bac1",
    strong: "#f2f3f5",
    card: "#0c0d0e",
    composer: "#1b1d20",
    brand: "#5865f2",
    cue: "rgba(73, 214, 255, .38)",
    colorScheme: "dark",
  },
  ash: {
    background: "#2b2d31",
    rowHover: "#303237",
    text: "#dbdee1",
    muted: "#a1a6ae",
    strong: "#f2f3f5",
    card: "#24262a",
    composer: "#35373c",
    brand: "#5865f2",
    cue: "rgba(73, 214, 255, .40)",
    colorScheme: "dark",
  },
};

export function discordVisualCssVariables(recipe: DiscordVisualRecipe): Readonly<Record<string, string>> {
  const tokens = themeTokens[recipe.theme];
  return {
    "--osl-transcript-bg": tokens.background,
    "--osl-transcript-row-hover": tokens.rowHover,
    "--osl-transcript-text": tokens.text,
    "--osl-transcript-muted": tokens.muted,
    "--osl-transcript-strong": tokens.strong,
    "--osl-transcript-card": tokens.card,
    "--osl-transcript-cue": recipe.highContrast ? "CanvasText" : tokens.cue,
    "--osl-overlay-bg": tokens.background,
    "--osl-overlay-text": tokens.text,
    "--osl-overlay-muted": tokens.muted,
    "--osl-composer-bg": tokens.composer,
    "--osl-overlay-brand": tokens.brand,
    "--osl-composer-min-height": `${recipe.composerMinHeightPx}px`,
    "--osl-composer-line-height": `${recipe.lineHeightPx}px`,
    "--osl-composer-width": `${recipe.composerWidthPx}px`,
    "--osl-message-column-width": `${recipe.messageColumnWidthPx}px`,
    "--osl-discord-zoom": String(recipe.zoom),
    "--osl-discord-dpi-scale": String(recipe.dpiScale),
    "color-scheme": tokens.colorScheme,
  };
}

export function discordTranscriptTheme(
  theme: DiscordVisualThemePack,
): "discord-dark" | "discord-light" | "discord-midnight" | "discord-ash" {
  return `discord-${theme}`;
}
