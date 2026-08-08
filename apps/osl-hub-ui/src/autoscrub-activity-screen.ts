/**
 * The AutoScrub activity screen: every finished run, in one list.
 *
 * Each row is a picture of TASK 1472's activity record (`run_id`, `account_id`,
 * `text`, `location`, the match/deleted/failed counts) and TASK 1476's notices
 * (which resolve back to the same record through `autoscrub_activity_get`).
 * This module renders that record and nothing more: it never draws a link a
 * reader could follow into the scrubbed conversation itself. Deletion already
 * ran against `record.text`'s summary of what matched -- putting a jump link
 * on top of that summary would let a screen meant to report on a finished,
 * unattended run turn into a second surface for finding the same messages.
 * That is why "no jump links" is a screen-wide rule, not a per-row choice: no
 * row markup here ever contains an `<a`, `href=`, or a "go to" / "view
 * message" control, however the row's `signedIn` state resolves.
 */

/** Renderer-side mirror of TASK 1472's `AutoScrubRunLocation`. */
export type AutoScrubActivityLocation = "local" | "cloud";

/** One finished run, as the activity record (TASK 1472) and notices (TASK 1476) already describe it. */
export interface AutoScrubActivityRun {
  runId: string;
  accountId: string;
  serviceId: string;
  /** The record's human-readable summary, e.g. "acct: matched 11, deleted 7, failed 3 (ran on this device)". */
  text: string;
  location: AutoScrubActivityLocation;
  /** Whether the account was signed in when this run finished. A signed-out account can only be opened, never resumed here. */
  signedIn: boolean;
}

/** The three actions a row can offer. Turn off always applies to the whole schedule, not one run. */
export type AutoScrubActivityRowActionKind = "openAccount" | "skipAccount" | "turnOff";

export interface AutoScrubActivityRowAction {
  readonly action: AutoScrubActivityRowActionKind;
  readonly label: "Open account" | "Skip account" | "Turn off";
  readonly accountId: string | null;
}

const LOCATION_LABEL: Readonly<Record<AutoScrubActivityLocation, string>> = {
  local: "Ran on this device",
  cloud: "Ran in the cloud",
};

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** Every row always offers the same three actions; only the target account id changes. */
export function autoScrubActivityRowActions(run: AutoScrubActivityRun): AutoScrubActivityRowAction[] {
  return [
    { action: "openAccount", label: "Open account", accountId: run.accountId },
    { action: "skipAccount", label: "Skip account", accountId: run.accountId },
    { action: "turnOff", label: "Turn off", accountId: null },
  ];
}

/** Which account "Open account" opens for a given run, or null if the action does not target this run. */
export function openAccountTarget(
  run: AutoScrubActivityRun,
  action: AutoScrubActivityRowActionKind,
): string | null {
  if (action !== "openAccount") return null;
  return run.accountId;
}

function actionButton(action: AutoScrubActivityRowAction): string {
  const attr = action.accountId === null
    ? `data-autoscrub-activity-action="${action.action}"`
    : `data-autoscrub-activity-action="${action.action}" data-autoscrub-activity-account="${escapeHtml(action.accountId)}"`;
  return `<button type="button" class="autoscrub-activity-action" ${attr}>${escapeHtml(action.label)}</button>`;
}

function rowMarkup(run: AutoScrubActivityRun): string {
  const signedInMark = run.signedIn
    ? `<span class="autoscrub-activity-signed-in" data-signed-in="true">Signed in</span>`
    : `<span class="autoscrub-activity-signed-in" data-signed-in="false">Signed out &mdash; sign in yourself to continue</span>`;
  return [
    `<li class="autoscrub-activity-row" data-run-id="${escapeHtml(run.runId)}" data-account-id="${escapeHtml(run.accountId)}">`,
    `<div class="autoscrub-activity-row-facts">`,
    `<p class="autoscrub-activity-row-text">${escapeHtml(run.text)}</p>`,
    `<p class="autoscrub-activity-row-location">${escapeHtml(LOCATION_LABEL[run.location])}</p>`,
    signedInMark,
    `</div>`,
    `<div class="autoscrub-activity-row-actions">${autoScrubActivityRowActions(run).map(actionButton).join("")}</div>`,
    `</li>`,
  ].join("");
}

function countLabel(total: number): string {
  if (total === 0) return "No AutoScrub runs yet";
  return total === 1 ? "1 AutoScrub run" : `${total} AutoScrub runs`;
}

/**
 * The whole activity list, every run in the order given. Styling lives in
 * `autoscrub-activity-screen.css`; the shipped CSP is `style-src 'self'`, so
 * nothing here uses an inline `style` attribute.
 */
export function renderAutoScrubActivityScreen(runs: readonly AutoScrubActivityRun[]): string {
  const body = runs.length === 0
    ? `<p class="autoscrub-activity-empty">Nothing has run yet.</p>`
    : `<ul class="autoscrub-activity-rows">${runs.map(rowMarkup).join("")}</ul>`;
  return [
    `<section class="autoscrub-activity" aria-label="AutoScrub activity">`,
    `<h2 class="autoscrub-activity-heading">AutoScrub activity</h2>`,
    `<p class="autoscrub-activity-count" role="status">${escapeHtml(countLabel(runs.length))}</p>`,
    body,
    `</section>`,
  ].join("");
}
