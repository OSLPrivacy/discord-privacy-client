// TASK 1438. 1437 (apps/osl-hub/src/services.rs, finish_active_service_account_run_notice)
// gives every finished-scrub notice an id of the shape "scrub-finished-{run_id}" and never
// invents a run beyond that. This module is the UI-side connection: activating a notice opens
// exactly the run whose id it names, and nothing else. If no run with that id is on screen, the
// notice opens zero runs and carries a reason a screen can show instead of a result.

/** Mirrors `FinishedServiceAccountRunNotice` (apps/osl-hub/src/services.rs), serde camelCase. */
export interface ScrubFinishedNotice {
  id: string;
  service: string;
  finishedAccount: string;
  nextAccount: string | null;
  matchCount: number;
  title: string;
  detail: string;
}

/** One run's results, as shown by whatever screen renders a finished scan. */
export interface ScrubRunResults {
  runId: string;
  service: string;
  account: string;
  matchCount: number;
}

export interface OpenedNoticeResults {
  /** The run this notice named, or empty when it could not be opened. Never more than one. */
  opened: readonly ScrubRunResults[];
  /** Set exactly when `opened` is empty; explains why nothing opened. */
  reason: string | null;
}

const NOTICE_ID_PREFIX = "scrub-finished-";

/** Extracts the run id a notice names, or null if the id is not shaped like a finished-scrub notice. */
export function runIdFromNoticeId(noticeId: string): string | null {
  if (!noticeId.startsWith(NOTICE_ID_PREFIX)) return null;
  const runId = noticeId.slice(NOTICE_ID_PREFIX.length);
  return runId.length > 0 ? runId : null;
}

/**
 * Activating a notice opens exactly the run whose id it names: one match in, one match out.
 * An id that names no known run, or that is not shaped like a finished-scrub notice at all,
 * opens zero runs and reports why instead.
 */
export function openNoticeResults(
  notice: ScrubFinishedNotice,
  runs: readonly ScrubRunResults[],
): OpenedNoticeResults {
  const runId = runIdFromNoticeId(notice.id);
  if (runId === null) {
    return { opened: [], reason: `Notice "${notice.id}" does not name a run` };
  }

  const match = runs.find((run) => run.runId === runId);
  if (!match) {
    return { opened: [], reason: `No results found for run "${runId}"` };
  }

  return { opened: [match], reason: null };
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Renders the outcome of activating one notice: either the opened run or the reason it did not open. */
export function noticeResultsMarkup(opened: OpenedNoticeResults): string {
  if (opened.reason !== null) {
    return `<section class="scrub-notice-results" role="alert"><p class="scrub-notice-results-reason">${escapeHtml(opened.reason)}</p></section>`;
  }
  const run = opened.opened[0];
  return `<section class="scrub-notice-results" data-scrub-notice-results-run="${escapeHtml(run.runId)}"><h2>${escapeHtml(run.service)} results</h2><p>${escapeHtml(run.account)}: ${run.matchCount} ${run.matchCount === 1 ? "match" : "matches"}</p></section>`;
}
