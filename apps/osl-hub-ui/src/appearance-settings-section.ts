import { appearanceSettingsMarkup, type AppearancePreferences } from "./appearance-preferences";
import type { ThemeChoice } from "./theme-preference";

export function appearanceSettingsContent(
  appearancePreferences: AppearancePreferences,
  themeChoice: ThemeChoice,
): string {
  const theme = `<div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong>${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>`;
  return `${appearanceSettingsMarkup(appearancePreferences)}<section class="appearance-theme"><h3>Theme</h3>${theme}</section>`;
}
