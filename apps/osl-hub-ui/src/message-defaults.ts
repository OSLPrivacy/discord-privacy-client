import "./message-defaults.css";
import { choiceRadio } from "./onboarding-controls";

/**
 * Message defaults screen.
 *
 * Four saved choices decide how every NEW protected message starts: how long it
 * lives (timer), how far a burn reaches (burn scope), how long a view-once item
 * stays on screen (view-once length), and who writes the cover text (AI or
 * Wordbank). The same four values are the ones `cmd_osl_save_message_defaults`
 * stores and `cmd_osl_start_direct_new_message_plan` reads back, so the labels
 * here are a reading of saved state, never a second copy of it.
 *
 * Reset fills the form with the OSL factory values but does NOT save them; a
 * defaults screen that silently rewrote saved state on a stray click would take
 * a decision away from the person making it. Save is the only writer.
 */

export type MessageBurnScope = "message" | "conversation" | "app";
export type MessageCoverWriting = "ai_covertext" | "plaintext";

export type MessageDefaults = {
  burnScope: MessageBurnScope;
  timerSeconds: number;
  viewOnceLengthSeconds: number;
  coverWriting: MessageCoverWriting;
};

/**
 * The values a fresh OSL install saves. These mirror the Rust defaults in
 * `crates/ipc/src/app_preferences.rs` (`MessageDefaults::default`), and
 * `message-defaults.test.ts` reads that file to keep the two in step.
 */
export const FACTORY_MESSAGE_DEFAULTS: MessageDefaults = {
  burnScope: "message",
  timerSeconds: 300,
  viewOnceLengthSeconds: 10,
  coverWriting: "plaintext",
};

export const MESSAGE_TIMER_CHOICES: ReadonlyArray<{ seconds: number; label: string }> = [
  { seconds: 300, label: "5 minutes" },
  { seconds: 3_600, label: "1 hour" },
  { seconds: 86_400, label: "1 day" },
  { seconds: 604_800, label: "1 week" },
];

export const MESSAGE_BURN_SCOPE_CHOICES: ReadonlyArray<{ value: MessageBurnScope; label: string }> = [
  { value: "message", label: "This message" },
  { value: "conversation", label: "This conversation" },
  { value: "app", label: "This app" },
];

export const VIEW_ONCE_LENGTH_CHOICES: ReadonlyArray<{ seconds: number; label: string }> = [
  { seconds: 10, label: "10 seconds" },
  { seconds: 30, label: "30 seconds" },
  { seconds: 45, label: "45 seconds" },
  { seconds: 60, label: "60 seconds" },
];

export const MESSAGE_WRITING_CHOICES: ReadonlyArray<{ value: MessageCoverWriting; label: string }> = [
  { value: "ai_covertext", label: "AI" },
  { value: "plaintext", label: "Wordbank" },
];

export type MessageDefaultsControl = "timer" | "burn-scope" | "view-once-length" | "writing";

export const MESSAGE_DEFAULTS_TITLE = "Message defaults";

/** One heading and one plain sentence for each of the four saved choices. */
export const MESSAGE_DEFAULTS_EXPLANATIONS: Readonly<Record<MessageDefaultsControl, { legend: string; why: string }>> = {
  timer: {
    legend: "Timer",
    why: "Every new protected message is removed this long after you send it, on both sides.",
  },
  "burn-scope": {
    legend: "Burn after reading",
    why: "Choose how much a burn takes away once the message has been read: one message, the whole conversation, or everything OSL holds for this app.",
  },
  "view-once-length": {
    legend: "View-once length",
    why: "A view-once picture or note stays on screen this long after it is opened, then it closes by itself.",
  },
  writing: {
    legend: "AI or Wordbank",
    why: "Choose who writes the cover text that carries your message: the local AI model, or the built-in wordbank of ordinary words.",
  },
};

export type MessageDefaultsScreenState = {
  saved: MessageDefaults;
  draft: MessageDefaults;
};

