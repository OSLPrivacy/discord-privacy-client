/** Home's compact AutoScrub status is deliberately driven by a saved record. */
export interface AutoScrubHomeActivityRecord {
  readonly id: string;
  readonly accountId: string;
  readonly summary: string;
  readonly finishedAt: string;
}

export interface AutoScrubHomeAccount {
  readonly accountId: string;
  readonly nextRun: string;
  readonly paused: boolean;
  readonly pauseReason: string | null;
  readonly lastRun: AutoScrubHomeActivityRecord;
}

/** Mirrors Task 1473's backend-owned pause activity detail exactly. */
export const AUTOSCRUB_HOME_PAUSE_REASON = "Schedule paused; next run unchanged.";

/** A paused schedule and its saved Task 1472-style activity record. */
export const AUTOSCRUB_HOME_PAUSED_FIXTURE: AutoScrubHomeAccount = {
  accountId: "discord-maple",
  nextRun: "Aug 7, 2026, 6:00 PM UTC",
  paused: true,
  pauseReason: AUTOSCRUB_HOME_PAUSE_REASON,
  lastRun: {
    id: "run-1474-discord-maple",
    accountId: "discord-maple",
    summary: "discord-maple: matched 11, deleted 7, failed 3 (ran on this device)",
    finishedAt: "Aug 7, 2026, 5:44 PM UTC",
  },
};

function escapeHtml(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&#39;");
}

export function autoScrubHomeActivityMarkup(account: AutoScrubHomeAccount = AUTOSCRUB_HOME_PAUSED_FIXTURE): string {
  const paused = account.paused
    ? `<p class="autoscrub-home-paused" data-autoscrub-paused-account="${escapeHtml(account.accountId)}"><strong>Paused: ${escapeHtml(account.accountId)}</strong><small>${escapeHtml(account.pauseReason ?? "")}</small></p>`
    : "";
  return `<section class="autoscrub-home-activity" aria-label="AutoScrub activity"><header><div><h2>AutoScrub</h2><p>Next run: ${escapeHtml(account.nextRun)}</p></div><div class="autoscrub-home-controls"><button type="button" data-autoscrub-home-control="run-now">Run now</button><button type="button" data-autoscrub-home-control="pause">Pause</button><button type="button" data-autoscrub-home-control="resume">Resume</button><button type="button" data-autoscrub-home-control="stop-and-turn-off">Stop and turn off</button></div></header>${paused}<div class="autoscrub-home-last-run"><strong>Last run</strong><small>${escapeHtml(account.lastRun.summary)} · ${escapeHtml(account.lastRun.finishedAt)}</small><button type="button" class="button compact" data-autoscrub-home-view-activity="${escapeHtml(account.lastRun.id)}" data-autoscrub-home-account="${escapeHtml(account.accountId)}">View activity</button></div></section>`;
}

/** A View activity action can only open the record bound to that account. */
export function openAutoScrubHomeActivity(recordId: string, account: AutoScrubHomeAccount = AUTOSCRUB_HOME_PAUSED_FIXTURE): AutoScrubHomeActivityRecord | null {
  return recordId === account.lastRun.id && account.lastRun.accountId === account.accountId ? account.lastRun : null;
}

export function autoScrubActivityRecordMarkup(record: AutoScrubHomeActivityRecord | null): string {
  if (!record) return "";
  return `<section class="autoscrub-opened-activity-record" data-autoscrub-activity-record-id="${escapeHtml(record.id)}" data-autoscrub-activity-account="${escapeHtml(record.accountId)}"><h2>AutoScrub activity</h2><p>${escapeHtml(record.summary)}</p><small>Finished ${escapeHtml(record.finishedAt)}</small></section>`;
}
