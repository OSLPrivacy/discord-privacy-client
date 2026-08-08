/**
 * TASK 0564 - view-once overlay: a modal layer displayed over the app with a
 * protected text or image, a play button, a duration choice, and an X close
 * action.
 *
 * Words are read from the backend's `view_once_overlay` screen-words file for
 * the current language, the same way `view-once-player-screen.ts` does.
 */

export interface ViewOnceOverlayProtectedText {
  readonly kind: "text";
  readonly text: string;
}

export interface ViewOnceOverlayProtectedImage {
  readonly kind: "image";
  readonly src: string;
  readonly alt?: string;
}

export type ViewOnceOverlayProtectedContent = ViewOnceOverlayProtectedText | ViewOnceOverlayProtectedImage;

export interface ViewOnceOverlayDurationChoice {
  readonly seconds: number;
}

export interface ViewOnceOverlayModel {
  readonly open: boolean;
  readonly content: ViewOnceOverlayProtectedContent | null;
  readonly durationSeconds: number;
  readonly durationChoices: readonly ViewOnceOverlayDurationChoice[];
}

export const EMPTY_VIEW_ONCE_OVERLAY: ViewOnceOverlayModel = {
  open: false,
  content: null,
  durationSeconds: 10,
  durationChoices: [{ seconds: 5 }, { seconds: 10 }, { seconds: 30 }],
};

function requireWord(words: Record<string, string>, key: string): string {
  const value = words[key];
  if (!value) throw new Error(`OSL: missing view once overlay screen word '${key}'`);
  return value;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (char) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char] as string
  ));
}

function durationOptionMarkup(choice: ViewOnceOverlayDurationChoice, selected: boolean, words: Record<string, string>): string {
  const label = requireWord(words, "duration_option_seconds_template").replace("{seconds}", String(choice.seconds));
  return `<option value="${choice.seconds}"${selected ? " selected" : ""}>${escapeHtml(label)}</option>`;
}

function protectedContentMarkup(content: ViewOnceOverlayProtectedContent | null, words: Record<string, string>): string {
  if (!content) {
    return `<div class="voo-content voo-content-empty" data-voo-content-state="empty">
      <p class="voo-empty-copy">${escapeHtml(requireWord(words, "empty_copy"))}</p>
    </div>`;
  }
  if (content.kind === "text") {
    return `<div class="voo-content voo-content-text" data-voo-content-state="text">
      <p class="voo-protected-text">${escapeHtml(content.text)}</p>
    </div>`;
  }
  return `<div class="voo-content voo-content-image" data-voo-content-state="image">
    <img class="voo-protected-image" src="${escapeHtml(content.src)}" alt="${escapeHtml(content.alt ?? requireWord(words, "protected_image_alt"))}" data-voo-image>
  </div>`;
}

export function viewOnceOverlayMarkup(model: ViewOnceOverlayModel, words: Record<string, string>): string {
  if (!model.open) {
    return `<div class="view-once-overlay voo-closed" data-voo-state="closed" hidden></div>`;
  }

  const content = protectedContentMarkup(model.content, words);
  const durationLabel = escapeHtml(requireWord(words, "duration_label"));
  const durationOptions = model.durationChoices
    .map((choice) => durationOptionMarkup(choice, choice.seconds === model.durationSeconds, words))
    .join("");

  return `<div class="view-once-overlay voo-open" data-voo-state="open">
    <div class="voo-backdrop" aria-hidden="true"></div>
    <div class="voo-card" role="dialog" aria-modal="true" aria-label="${escapeHtml(requireWord(words, "protected_text_placeholder"))}" data-voo-card>
      <button class="voo-close" type="button" aria-label="${escapeHtml(requireWord(words, "close_aria_label"))}" data-voo-close>&times;</button>
      ${content}
      <div class="voo-controls">
        <label class="voo-duration-label">
          <span>${durationLabel}</span>
          <select class="voo-duration" aria-label="${durationLabel}" data-voo-duration>
            ${durationOptions}
          </select>
        </label>
        <button class="voo-play" type="button" aria-label="${escapeHtml(requireWord(words, "play_aria_label"))}" data-voo-play>&#9654;</button>
      </div>
    </div>
  </div>`;
}
