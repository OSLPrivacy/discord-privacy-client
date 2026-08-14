import { invoke } from "@tauri-apps/api/core";

/**
 * TASK 1465: connect schedule page.
 *
 * Wire format matches `crates/ipc/src/schedule_storage.rs` (TASK 1463)
 * exactly: `schedule_save` with `{ accountId, kind, ...kindFields,
 * nowUnixSecs }`, where `kind` is one of `daily` | `weekly` | `monthly` |
 * `only_when_chosen`. That module has no execution logic of its own (it only
 * stores the choice and computes the next run) -- an account with no
 * automatic schedule fires only on an explicit "Run now" (TASK 1473), which
 * is why the "Run now" choice below writes an `only_when_chosen` schedule:
 * choosing it is how a user says "don't run this on its own."
 */

export type Weekday = "sun" | "mon" | "tue" | "wed" | "thu" | "fri" | "sat";

export type SchedulePageOptionId = "daily" | "weekly" | "monthly" | "only_when_chosen" | "run_now";

export interface SchedulePageOption {
  readonly id: SchedulePageOptionId;
  readonly label: string;
}

/** The visible choices this page renders, in display order. */
export const SCHEDULE_PAGE_OPTIONS: readonly SchedulePageOption[] = Object.freeze([
  { id: "daily", label: "Daily" },
  { id: "weekly", label: "Weekly" },
  { id: "monthly", label: "Monthly" },
  { id: "only_when_chosen", label: "Only when I choose" },
  { id: "run_now", label: "Run now" },
]);

export type ScheduleChoice =
  | { readonly option: "daily"; readonly hour: number; readonly minute: number }
  | { readonly option: "weekly"; readonly weekday: Weekday; readonly hour: number; readonly minute: number }
  | { readonly option: "monthly"; readonly day: number; readonly hour: number; readonly minute: number }
  | { readonly option: "only_when_chosen" }
  | { readonly option: "run_now" };

export type ScheduleKind =
  | { readonly kind: "daily"; readonly hour: number; readonly minute: number }
  | { readonly kind: "weekly"; readonly weekday: Weekday; readonly hour: number; readonly minute: number }
  | { readonly kind: "monthly"; readonly day: number; readonly hour: number; readonly minute: number }
  | { readonly kind: "only_when_chosen" };

export interface ScheduleRecord {
  readonly accountId: string;
  readonly kind: ScheduleKind;
  readonly nextRunUnixSecs: number | null;
}

export interface ScheduleSaveOutcome {
  readonly ok: boolean;
  readonly command: string;
  readonly schedule?: ScheduleRecord;
  readonly errorCode?: string;
  readonly error?: string;
}

export interface SchedulePagePort {
  invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
}

const productionPort: SchedulePagePort = {
  invoke: (command, args) => invoke(command, args),
};

const SCHEDULE_SAVE_COMMAND = "schedule_save";

/**
 * Turns a visible page choice into the `ScheduleKind` `schedule_storage`
 * expects. "Run now" carries no recurrence of its own, so it maps to the
 * same `only_when_chosen` kind the "Only when I choose" radio writes.
 */
export function scheduleKindForChoice(choice: ScheduleChoice): ScheduleKind {
  switch (choice.option) {
    case "daily":
      return { kind: "daily", hour: choice.hour, minute: choice.minute };
    case "weekly":
      return { kind: "weekly", weekday: choice.weekday, hour: choice.hour, minute: choice.minute };
    case "monthly":
      return { kind: "monthly", day: choice.day, hour: choice.hour, minute: choice.minute };
    case "only_when_chosen":
    case "run_now":
      return { kind: "only_when_chosen" };
  }
}

/**
 * Commits whichever choice is visibly selected on the schedule page,
 * writing the matching schedule type through the `schedule_save`
 * direct-invoke command.
 */
