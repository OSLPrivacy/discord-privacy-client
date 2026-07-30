import { describe, expect, it } from "vitest";
import {
  DISCORD_VISUAL_RECIPE_VERSION,
  defaultDiscordVisualRecipe,
  discordTranscriptTheme,
  discordVisualCssVariables,
  parseDiscordVisualRecipe,
} from "./discord-visual-recipe";

const valid = {
  ...defaultDiscordVisualRecipe(),
  version: DISCORD_VISUAL_RECIPE_VERSION,
};

describe("Discord content-free visual recipe", () => {
  it("accepts the exact bounded contract and returns an independent record", () => {
    const parsed = parseDiscordVisualRecipe(valid);
    expect(parsed).toEqual(valid);
    expect(parsed).not.toBe(valid);
  });

  it("supports only the reviewed theme packs and densities", () => {
    for (const theme of ["dark", "light", "midnight", "ash"] as const) {
      expect(parseDiscordVisualRecipe({ ...valid, theme })?.theme).toBe(theme);
      expect(discordTranscriptTheme(theme)).toBe(`discord-${theme}`);
    }
    for (const density of ["cozy", "compact"] as const) {
      expect(parseDiscordVisualRecipe({ ...valid, density })?.density).toBe(density);
    }
    expect(parseDiscordVisualRecipe({ ...valid, theme: "nitro-custom" })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, density: "ultra" })).toBeNull();
  });

  it("rejects extra, missing, secret-bearing, non-finite, and unsafe fields", () => {
    expect(parseDiscordVisualRecipe({ ...valid, messageText: "secret" })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, token: "secret" })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, selector: "[contenteditable]" })).toBeNull();
    const { lineHeightPx: _lineHeightPx, ...missing } = valid;
    expect(parseDiscordVisualRecipe(missing)).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, zoom: Number.NaN })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, dpiScale: Number.POSITIVE_INFINITY })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, composerWidthPx: valid.messageColumnWidthPx + 1 })).toBeNull();
    expect(parseDiscordVisualRecipe({ ...valid, messageColumnWidthPx: 319 })).toBeNull();
  });

  it("maps theme, accessibility, geometry, and scale into local CSS variables", () => {
    const recipe = parseDiscordVisualRecipe({
      ...valid,
      theme: "light",
      density: "compact",
      zoom: 1.25,
      dpiScale: 1.5,
      composerMinHeightPx: 52,
      lineHeightPx: 18,
      highContrast: true,
      reducedMotion: true,
    });
    if (!recipe) throw new Error("fixture rejected");
    expect(discordVisualCssVariables(recipe)).toMatchObject({
      "--osl-overlay-bg": "#ffffff",
      "--osl-composer-min-height": "52px",
      "--osl-composer-line-height": "18px",
      "--osl-discord-zoom": "1.25",
      "--osl-discord-dpi-scale": "1.5",
      "--osl-transcript-cue": "CanvasText",
      "color-scheme": "light",
    });
  });

  it("keeps current secure presentation defaults when no recipe is available", () => {
    expect(parseDiscordVisualRecipe(undefined)).toBeNull();
    expect(defaultDiscordVisualRecipe()).toMatchObject({
      theme: "dark",
      density: "cozy",
      zoom: 1,
      dpiScale: 1,
      highContrast: false,
      reducedMotion: false,
    });
  });
});
