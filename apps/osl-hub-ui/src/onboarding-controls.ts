import "./onboarding-controls.css";

/**
 * The two controls every redesigned onboarding screen shares.
 *
 * They live here rather than being copied per screen because they had already
 * drifted once: the sign-in screens' arrow and the connection screen's arrow
 * were separate strings, and only one of them got the hover spring.
 */

/**
 * Drawn rather than built from CSS borders. A 1.5px ring made with
 * `border-radius` at this size lands on half-pixels and renders visibly soft --
 * which is exactly what it looked like on the first pass.
 *
 * Colour comes from the enclosing `.selected` card, so the caller only has to
 * mark the card.
 */
export function choiceRadio(): string {
  return `<svg class="osl-radio" viewBox="0 0 18 18" width="18" height="18" fill="none" aria-hidden="true">
    <circle class="osl-radio-ring" cx="9" cy="9" r="7.5" stroke-width="1.5"/>
    <circle class="osl-radio-dot" cx="9" cy="9" r="4"/>
  </svg>`;
}

/** Seats itself 14px from the button's right edge and leans forward on hover. */
export function continueArrow(): string {
  return `<svg class="signin-icon signin-arrow" viewBox="0 0 24 24" width="17" height="17" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12 h14"/><path d="M13 6 l6 6 -6 6"/></svg>`;
}

/**
 * The one on/off switch. Shape matters here more than anywhere else on the
 * screen: `styles.css` squares every corner in the product on purpose, and a
 * switch drawn as two squares reads as a broken box beside a white block rather
 * than as something you flip. The knob is a disc and the track is the slot it
 * slides in, so both are carved out of that rule by name.
 */
export function onOffToggle(id: string, on: boolean, label: string): string {
  return `<label class="osl-toggle"><input type="checkbox" id="${id}" ${on ? "checked" : ""}/><span class="osl-toggle-track"><span class="osl-toggle-knob"></span></span><span class="sr-only">${label}</span></label>`;
}

/** The full-width Continue used by every redesigned setup screen. */
export function continueButton(attributes: string, extraClass = ""): string {
  return `<button class="signin-unlock osl-continue${extraClass ? ` ${extraClass}` : ""}" ${attributes} type="button"><span class="signin-unlock-label">Continue</span>${continueArrow()}</button>`;
}
