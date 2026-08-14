import "./onboarding-capture-visibility.css";
import { continueButton } from "./onboarding-controls";
import type { CoverInsertionChoice } from "./state";

export const CAPTURE_VISIBILITY_TITLE = "Can people tell you use OSL";

/**
 * TASK 6802 — four REAL switches, not two labels.
 *
 * This page used to be a SILENT/VISIBLE pair of buttons that set nothing: the
 * markup carried `aria-pressed` and the choice reached no preference, no
 * command and no behaviour. The owner's ruling keeps the page and requires
 * four working Visibility switches, so every row here is bound to one field of
 * the real onboarding-preference record that `save_onboarding_preferences`
 * persists — the same record the app reloads on the next launch.
 *
 * Each checkbox owns exactly one field. Nothing here writes two.
 */
export const VISIBILITY_SWITCH_IDS = [
  "window-capture",
  "plaintext-preview",
  "cover-insertion",
  "wire-policy",
] as const;

export type VisibilitySwitchId = (typeof VISIBILITY_SWITCH_IDS)[number];

/** The real preference field each switch moves. */
export type VisibilitySwitchField =
  | "windowCaptureEnabled"
  | "showPlaintextPreview"
  | "coverInsertion"
  | "rnWirePolicyRequested";

export interface VisibilitySwitchDefinition {
  id: VisibilitySwitchId;
  field: VisibilitySwitchField;
  label: string;
  detail: string;
}

export const VISIBILITY_SWITCHES: readonly VisibilitySwitchDefinition[] = [
  {
    id: "window-capture",
    field: "windowCaptureEnabled",
    label: "Hide OSL from screen capture",
    detail: "A screen recording or shared screen shows the OSL window as blank.",
  },
  {
    id: "plaintext-preview",
    field: "showPlaintextPreview",
    label: "Show decrypted text inside OSL",
    detail: "Off means anyone looking at this screen cannot read an opened message.",
  },
  {
    id: "cover-insertion",
    field: "coverInsertion",
    label: "Let OSL type cover text into the app",
    detail: "Off means you place the cover text yourself, so nothing types on its own.",
  },
  {
    id: "wire-policy",
    field: "rnWirePolicyRequested",
    label: "Ask for the next-generation wire policy",
    detail: "Changes what OSL's own protected traffic looks like to anything watching it.",
  },
] as const;

/** The four values, in the shape the preference record stores them. */
export interface VisibilitySwitchValues {
  windowCaptureEnabled: boolean;
  showPlaintextPreview: boolean;
  coverInsertion: CoverInsertionChoice | null;
  rnWirePolicyRequested: boolean;
}

export const COVER_INSERTION_ON: CoverInsertionChoice = "insert-on-send";
export const COVER_INSERTION_OFF: CoverInsertionChoice = "type-naturally";

export function visibilitySwitchChecked(
  definition: VisibilitySwitchDefinition,
  values: VisibilitySwitchValues,
): boolean {
  if (definition.field === "coverInsertion") return values.coverInsertion === COVER_INSERTION_ON;
  return values[definition.field] === true;
}

export function isVisibilitySwitchId(candidate: string): candidate is VisibilitySwitchId {
  return (VISIBILITY_SWITCH_IDS as readonly string[]).includes(candidate);
}

/**
 * Apply one switch. Returns a NEW record so a caller cannot half-apply a
 * change, and touches only the one field the switch owns — the check for this
 * task compares every other field before and after.
 */
export function applyVisibilitySwitch(
  values: VisibilitySwitchValues,
  id: VisibilitySwitchId,
  checked: boolean,
): VisibilitySwitchValues {
  const definition = VISIBILITY_SWITCHES.find((candidate) => candidate.id === id);
  if (!definition) return { ...values };
  if (definition.field === "coverInsertion") {
    return { ...values, coverInsertion: checked ? COVER_INSERTION_ON : COVER_INSERTION_OFF };
  }
  return { ...values, [definition.field]: checked };
}

export function onboardingCaptureVisibilityMarkup(values: VisibilitySwitchValues): string {
  const rows = VISIBILITY_SWITCHES.map((definition) => {
    const checked = visibilitySwitchChecked(definition, values);
    return `<label class="cv-switch" data-visibility-switch-row="${definition.id}">
      <input type="checkbox" data-visibility-switch="${definition.id}" data-visibility-field="${definition.field}" ${checked ? "checked" : ""}/>
      <span><strong>${definition.label}</strong><small>${definition.detail}</small></span>
    </label>`;
  }).join("");
  return `<section class="cv-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="cv-title">${CAPTURE_VISIBILITY_TITLE}</h1>
    <div class="cv-switch-list" role="group" aria-label="${CAPTURE_VISIBILITY_TITLE}">${rows}</div>
    <div class="setup-footer onboarding-actions cv-actions">
      ${continueButton('data-onboarding="passwords"', "cv-continue")}
      <button class="text-button onboarding-step-back cv-back" data-onboarding="cover" type="button">Back</button>
    </div>
  </section>`;
}
