import "./onboarding-sending.css";
import { choiceRadio, continueButton, onOffToggle } from "./onboarding-controls";
import type { SendMode } from "./state";

export interface SendingScreenState {
  mode: SendMode;
  riskAccepted: boolean;
  captureEnabled: boolean;
  /** True only where Windows actually enforces the protection right now. */
  captureApplied: boolean;
}

/**
 * 2026-08-06 restyle. The old screen was four unrelated things in one column: a
 * screen-capture section with its own heading and a status row, a
 * Write/Encrypt/Copy stepper that told only ONE mode's story, the mode list and
 * a footnote. It was also the last onboarding screen still using eyebrow
 * labels, rules and the solid button.
 *
 * Now the four modes sit side by side, each showing the same journey -- your
 * machine to the server -- so the only thing that differs between them is WHAT
 * YOU DO to make it happen. That comparison is the screen.
 */

interface SendOption {
  readonly mode: SendMode;
  readonly name: string;
  /** Empty for the two modes that cannot send without you. */
  readonly tag: string;
  readonly tagKind: "" | "warn" | "danger";
  readonly described: string;
}

export const SEND_OPTIONS: readonly SendOption[] = [
  { mode: "manual", name: "Manual", tag: "", tagKind: "", described: "you click to place the message, then send it yourself" },
  { mode: "clipboard", name: "Clipboard", tag: "", tagKind: "", described: "the message goes to the clipboard and you paste it" },
  // Two Enters: the first places, the second sends. It can still reach the
  // wrong chat if the app moves under OSL, which is what DANGEROUS is for.
  { mode: "double", name: "Double Enter", tag: "DANGEROUS", tagKind: "warn", described: "two separate Enters: one places the message, one sends it" },
  // Restored 2026-08-06 on the owner's instruction. It was in the send model all
  // along and offered in the overlay, but this screen left it out AND quietly
  // rewrote a saved Single Enter back to Manual. One keypress does everything,
  // which is also what automated-access rules tend to object to.
  { mode: "single", name: "Single Enter", tag: "DANGEROUS · MAY BREAK TOS", tagKind: "danger", described: "one Enter places the message and sends it" },
];

export const RISK_ACKNOWLEDGEMENT =
  "Automatic sending can reach the wrong chat if an app changes. OSL stops unless it can verify the destination.";

/** Only the two modes that press keys for you can be got wrong by an app moving. */
export function sendModeIsDangerous(mode: SendMode): boolean {
  return mode === "double" || mode === "single";
}

export function canLeaveSendingScreen(state: SendingScreenState): boolean {
  return !sendModeIsDangerous(state.mode) || state.riskAccepted;
}

// Every card draws the same two ends and the same road between them, so the eye
// compares the middle -- which is the only part that differs.
const ENDS = `
  <rect class="snd-ink" x="8" y="24" width="18" height="13" rx="2"/>
  <path class="snd-ink" d="M13 41 h8"/>
  <path class="snd-road" d="M30 31 H112"/>
  <rect class="snd-ink" x="116" y="23" width="18" height="6.5" rx="1.5"/>
  <rect class="snd-ink" x="116" y="33" width="18" height="6.5" rx="1.5"/>`;

const KEYCAP = `
  <rect class="snd-key" x="34" y="12" width="20" height="14" rx="3"/>
  <path class="snd-key-mark" d="M48 16.5 v3.5 h-7 M43.5 18 l-2.5 2 2.5 2"/>`;

function scene(mode: SendMode, body: string, label: string): string {
  return `<svg class="snd-scene snd-scene-${mode}" viewBox="0 0 140 44" width="140" height="44" fill="none" role="img" aria-label="${label}">${ENDS}${body}</svg>`;
}

