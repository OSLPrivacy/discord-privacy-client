import "./onboarding-cover.css";
import { continueButton } from "./onboarding-controls";

export type SilentVisibleMode = "SILENT" | "VISIBLE";

const silentVisibleModes: readonly SilentVisibleMode[] = ["SILENT", "VISIBLE"];
export const visibleDisplayMark = "VISIBLE";

export function chooseSilentVisibleMode(
  current: SilentVisibleMode | null,
  candidate: unknown,
): SilentVisibleMode | null {
  return silentVisibleModes.includes(candidate as SilentVisibleMode)
    ? candidate as SilentVisibleMode
    : current;
}

export function silentVisibleDisplayMark(mode: SilentVisibleMode): string | null {
  return mode === "VISIBLE" ? visibleDisplayMark : null;
}

function button(mode: SilentVisibleMode, selected: boolean): string {
  const mark = selected ? silentVisibleDisplayMark(mode) : null;
  return `<button class="cover-choice-card${selected ? " selected" : ""}" type="button" data-silent-visible-mode="${mode}" aria-pressed="${selected}">
    <span class="cover-card-head"><strong>${mode}</strong>${mark ? `<em class="cover-tag" data-silent-visible-display-mark>${mark}</em>` : ""}</span>
  </button>`;
}

export function onboardingSilentVisibleMarkup(mode: SilentVisibleMode | null): string {
  return `<section class="cover-onboarding silent-visible-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="cover-title">Choose display mode</h1>
    <div class="cover-choice-grid" role="group" aria-label="Display mode">
      ${button("SILENT", mode === "SILENT")}
      ${button("VISIBLE", mode === "VISIBLE")}
    </div>
    <div class="setup-footer onboarding-actions">${continueButton(`id="continue-silent-visible"${mode === null ? " disabled" : ""}`, "cover-continue")}</div>
  </section>`;
}