export function initialMessageDefaultsScreenState(
  saved: MessageDefaults = FACTORY_MESSAGE_DEFAULTS,
): MessageDefaultsScreenState {
  return { saved: { ...saved }, draft: { ...saved } };
}

export function messageDefaultsUnsaved(state: MessageDefaultsScreenState): boolean {
  return (
    state.draft.burnScope !== state.saved.burnScope
    || state.draft.timerSeconds !== state.saved.timerSeconds
    || state.draft.viewOnceLengthSeconds !== state.saved.viewOnceLengthSeconds
    || state.draft.coverWriting !== state.saved.coverWriting
  );
}

/** Move one control in the form. An unknown control or value changes nothing. */
export function chooseMessageDefault(
  state: MessageDefaultsScreenState,
  control: MessageDefaultsControl,
  value: string,
): MessageDefaultsScreenState {
  const draft = { ...state.draft };
  if (control === "timer") {
    const choice = MESSAGE_TIMER_CHOICES.find((candidate) => String(candidate.seconds) === value);
    if (!choice) return state;
    draft.timerSeconds = choice.seconds;
  } else if (control === "burn-scope") {
    const choice = MESSAGE_BURN_SCOPE_CHOICES.find((candidate) => candidate.value === value);
    if (!choice) return state;
    draft.burnScope = choice.value;
  } else if (control === "view-once-length") {
    const choice = VIEW_ONCE_LENGTH_CHOICES.find((candidate) => String(candidate.seconds) === value);
    if (!choice) return state;
    draft.viewOnceLengthSeconds = choice.seconds;
  } else if (control === "writing") {
    const choice = MESSAGE_WRITING_CHOICES.find((candidate) => candidate.value === value);
    if (!choice) return state;
    draft.coverWriting = choice.value;
  } else {
    return state;
  }
  return { saved: state.saved, draft };
}

/** Save writes the form onto the saved values. */
export function saveMessageDefaults(state: MessageDefaultsScreenState): MessageDefaultsScreenState {
  return { saved: { ...state.draft }, draft: { ...state.draft } };
}

/** Reset fills the form with the factory values and leaves saved state alone. */
export function resetMessageDefaults(state: MessageDefaultsScreenState): MessageDefaultsScreenState {
  return { saved: state.saved, draft: { ...FACTORY_MESSAGE_DEFAULTS } };
}

export function messageTimerLabel(seconds: number): string {
  return MESSAGE_TIMER_CHOICES.find((choice) => choice.seconds === seconds)?.label ?? `${seconds} seconds`;
}

export function messageBurnScopeLabel(scope: MessageBurnScope): string {
  return MESSAGE_BURN_SCOPE_CHOICES.find((choice) => choice.value === scope)?.label ?? scope;
}

export function viewOnceLengthLabel(seconds: number): string {
  return VIEW_ONCE_LENGTH_CHOICES.find((choice) => choice.seconds === seconds)?.label ?? `${seconds} seconds`;
}

export function messageWritingLabel(writing: MessageCoverWriting): string {
  return MESSAGE_WRITING_CHOICES.find((choice) => choice.value === writing)?.label ?? writing;
}

/** The four saved values as the words the screen shows for them. */
export function savedMessageDefaultLabels(defaults: MessageDefaults): Record<MessageDefaultsControl, string> {
  return {
    timer: messageTimerLabel(defaults.timerSeconds),
    "burn-scope": messageBurnScopeLabel(defaults.burnScope),
    "view-once-length": viewOnceLengthLabel(defaults.viewOnceLengthSeconds),
    writing: messageWritingLabel(defaults.coverWriting),
  };
}

/** The wire shape `cmd_osl_save_message_defaults` accepts. */
export function messageDefaultsWirePayload(defaults: MessageDefaults): {
  burn_scope: string;
  timer_seconds: number;
  view_once_length_seconds: number;
  cover_writing: string;
} {
  return {
    burn_scope: defaults.burnScope,
    timer_seconds: defaults.timerSeconds,
    view_once_length_seconds: defaults.viewOnceLengthSeconds,
    cover_writing: defaults.coverWriting,
  };
}

