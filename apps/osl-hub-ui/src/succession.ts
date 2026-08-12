/**
 * TASK 6100 — succession ownership-transfer disclosure.
 *
 * The period and successor are owner-configurable values supplied by the native
 * side. The UI only renders the sentence; it never invents the values.
 */

export interface SuccessionPlan {
  readonly period: string;
  readonly successor: string;
}

/** The exact TASK 6100 succession warning with resolved placeholders. */
export function successionWarningSentence(period: string, successor: string): string {
  return `After ${period} without a successfully authenticated foreground owner action, ownership transfers automatically to ${successor} and you may lose owner access. Background sync does not reset this timer.`;
}

/** Default fixture used when no native plan has been loaded yet. */
export function defaultSuccessionPlan(): SuccessionPlan {
  return { period: "30 days", successor: "Recovery contact" };
}

export function successionSetupMarkup(plan: SuccessionPlan): string {
  return `<section class="succession-setup" aria-labelledby="succession-setup-title"><h1 id="succession-setup-title">Succession</h1><p>${successionWarningSentence(plan.period, plan.successor)}</p></section>`;
}

export function successionReviewMarkup(plan: SuccessionPlan): string {
  return `<section class="succession-review" aria-labelledby="succession-review-title"><h1 id="succession-review-title">Review succession</h1><p>${successionWarningSentence(plan.period, plan.successor)}</p></section>`;
}
