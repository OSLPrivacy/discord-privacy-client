import { appearanceSettingsMarkup, type AppearancePreferences } from "./appearance-preferences";
import { appearanceLivePreviewMarkup } from "./appearance-live-preview";
import type { ScopedProfileRecord } from "./osl-profile-pane";
import { settingsProfileBlockMarkup } from "./settings-profile-block";
import type { ThemeChoice } from "./theme-preference";

export const MESSAGE_APPEARANCE_PREVIEW_DISCLOSURE = "Message appearance controls are preview features and may not affect every live thread in this release.";

export function appearanceSettingsContent(
  appearancePreferences: AppearancePreferences,
  themeChoice: ThemeChoice,
  profileDraft: ScopedProfileRecord,
): string {
  const theme = `<div class="theme-grid">${(["system", "dark", "light"] as ThemeChoice[]).map((choice) => `<button class="theme-card ${themeChoice === choice ? "selected" : ""}" data-theme-choice="${choice}"><span class="theme-swatch ${choice}"></span><strong class="machine-fact">${choice[0].toUpperCase()}${choice.slice(1)}</strong><small>${choice === "system" ? "Follow this device" : `${choice} interface`}</small></button>`).join("")}</div>`;
  const legacyControlsOnly = appearanceSettingsMarkup(appearancePreferences).replace(/<aside class="appearance-preview"[\s\S]*?<\/aside>/u, "");
  const accent = ({ cyan: "#2ac0f0", violet: "#a28af8", coral: "#ff977f" } as const)[appearancePreferences.accent];
  return `${legacyControlsOnly.replace("</section>", `<p class="appearance-preview-disclosure" role="note">${MESSAGE_APPEARANCE_PREVIEW_DISCLOSURE}</p></section>`)}<section class="appearance-theme"><h3 class="machine-fact">Theme</h3>${theme}</section><div data-settings-profile-mount>${settingsProfileBlockMarkup(profileDraft)}</div>${appearanceLivePreviewMarkup(profileDraft, accent)}`;
}
