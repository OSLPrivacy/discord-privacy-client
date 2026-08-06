import "./onboarding-cover.css";
import { choiceRadio, continueButton } from "./onboarding-controls";

/** How the cover text lands in the other app's message box. */
export type CoverInsertionChoice = "insert-on-send" | "type-naturally";

export const initialCoverInsertionChoice = (): CoverInsertionChoice => "insert-on-send";

export function chooseCoverInsertion(
  _current: CoverInsertionChoice,
  choice: CoverInsertionChoice,
): CoverInsertionChoice {
  return choice;
}

/**
 * The two demo boxes ARE the explanation. The screen used to carry a sentence
 * under each option -- "Press Enter. The whole cover appears together." and "AI
 * writes the cover one character at a time." -- which describes a difference
 * nobody can picture from words. One box fills in a single blink; the other
 * types. Showing it takes three seconds and needs no reading.
 */
export function onboardingCoverMarkup(choice: CoverInsertionChoice): string {
  const card = (
    value: CoverInsertionChoice,
    label: string,
    tag: string,
    demo: string,
  ): string => {
    const selected = choice === value;
    return `<label class="cover-choice-card${selected ? " selected" : ""}">
      <input class="sr-only" type="radio" name="cover-mode" value="${value}"${selected ? " checked" : ""}/>
      <span class="cover-card-head">${choiceRadio()}<strong>${label}</strong><em class="cover-tag">${tag}</em></span>
      ${demo}
    </label>`;
  };

  // Both demos spell the same phrase, so the only visible difference between
  // the two cards is HOW it arrives.
  const atomicDemo = `<span class="cover-demo" aria-label="the whole cover appears at once when you press Enter">
    <span class="cover-demo-text cover-demo-atomic" aria-hidden="true">Looks good</span>
    <b class="cover-enter cover-demo-atomic" aria-hidden="true">↵</b>
  </span>`;
  const typedDemo = `<span class="cover-demo cover-demo-left" aria-label="the cover is typed one character at a time">
    <span class="cover-demo-clip" aria-hidden="true"><span class="cover-demo-text">Looks good</span></span>
    <b class="cover-caret" aria-hidden="true"></b>
  </span>`;

  return `<section class="cover-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="cover-title">Choose cover insertion</h1>
    <fieldset class="cover-choice-grid"><legend class="sr-only">Cover insertion</legend>
      ${card("insert-on-send", "Insert on send", "FREE", atomicDemo)}
      ${card("type-naturally", "Type naturally", "PRO", typedDemo)}
    </fieldset>
    <div class="setup-footer onboarding-actions">${continueButton('id="continue-cover-draft"', "cover-continue")}</div>
  </section>`;
}
