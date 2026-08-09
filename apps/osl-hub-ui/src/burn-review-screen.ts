import { escapeHtml } from "./services";

export type BurnReviewSide = "your_side" | "their_side" | "both_sides";

export interface BurnReviewScreenState {
  selectedSide: BurnReviewSide;
  hideOtherPeople: boolean;
  /** Server-wide choices are meaningful only for an active server channel. */
  serverChannel: boolean;
}

export const BURN_REVIEW_SIDE_OPTIONS: ReadonlyArray<{ side: BurnReviewSide; title: string; detail: string }> = [
  { side: "your_side", title: "Your side", detail: "Only what you sent." },
  { side: "their_side", title: "Their side", detail: "Only what they sent." },
  { side: "both_sides", title: "Both sides", detail: "Everything in this chat." },
];

export function initialBurnReviewScreenState(serverChannel = false): BurnReviewScreenState {
  return { selectedSide: "both_sides", hideOtherPeople: false, serverChannel };
}

export function selectBurnReviewSide(state: BurnReviewScreenState, side: BurnReviewSide): BurnReviewScreenState {
  return { ...state, selectedSide: side };
}

export function toggleBurnReviewHideOtherPeople(state: BurnReviewScreenState): BurnReviewScreenState {
  return { ...state, hideOtherPeople: !state.hideOtherPeople };
}

export function burnReviewScreenMarkup(state: BurnReviewScreenState): string {
  const sides = BURN_REVIEW_SIDE_OPTIONS.map((option) =>
    `<button class="burn-scope-card burn-review-side ${state.selectedSide === option.side ? "selected" : ""}" type="button" data-burn-review-side="${option.side}" aria-pressed="${state.selectedSide === option.side}"><strong>${escapeHtml(option.title)}</strong><small>${escapeHtml(option.detail)}</small></button>`,
  ).join("");
  const serverChoices = state.serverChannel
    ? `<fieldset class="burn-review-server-choices"><legend>Server scope</legend><button class="button" type="button" data-burn-review-server-choice="this_channel">This channel</button><button class="button" type="button" data-burn-review-server-choice="whole_server">Whole server</button></fieldset>`
    : "";
  return `<section class="burn-review-screen" id="burn-review-screen" aria-labelledby="burn-review-title"><h1 id="burn-review-title">Review before burn</h1><div class="burn-scope-grid burn-review-sides" role="group" aria-label="Whose side to review">${sides}</div>${serverChoices}<label class="setting-line interactive burn-review-hide-other-people" for="burn-review-hide-other-people"><span><strong>Hide other people</strong><small>Only show messages that involve you in this review.</small></span><input id="burn-review-hide-other-people" type="checkbox" ${state.hideOtherPeople ? "checked" : ""}/></label><footer class="burn-review-actions"><button class="button" id="burn-review-back" type="button">BACK</button></footer></section>`;
}
