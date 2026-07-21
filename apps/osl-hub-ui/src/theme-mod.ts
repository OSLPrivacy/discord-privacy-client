export const themeModStorageKey = "osl-theme-mod-v1";

const colorKeys = ["brand", "background", "panel", "text", "muted"] as const;
type ThemeColorKey = typeof colorKeys[number];

export interface ThemeMod {
  version: 1;
  name: string;
  colors: Record<ThemeColorKey, string>;
  radius: number;
}

const hexColor = /^#[0-9a-fA-F]{6}$/;

export function parseThemeMod(raw: string | null): ThemeMod | null {
  if (raw === null) return null;
  if (new TextEncoder().encode(raw).length > 8 * 1024) return null;
  try {
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (Object.keys(value).sort().join(",") !== "colors,name,radius,version"
      || value.version !== 1
      || typeof value.name !== "string"
      || value.name.trim().length < 1
      || value.name.length > 48
      || !Number.isInteger(value.radius)
      || Number(value.radius) < 0
      || Number(value.radius) > 20
      || typeof value.colors !== "object"
      || value.colors === null) return null;
    const colors = value.colors as Record<string, unknown>;
    if (Object.keys(colors).sort().join(",") !== [...colorKeys].sort().join(",")
      || colorKeys.some((key) => typeof colors[key] !== "string" || !hexColor.test(colors[key] as string))) return null;
    return value as unknown as ThemeMod;
  } catch {
    return null;
  }
}

export function applyThemeMod(root: HTMLElement, mod: ThemeMod | null): void {
  const mapping: Record<ThemeColorKey, string> = {
    brand: "--brand",
    background: "--bg",
    panel: "--panel",
    text: "--text",
    muted: "--muted",
  };
  for (const key of colorKeys) {
    if (mod) root.style.setProperty(mapping[key], mod.colors[key]);
    else root.style.removeProperty(mapping[key]);
  }
  if (mod) {
    root.style.setProperty("--radius", `${mod.radius}px`);
    root.dataset.themeMod = mod.name;
  } else {
    root.style.removeProperty("--radius");
    delete root.dataset.themeMod;
  }
}