export async function commitScheduleChoice(
  accountId: string,
  choice: ScheduleChoice,
  nowUnixSecs: number,
  port: SchedulePagePort = productionPort,
): Promise<ScheduleSaveOutcome> {
  validateAccountId(accountId);
  validateNow(nowUnixSecs);
  const kind = scheduleKindForChoice(choice);
  validateKind(kind);
  const request: Record<string, unknown> = {
    accountId,
    kind: kind.kind,
    nowUnixSecs,
  };
  if (kind.kind === "daily") {
    request.hour = kind.hour;
    request.minute = kind.minute;
  } else if (kind.kind === "weekly") {
    request.weekday = kind.weekday;
    request.hour = kind.hour;
    request.minute = kind.minute;
  } else if (kind.kind === "monthly") {
    request.day = kind.day;
    request.hour = kind.hour;
    request.minute = kind.minute;
  }
  // `schedule_storage::run_schedule_command` takes `request_json: &str`; the
  // eventual `#[tauri::command]` wrapper takes the same shape, which Tauri
  // exposes to JS as the camelCase `requestJson`.
  return parseOutcome(await port.invoke(SCHEDULE_SAVE_COMMAND, { requestJson: JSON.stringify(request) }));
}

export const chooseDaily = (
  accountId: string,
  hour: number,
  minute: number,
  nowUnixSecs: number,
  port?: SchedulePagePort,
): Promise<ScheduleSaveOutcome> => commitScheduleChoice(accountId, { option: "daily", hour, minute }, nowUnixSecs, port);

export const chooseWeekly = (
  accountId: string,
  weekday: Weekday,
  hour: number,
  minute: number,
  nowUnixSecs: number,
  port?: SchedulePagePort,
): Promise<ScheduleSaveOutcome> =>
  commitScheduleChoice(accountId, { option: "weekly", weekday, hour, minute }, nowUnixSecs, port);

export const chooseMonthly = (
  accountId: string,
  day: number,
  hour: number,
  minute: number,
  nowUnixSecs: number,
  port?: SchedulePagePort,
): Promise<ScheduleSaveOutcome> =>
  commitScheduleChoice(accountId, { option: "monthly", day, hour, minute }, nowUnixSecs, port);

export const chooseOnlyWhenChosen = (
  accountId: string,
  nowUnixSecs: number,
  port?: SchedulePagePort,
): Promise<ScheduleSaveOutcome> => commitScheduleChoice(accountId, { option: "only_when_chosen" }, nowUnixSecs, port);

export const chooseRunNow = (
  accountId: string,
  nowUnixSecs: number,
  port?: SchedulePagePort,
): Promise<ScheduleSaveOutcome> => commitScheduleChoice(accountId, { option: "run_now" }, nowUnixSecs, port);

function validateAccountId(accountId: string): void {
  const bytes = new TextEncoder().encode(accountId).length;
  if (bytes < 1 || bytes > 128) {
    throw new Error("account id must be 1..=128 bytes");
  }
}

function validateNow(nowUnixSecs: number): void {
  if (!Number.isSafeInteger(nowUnixSecs) || nowUnixSecs < 0) {
    throw new Error("nowUnixSecs must be a non-negative integer");
  }
}

function validateKind(kind: ScheduleKind): void {
  if (kind.kind === "daily" || kind.kind === "weekly" || kind.kind === "monthly") {
    if (!Number.isInteger(kind.hour) || kind.hour < 0 || kind.hour > 23) {
      throw new Error("hour must be 0..=23");
    }
    if (!Number.isInteger(kind.minute) || kind.minute < 0 || kind.minute > 59) {
      throw new Error("minute must be 0..=59");
    }
  }
  if (kind.kind === "monthly" && (!Number.isInteger(kind.day) || kind.day < 1 || kind.day > 31)) {
    throw new Error("day must be 1..=31");
  }
}

function parseOutcome(raw: unknown): ScheduleSaveOutcome {
  if (typeof raw === "string") {
    return parseOutcome(JSON.parse(raw));
  }
  if (typeof raw !== "object" || raw === null || !("ok" in raw) || !("command" in raw)) {
    throw new Error("invalid schedule_save response");
  }
  return raw as ScheduleSaveOutcome;
}
