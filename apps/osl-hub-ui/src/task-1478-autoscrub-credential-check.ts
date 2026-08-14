import { readFileSync } from "node:fs";
import {
  AUTOSCRUB_SIGN_IN_YOURSELF,
  runDueAutoScrubAccount,
  type AutoScrubDueRun,
  type AutoScrubDueRunSession,
} from "./autoscrub-credential-boundary";
import { renderAutoScrubActivityScreen } from "./autoscrub-activity-screen";

export interface Task1478Fixture {
  readonly dueRun: AutoScrubDueRun;
  readonly nowUnixSeconds: number;
  readonly logout: true;
  readonly password: string;
  readonly code: string;
  readonly humanCheck: string;
}

export interface Task1478SubmissionCounts {
  readonly credentialReads: number;
  readonly credentialSubmits: number;
  readonly humanCheckSubmits: number;
}

export interface Task1478Proof extends Task1478SubmissionCounts {
  readonly fixture: Task1478Fixture;
  readonly status: "paused";
  readonly activityText: typeof AUTOSCRUB_SIGN_IN_YOURSELF;
  readonly renderedActivity: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonemptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function parseFixture(value: unknown, pathname: string): Task1478Fixture {
  if (!isRecord(value)) {
    throw new Error(`TASK1478 invalid fixture object: ${pathname}`);
  }

  const missing: string[] = [];
  if (value.logout !== true) missing.push("logout");
  if (!nonemptyString(value.password)) missing.push("password");
  if (!nonemptyString(value.code)) missing.push("code");
  if (!nonemptyString(value.humanCheck)) missing.push("human-check");
  if (missing.length > 0) {
    throw new Error(`TASK1478 fixture missing required data: ${missing.join(", ")}`);
  }

  if (!Number.isSafeInteger(value.nowUnixSeconds)) {
    throw new Error(`TASK1478 fixture needs an integer nowUnixSeconds: ${pathname}`);
  }
  if (!isRecord(value.dueRun)) {
    throw new Error(`TASK1478 fixture needs a dueRun: ${pathname}`);
  }

  const dueRun = value.dueRun;
  if (
    !nonemptyString(dueRun.runId)
    || !nonemptyString(dueRun.accountId)
    || !nonemptyString(dueRun.serviceId)
    || !Number.isSafeInteger(dueRun.nextRunUnixSeconds)
    || (dueRun.location !== "local" && dueRun.location !== "cloud")
  ) {
    throw new Error(`TASK1478 fixture has an invalid dueRun: ${pathname}`);
  }
  if ((dueRun.nextRunUnixSeconds as number) > (value.nowUnixSeconds as number)) {
    throw new Error(`TASK1478 fixture run is not due: ${pathname}`);
  }

  return {
    dueRun: {
      runId: dueRun.runId,
      accountId: dueRun.accountId,
      serviceId: dueRun.serviceId,
      nextRunUnixSeconds: dueRun.nextRunUnixSeconds as number,
      location: dueRun.location,
    },
    nowUnixSeconds: value.nowUnixSeconds as number,
    logout: true,
    password: value.password as string,
    code: value.code as string,
    humanCheck: value.humanCheck as string,
  };
}

/**
 * Adversarial connection fixture. The due runner receives this object only as
 * `AutoScrubDueRunSession`, so its forbidden methods remain unreachable to
 * the production boundary. Counters make accidental widening observable.
 */
class CredentialAndCheckFixtureSession implements AutoScrubDueRunSession {
  private credentialReadCount = 0;
  private credentialSubmitCount = 0;
  private humanCheckSubmitCount = 0;

  constructor(private readonly fixture: Task1478Fixture) {}

  isLoggedOut(): boolean {
    return this.fixture.logout;
  }

  readPasswordOrCode(): string {
    this.credentialReadCount += 1;
    return `${this.fixture.password}:${this.fixture.code}`;
  }

  submitPasswordOrCode(_value: string): void {
    this.credentialSubmitCount += 1;
  }

  submitHumanCheck(_answer: string): void {
    this.humanCheckSubmitCount += 1;
  }

  counts(): Task1478SubmissionCounts {
    return {
      credentialReads: this.credentialReadCount,
      credentialSubmits: this.credentialSubmitCount,
      humanCheckSubmits: this.humanCheckSubmitCount,
    };
  }
}

export function loadTask1478Fixture(pathname: string): Task1478Fixture {
  const value: unknown = JSON.parse(readFileSync(pathname, "utf8"));
  return parseFixture(value, pathname);
}

/** Run the direct finish-line check against the named fixture. */
export function checkTask1478Fixture(pathname: string): Task1478Proof {
  const fixture = loadTask1478Fixture(pathname);
  const session = new CredentialAndCheckFixtureSession(fixture);
  const result = runDueAutoScrubAccount(fixture.dueRun, fixture.nowUnixSeconds, session);
  const counts = session.counts();

  if (result.status !== "paused" || result.activity.text !== AUTOSCRUB_SIGN_IN_YOURSELF) {
    throw new Error("TASK1478 due logout did not pause with sign in yourself activity");
  }
  if (counts.credentialReads !== 0 || counts.credentialSubmits !== 0 || counts.humanCheckSubmits !== 0) {
    throw new Error("TASK1478 due logout touched a credential or submitted a check");
  }

  const renderedActivity = renderAutoScrubActivityScreen([result.activity]);
  if (!renderedActivity.includes(AUTOSCRUB_SIGN_IN_YOURSELF)) {
    throw new Error("TASK1478 activity screen did not say sign in yourself");
  }

  return {
    fixture,
    status: "paused",
    activityText: AUTOSCRUB_SIGN_IN_YOURSELF,
    renderedActivity,
    ...counts,
  };
}
