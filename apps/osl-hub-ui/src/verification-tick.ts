// The two-way verification tick. The backend compares both direct-message
// whitelist directions between two people and reports verificationState as
// "visible" only when both exist ("two-way"). Anything else — no state loaded,
// "none", "one-way", or a report that contradicts itself — draws nothing, so
// the tick can never overstate trust.
export interface AllowedPlaceDirectionStateModel {
  /** `none`, `one-way`, or `two-way`. */
  state: string;
  /** `visible` only when the reciprocal direct-message whitelist exists. */
  verificationState: string;
  savedDirections: number;
  firstToSecond: boolean;
  secondToFirst: boolean;
}

export function verificationTickVisible(model: AllowedPlaceDirectionStateModel | null): boolean {
  return model !== null && model.state === "two-way" && model.verificationState === "visible";
}

export function verificationTickMarkup(model: AllowedPlaceDirectionStateModel | null): string {
  if (!verificationTickVisible(model)) return "";
  return `<span class="verification-tick" data-verification-tick="two-way" role="img" aria-label="Verified both ways">✓</span>`;
}
