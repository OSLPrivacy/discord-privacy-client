import { describe, expect, it, vi } from "vitest";
import {
  chooseDaily,
  chooseMonthly,
  chooseOnlyWhenChosen,
  chooseRunNow,
  chooseWeekly,
  commitScheduleChoice,
  scheduleKindForChoice,
  SCHEDULE_PAGE_OPTIONS,
  type SchedulePagePort,
} from "./schedule-page";

// 2026-08-07 12:00:00 UTC is a Friday -- the same fixed clock
// crates/ipc/tests/task_1463_schedule_storage.rs asserts against, so the
// expected `nextRunUnixSecs` values below are independently verifiable.
const NOW = 1_786_104_000;

function replyingPort(schedule: Record<string, unknown>): { port: SchedulePagePort; invoke: ReturnType<typeof vi.fn> } {
  const invoke = vi.fn().mockResolvedValue(
    JSON.stringify({ ok: true, command: "schedule_save", schedule }),
  );
  return { port: { invoke }, invoke };
}

function requestBody(invoke: ReturnType<typeof vi.fn>): Record<string, unknown> {
  const call = invoke.mock.calls[0];
  expect(call[0]).toBe("schedule_save");
  return JSON.parse(call[1].requestJson as string);
}

describe("schedule page options", () => {
  it("shows daily, weekly, monthly, only-when-chosen, and run now", () => {
    expect(SCHEDULE_PAGE_OPTIONS.map(option => option.id)).toEqual([
      "daily",
      "weekly",
      "monthly",
      "only_when_chosen",
      "run_now",
    ]);
  });
});

describe("each visible choice writes its matching schedule type", () => {
  it("daily writes a daily schedule with the chosen time", async () => {
    const { port, invoke } = replyingPort({
      accountId: "acct-1",
      kind: { kind: "daily", hour: 18, minute: 0 },
      nextRunUnixSecs: 1_786_125_600,
    });
    const outcome = await chooseDaily("acct-1", 18, 0, NOW, port);
    expect(requestBody(invoke)).toEqual({
      accountId: "acct-1",
      kind: "daily",
      hour: 18,
      minute: 0,
      nowUnixSecs: NOW,
    });
    expect(outcome.ok).toBe(true);
    expect(outcome.schedule?.nextRunUnixSecs).toBe(1_786_125_600);
  });

  it("weekly writes a weekly schedule with the chosen weekday and time", async () => {
    const { port, invoke } = replyingPort({
      accountId: "acct-2",
      kind: { kind: "weekly", weekday: "mon", hour: 9, minute: 0 },
      nextRunUnixSecs: 1_786_352_400,
    });
    const outcome = await chooseWeekly("acct-2", "mon", 9, 0, NOW, port);
    expect(requestBody(invoke)).toEqual({
      accountId: "acct-2",
      kind: "weekly",
      weekday: "mon",
      hour: 9,
      minute: 0,
      nowUnixSecs: NOW,
    });
    expect(outcome.ok).toBe(true);
    expect(outcome.schedule?.nextRunUnixSecs).toBe(1_786_352_400);
  });

  it("monthly writes a monthly schedule with the chosen day and time", async () => {
    const { port, invoke } = replyingPort({
      accountId: "acct-3",
      kind: { kind: "monthly", day: 20, hour: 0, minute: 0 },
      nextRunUnixSecs: 1_787_184_000,
    });
    const outcome = await chooseMonthly("acct-3", 20, 0, 0, NOW, port);
    expect(requestBody(invoke)).toEqual({
      accountId: "acct-3",
      kind: "monthly",
      day: 20,
      hour: 0,
      minute: 0,
      nowUnixSecs: NOW,
    });
    expect(outcome.ok).toBe(true);
    expect(outcome.schedule?.nextRunUnixSecs).toBe(1_787_184_000);
  });

  it("only-when-chosen writes a schedule with no automatic next run", async () => {
    const { port, invoke } = replyingPort({
      accountId: "acct-4",
      kind: { kind: "only_when_chosen" },
      nextRunUnixSecs: null,
    });
    const outcome = await chooseOnlyWhenChosen("acct-4", NOW, port);
    expect(requestBody(invoke)).toEqual({
      accountId: "acct-4",
      kind: "only_when_chosen",
      nowUnixSecs: NOW,
    });
    expect(outcome.ok).toBe(true);
    expect(outcome.schedule?.nextRunUnixSecs).toBeNull();
  });

  it("run now also writes an only-when-chosen schedule, matching its no-automatic-run meaning", async () => {
    const { port, invoke } = replyingPort({
      accountId: "acct-5",
      kind: { kind: "only_when_chosen" },
      nextRunUnixSecs: null,
    });
    const outcome = await chooseRunNow("acct-5", NOW, port);
    expect(requestBody(invoke)).toEqual({
      accountId: "acct-5",
      kind: "only_when_chosen",
      nowUnixSecs: NOW,
    });
    expect(outcome.ok).toBe(true);
    expect(scheduleKindForChoice({ option: "run_now" })).toEqual({ kind: "only_when_chosen" });
  });
});

describe("validation", () => {
  it("refuses an out-of-range hour before calling the port", async () => {
    const invoke = vi.fn();
    await expect(chooseDaily("acct-1", 24, 0, NOW, { invoke })).rejects.toThrow(/hour/);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("refuses an out-of-range month day before calling the port", async () => {
    const invoke = vi.fn();
    await expect(chooseMonthly("acct-1", 32, 0, 0, NOW, { invoke })).rejects.toThrow(/day/);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("refuses an empty account id before calling the port", async () => {
    const invoke = vi.fn();
    await expect(chooseOnlyWhenChosen("", NOW, { invoke })).rejects.toThrow(/account id/);
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe("outcome parsing surfaces a refusal", () => {
  it("returns ok: false with the error code the store sent back", async () => {
    const invoke = vi.fn().mockResolvedValue(
      JSON.stringify({
        ok: false,
        command: "schedule_save",
        errorCode: "invalid_schedule_time",
        error: "hour must be 0..=23 and minute must be 0..=59",
      }),
    );
    const outcome = await commitScheduleChoice("acct-1", { option: "daily", hour: 12, minute: 0 }, NOW, { invoke });
    expect(outcome.ok).toBe(false);
    expect(outcome.errorCode).toBe("invalid_schedule_time");
  });
});