function sceneFor(option: SendOption): string {
  const label = `your machine sends to the server: ${option.described}`;
  if (option.mode === "manual") {
    return scene("manual", `
      <rect class="snd-bubble snd-manual-bubble" x="34" y="25" width="17" height="12" rx="4"/>
      <path class="snd-cursor" d="M48 34 l0 9 2.4 -2.2 1.8 4 2 -1 -1.8 -3.9 3.2 -0.3 z"/>`, label);
  }
  if (option.mode === "clipboard") {
    return scene("clipboard", `
      <g class="snd-clip">
        <rect class="snd-ink" x="64" y="14" width="15" height="21" rx="2.5"/>
        <rect class="snd-ink" x="68" y="11.5" width="7" height="5" rx="1.5"/>
      </g>
      <rect class="snd-bubble snd-clip-in" x="32" y="25" width="15" height="11" rx="3.5"/>
      <rect class="snd-bubble snd-clip-out" x="82" y="25" width="15" height="11" rx="3.5"/>`, label);
  }
  // Two flashes against one. That contrast IS the difference between the modes,
  // and it is why these two scenes are otherwise identical.
  const flashes = option.mode === "double" ? "snd-key-twice" : "snd-key-once";
  const travel = option.mode === "double" ? "snd-double-bubble" : "snd-single-bubble";
  return scene(option.mode, `
      <g class="${flashes}">${KEYCAP}</g>
      <rect class="snd-bubble ${travel}" x="62" y="25" width="15" height="11" rx="3.5"/>`, label);
}

function riskCheckbox(): string {
  return `<svg class="snd-check" viewBox="0 0 18 18" width="18" height="18" fill="none" aria-hidden="true"><rect class="snd-check-box" x="1.5" y="1.5" width="15" height="15" rx="2" stroke-width="1.5"/><path class="snd-check-mark" d="M5 9.5 L8 12.5 L13 6" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
}

export function onboardingSendingMarkup(state: SendingScreenState): string {
  const card = (option: SendOption): string => {
    const selected = state.mode === option.mode;
    return `<label class="snd-card${selected ? " selected" : ""}">
      <input class="sr-only" type="radio" name="send-mode" value="${option.mode}" data-send-mode="${option.mode}"${selected ? " checked" : ""}/>
      <span class="snd-card-head">${choiceRadio()}<strong>${option.name}</strong></span>
      ${sceneFor(option)}
      <span class="snd-card-tag snd-tag-${option.tagKind || "none"}">${option.tag}</span>
    </label>`;
  };

  const dangerous = sendModeIsDangerous(state.mode);
  const risk = dangerous
    ? `<label class="snd-risk${state.riskAccepted ? " accepted" : ""}"><input class="sr-only" id="accept-send-risk" type="checkbox" ${state.riskAccepted ? "checked" : ""}/>${riskCheckbox()}<span class="snd-risk-copy"><strong>I understand</strong><small>${RISK_ACKNOWLEDGEMENT}</small></span></label>`
    : "";

  // Says what is TRUE right now, including when the honest answer is "this
  // machine cannot do it". A switch that is on but unenforced must not read as
  // protection.
  const captureNote = state.captureEnabled
    ? (state.captureApplied
      ? "Active on this device."
      : "This device cannot enforce it. Everything on screen can be captured.")
    : "Off. Everything on screen can be captured.";

  return `<section class="snd-onboarding" aria-labelledby="route-heading">
    <h1 id="route-heading" tabindex="-1" class="snd-title">Privacy and sending</h1>
    <div class="snd-grid" role="radiogroup" aria-label="Sending behavior">${SEND_OPTIONS.map(card).join("")}</div>
    ${risk}
    <p class="snd-quiet">If OSL cannot prove where it is sending, it sends nothing</p>
    <div class="snd-capture">
      <span class="snd-capture-copy"><strong>Resist screenshots of OSL</strong><small>${captureNote}</small></span>
      <span class="snd-capture-control">${onOffToggle("window-capture-enabled", state.captureEnabled, "Resist screenshots of OSL")}</span>
    </div>
    <div class="setup-footer onboarding-actions">${continueButton(`id="finish-onboarding" ${canLeaveSendingScreen(state) ? "" : 'disabled aria-disabled="true"'}`, "snd-continue")}</div>
  </section>`;
}
