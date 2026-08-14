import "./onboarding-before-send.css";
import { continueButton, onOffToggle } from "./onboarding-controls";

/** What OSL offers to do with a file's hidden details before it is sent. */
export type CleanFilesChoice = "always" | "ask" | "never";

export interface BeforeSendChecks {
  warnUnprotected: boolean;
  warnProtected: boolean;
  cleanFiles: CleanFilesChoice;
}

export const CLEAN_FILES_CHOICES: readonly CleanFilesChoice[] = ["always", "ask", "never"];

/**
 * Warn-on-unprotected is on because sending an ordinary message in a chat you
 * normally protect is the mistake this screen exists to catch. Warn-on-protected
 * is off because it fires on every single send. Files default to asking rather
 * than to stripping, because changing someone's file without permission is worse
 * than leaving it alone.
 */
export const initialBeforeSendChecks = (): BeforeSendChecks => ({
  warnUnprotected: true,
  warnProtected: false,
  cleanFiles: "ask",
});

/**
 * All three pictures share one grammar: something travels left to right, pauses
 * at a checkpoint in the middle, and something happens there. That is the whole
 * idea of the screen -- a check that happens on the way out -- and it is the
 * same 4.2s loop on every row so the three read as one behaviour, not three.
 *
 * The paragraph under each row used to say this in words. Nobody read three
 * paragraphs on a setup screen.
 */
const TRACK = `<path class="bs-track" d="M8 44 H132"/><path class="bs-gate" d="M70 22 V44"/>`;

/** Amber "!" -- something is about to leave and OSL wants a moment first. */
function warnBadge(): string {
  return `<g class="bs-badge"><circle class="bs-badge-disc bs-warn" cx="70" cy="7" r="6.5"/><path class="bs-badge-glyph" d="M70 3.9 v3.6 M70 10.2 v0.1"/></g>`;
}

/** Green tick -- OSL did something for you rather than stopping you. */
function cleanBadge(): string {
  return `<g class="bs-badge"><circle class="bs-badge-disc bs-good" cx="70" cy="7" r="6.5"/><path class="bs-badge-check" d="M67 7 l2 2 L73 4.8"/></g>`;
}

/* Sits on the floor at y=44, which leaves the badge at y=7 floating clear above
   it. At the old height the two nearly touched and the badge read as part of the
   object rather than as a verdict on it. */
function padlock(open: boolean): string {
  const shackle = open
    ? `<path class="bs-ink" d="M15.5 33 v-3.5 a3.5 3.5 0 0 1 7 0 v3.5" transform="rotate(42 22.5 33)"/>`
    : `<path class="bs-ink" d="M15.5 33 v-3.5 a3.5 3.5 0 0 1 7 0 v3.5"/>`;
  return `<g class="bs-travel">${shackle}<rect class="bs-ink" x="12" y="33" width="14" height="11" rx="2"/></g>`;
}

/** The dots fall off WHILE the file is stopped at the gate, so the picture says
 *  the checkpoint is what removed them rather than the journey. */
function fileWithMetadata(): string {
  return `<g class="bs-travel">
    <rect class="bs-ink" x="12" y="26" width="14" height="18" rx="2"/>
    <path class="bs-rule" d="M15.5 31 h7 M15.5 35 h7"/>
    <circle class="bs-dot" cx="15.5" cy="40" r="1.7"/>
    <circle class="bs-dot" cx="19" cy="40" r="1.7"/>
    <circle class="bs-dot" cx="22.5" cy="40" r="1.7"/>
  </g>`;
}

function scene(body: string, badge: string, label: string): string {
  return `<svg class="bs-scene" viewBox="0 0 140 48" width="140" height="48" fill="none" role="img" aria-label="${label}">${TRACK}${body}${badge}</svg>`;
}

function toggle(id: string, on: boolean, label: string): string {
  return `<span class="bs-control">${onOffToggle(id, on, label)}</span>`;
}

function segmented(choice: CleanFilesChoice): string {
  const checkbox = (value: CleanFilesChoice, text: string) =>
    `<label class="bs-choice-row"><input type="checkbox" name="clean-files" value="${value}"${choice === value ? " checked" : ""}/><span>${text}</span></label>`;
  return `<span class="bs-control bs-choice-control" role="group" aria-label="Remove metadata from files">${checkbox("always", "Always")}${checkbox("ask", "Ask")}${checkbox("never", "Never")}</span>`;
}

/**
 * Row titles are the three answers to the question in the heading, so the screen
 * reads as one sentence rather than three. The full instruction each one stands
 * for ("warn me before an unprotected message") stays as the control's label for
 * anyone using a screen reader, where there is no heading above it to lean on.
 */
export function onboardingBeforeSendMarkup(checks: BeforeSendChecks): string {
  const row = (title: string, art: string, control: string) =>
    `<div class="bs-row"><span class="bs-row-title">${title}</span>${art}${control}</div>`;

  return `<section class="bs-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="bs-title">What should OSL check before you send?</h1>
    <div class="bs-list">
      ${row(
        "Unprotected messages",
        scene(padlock(true), warnBadge(), "an unlocked message stops at a checkpoint and is flagged"),
        toggle("warn-unprotected", checks.warnUnprotected, "Warn me before an unprotected message"),
      )}
      ${row(
        "Protected messages",
        scene(padlock(false), warnBadge(), "a locked message stops at the same checkpoint and is flagged"),
        toggle("warn-protected", checks.warnProtected, "Warn me before a protected message too"),
      )}
      ${row(
        "Metadata in files",
        scene(fileWithMetadata(), cleanBadge(), "a file stops at the checkpoint and its hidden details drop away"),
        segmented(checks.cleanFiles),
      )}
    </div>
    <p class="bs-quiet">Checks run on this device only</p>
    <div class="setup-footer onboarding-actions">${continueButton('data-onboarding="defaults"', "bs-continue")}</div>
  </section>`;
}