function choiceMarkup(
  control: MessageDefaultsControl,
  value: string,
  label: string,
  selected: boolean,
  saved: boolean,
): string {
  return `<label class="msg-def-choice${selected ? " selected" : ""}${saved ? " saved" : ""}">`
    + `<input class="sr-only" type="radio" name="message-default-${control}" value="${value}" data-message-default="${control}"${selected ? " checked" : ""}/>`
    + choiceRadio()
    + `<span class="msg-def-choice-label">${label}</span>`
    + (saved ? `<span class="msg-def-saved-tag" data-message-default-saved="${control}">Saved</span>` : "")
    + "</label>";
}

function groupMarkup(
  control: MessageDefaultsControl,
  choices: ReadonlyArray<{ value: string; label: string }>,
  draftValue: string,
  savedValue: string,
): string {
  const { legend, why } = MESSAGE_DEFAULTS_EXPLANATIONS[control];
  const rows = choices
    .map((choice) => choiceMarkup(control, choice.value, choice.label, choice.value === draftValue, choice.value === savedValue))
    .join("");
  return `<fieldset class="msg-def-group" data-message-default-group="${control}">`
    + `<legend>${legend}</legend>`
    + `<p class="msg-def-why" data-message-default-why="${control}">${why}</p>`
    + `<div class="msg-def-choices">${rows}</div>`
    + "</fieldset>";
}

export function messageDefaultsScreenMarkup(state: MessageDefaultsScreenState): string {
  const unsaved = messageDefaultsUnsaved(state);
  const groups = [
    groupMarkup(
      "timer",
      MESSAGE_TIMER_CHOICES.map((choice) => ({ value: String(choice.seconds), label: choice.label })),
      String(state.draft.timerSeconds),
      String(state.saved.timerSeconds),
    ),
    groupMarkup(
      "burn-scope",
      MESSAGE_BURN_SCOPE_CHOICES.map((choice) => ({ value: choice.value, label: choice.label })),
      state.draft.burnScope,
      state.saved.burnScope,
    ),
    groupMarkup(
      "view-once-length",
      VIEW_ONCE_LENGTH_CHOICES.map((choice) => ({ value: String(choice.seconds), label: choice.label })),
      String(state.draft.viewOnceLengthSeconds),
      String(state.saved.viewOnceLengthSeconds),
    ),
    groupMarkup(
      "writing",
      MESSAGE_WRITING_CHOICES.map((choice) => ({ value: choice.value, label: choice.label })),
      state.draft.coverWriting,
      state.saved.coverWriting,
    ),
  ].join("");
  const status = unsaved
    ? "Not saved yet. Save to use these choices for new protected messages."
    : "Saved. Protected messages start with these four choices.";
  return `<main class="content-viewport message-defaults-page" aria-labelledby="route-heading">`
    + `<header class="msg-def-head">`
    + `<h1 id="route-heading" tabindex="-1">${MESSAGE_DEFAULTS_TITLE}</h1>`
    + `<p class="msg-def-intro">Protected messages start with the four choices below. You can still change any of them on a single message before you send it.</p>`
    + `<p class="msg-def-receipts">Read receipts stay off unless you and the other person both turn them on.</p>`
    + "</header>"
    + `<div class="msg-def-groups">${groups}</div>`
    + `<footer class="msg-def-actions">`
    + `<p class="msg-def-status" data-message-default-status="${unsaved ? "unsaved" : "saved"}">${status}</p>`
    + `<span class="msg-def-buttons">`
    + `<button class="button" id="reset-message-defaults" type="button" data-message-default-reset>Reset</button>`
    + `<button class="button primary" id="save-message-defaults" type="button" data-message-default-save>Save</button>`
    + "</span>"
    + "</footer>"
    + "</main>";
}
