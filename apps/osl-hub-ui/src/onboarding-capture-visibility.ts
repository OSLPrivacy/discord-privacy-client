import "./onboarding-capture-visibility.css";
import { continueButton } from "./onboarding-controls";

export const CAPTURE_VISIBILITY_TITLE = "Can people tell you use OSL";
export const CAPTURE_VISIBILITY_CONTROLS = ["SILENT", "VISIBLE", "Continue", "Back"] as const;

export function onboardingCaptureVisibilityMarkup(): string {
  return `<section class="cv-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="cv-title">${CAPTURE_VISIBILITY_TITLE}</h1>
    <div class="cv-choice-row" role="group" aria-label="${CAPTURE_VISIBILITY_TITLE}">
      <button class="cv-choice selected" data-capture-visibility="silent" type="button" aria-pressed="true"><span>SILENT</span></button>
      <button class="cv-choice" data-capture-visibility="visible" type="button" aria-pressed="false"><span>VISIBLE</span></button>
    </div>
    <div class="setup-footer onboarding-actions cv-actions">
      ${continueButton('data-onboarding="passwords"', "cv-continue")}
      <button class="text-button onboarding-step-back cv-back" data-onboarding="cover" type="button">Back</button>
    </div>
  </section>`;
}
