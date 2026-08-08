import type { AutoScrubActivityRun } from "./autoscrub-activity-screen";

/**
 * Fixed fixture runs for TASK 1477's activity screen check.
 *
 * Three rows on purpose: a signed-in local run, a signed-in cloud run, and a
 * signed-out local run, so the fixture proves the signed-out row still gets
 * its own "Open account" target and still renders no jump link, without that
 * being the only row on the screen.
 */
export const AUTOSCRUB_ACTIVITY_SCREEN_RUNS: readonly AutoScrubActivityRun[] = [
  {
    runId: "run-1477-alpha",
    accountId: "discord-account-alpha-1477",
    serviceId: "discord",
    text: "discord-account-alpha-1477: matched 9, deleted 9, failed 0 (ran on this device)",
    location: "local",
    signedIn: true,
  },
  {
    runId: "run-1477-beta",
    accountId: "telegram-account-beta-1477",
    serviceId: "telegram",
    text: "telegram-account-beta-1477: matched 11, deleted 7, failed 3 (ran in the cloud)",
    location: "cloud",
    signedIn: true,
  },
  {
    runId: "run-1477-gamma",
    accountId: "email-account-gamma-1477",
    serviceId: "email",
    text: "email-account-gamma-1477: matched 0, deleted 0, failed 0 (ran on this device)",
    location: "local",
    signedIn: false,
  },
] as const;

/** The one fixture row that is logged out, for the finish-line check. */
export const AUTOSCRUB_ACTIVITY_SCREEN_LOGGED_OUT_RUN = AUTOSCRUB_ACTIVITY_SCREEN_RUNS[2];
