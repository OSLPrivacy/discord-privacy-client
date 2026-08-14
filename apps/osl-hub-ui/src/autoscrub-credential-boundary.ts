import type { AutoScrubActivityLocation, AutoScrubActivityRun } from "./autoscrub-activity-screen";

/** The only recovery instruction an unattended run may record for a logged-out account. */
export const AUTOSCRUB_SIGN_IN_YOURSELF = "sign in yourself";

export interface AutoScrubDueRun {
  readonly runId: string;
  readonly accountId: string;
  readonly serviceId: string;
  readonly nextRunUnixSeconds: number;
  readonly location: AutoScrubActivityLocation;
}

/**
 * Deliberately narrow session view available to the due runner.
 *
 * Passwords, verification codes, and human checks are not part of this
 * interface. A connection object may hold those values, but the unattended
 * runner has no typed route to read or submit them.
 */
export interface AutoScrubDueRunSession {
  isLoggedOut(): boolean;
}

export type AutoScrubDueRunResult =
  | { readonly status: "not_due"; readonly activity: null }
  | { readonly status: "ready"; readonly activity: null }
  | { readonly status: "paused"; readonly activity: AutoScrubActivityRun };

/**
 * Apply the credential boundary before a due account can scan or delete.
 * A logged-out run becomes an activity row and stops at this boundary.
 */
export function runDueAutoScrubAccount(
  run: AutoScrubDueRun,
  nowUnixSeconds: number,
  session: AutoScrubDueRunSession,
): AutoScrubDueRunResult {
  if (run.nextRunUnixSeconds > nowUnixSeconds) {
    return { status: "not_due", activity: null };
  }

  if (!session.isLoggedOut()) {
    return { status: "ready", activity: null };
  }

  return {
    status: "paused",
    activity: {
      runId: run.runId,
      accountId: run.accountId,
      serviceId: run.serviceId,
      text: AUTOSCRUB_SIGN_IN_YOURSELF,
      location: run.location,
      signedIn: false,
    },
  };
}
