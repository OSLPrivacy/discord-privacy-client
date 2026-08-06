import "./onboarding-delete.css";
import { continueButton, onOffToggle } from "./onboarding-controls";

export interface DeleteChoices {
  deleteDrafts: boolean;
  deleteOldMessages: boolean;
}

/**
 * Both off. Nothing is deleted unless somebody turns it on here.
 *
 * This screen used to be phrased the other way round -- "what should OSL KEEP"
 * with the keeping switched on -- which meant the safe answer was the one where
 * the switches were lit. Asking what to DELETE puts the destructive answer on
 * the side that takes a deliberate action, and leaves the defaults dark.
 */
export const initialDeleteChoices = (): DeleteChoices => ({
  deleteDrafts: false,
  deleteOldMessages: false,
});

/**
 * Both scenes are drawn on the SAME canvas -- same width, same viewBox, content
 * built around the same centre line at x=70 -- so the window on one card and the
 * clock on the other sit on one vertical axis instead of each floating wherever
 * its own artwork happened to land.
 */
const CANVAS = 'class="del-scene" width="76" height="48" viewBox="35 0 70 44" fill="none"';

/**
 * The window blinks out and comes back; the draft inside never flinches. That is
 * the whole promise -- OSL closing does not take your half-written message with
 * it -- so the draft is deliberately NOT inside the blinking group.
 */
function draftScene(): string {
  return `<svg ${CANVAS} role="img" aria-label="an app window closes and reopens while the unsent message inside it stays put">
    <g class="del-blink">
      <rect class="del-ink" x="42" y="7" width="56" height="34" rx="3.5"/>
      <path class="del-detail" d="M42 15 H98"/>
      <circle class="del-ink-fill" cx="48" cy="11" r="1.4"/>
    </g>
    <rect class="del-ink del-draft" x="52" y="21" width="24" height="13" rx="4"/>
    <path class="del-detail" d="M57 25.5 h14 M57 29.5 h8"/>
    <g class="del-badge">
      <circle class="del-badge-disc" cx="98" cy="9" r="6.5"/>
      <path class="del-badge-check" d="M95 9 l2 2 L101 6.8"/>
    </g>
  </svg>`;
}

/** A clock going round while the messages under it lift away and vanish. */
function ageScene(): string {
  return `<svg ${CANVAS} role="img" aria-label="time passes on a clock while old messages fade out of a tray">
    <circle class="del-ink" cx="70" cy="10" r="6.5"/>
    <path class="del-ink" d="M70 10 L72.8 10"/>
    <path class="del-ink del-hand" d="M70 10 L70 5.8"/>
    <path class="del-ink" d="M50 24 v14 h40 v-14"/>
    <rect class="del-note" x="55" y="27" width="9" height="7" rx="2.5"/>
    <rect class="del-note" x="66" y="27" width="9" height="7" rx="2.5"/>
    <rect class="del-note" x="77" y="27" width="9" height="7" rx="2.5"/>
  </svg>`;
}

export function onboardingDeleteMarkup(choices: DeleteChoices): string {
  const row = (title: string, scene: string, control: string) =>
    `<div class="del-row"><span class="del-row-title">${title}</span><span class="del-art">${scene}</span><span class="del-control">${control}</span></div>`;

  return `<section class="del-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="del-title">What should OSL delete?</h1>
    <div class="del-list">
      ${row("Unsent private messages", draftScene(), onOffToggle("delete-drafts", choices.deleteDrafts, "Delete unsent private messages"))}
      ${row("Old messages", ageScene(), onOffToggle("delete-old-messages", choices.deleteOldMessages, "Delete old messages"))}
    </div>
    <p class="del-quiet">Everything kept is encrypted. Nothing is deleted without confirmation</p>
    <div class="setup-footer onboarding-actions">${continueButton('id="continue-defaults-review"', "del-continue")}</div>
  </section>`;
}
