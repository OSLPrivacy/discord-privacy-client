import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  bindScrubSetupControls,
  initialScrubSetupDraft,
  scrubSetupMarkup,
  type ScrubSetupDraft,
  type ScrubSetupSummary,
} from "./scrub-setup";

type Listener = () => void | Promise<void>;

class FakeControl {
  checked: boolean;
  disabled = false;
  readonly value: string;
  private readonly listeners = new Map<string, Listener[]>();

  constructor(value = "", checked = false) {
    this.value = value;
    this.checked = checked;
  }

  addEventListener(type: string, listener: Listener): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }

  async dispatch(type: string): Promise<void> {
    for (const listener of this.listeners.get(type) ?? []) await listener();
  }
}

class FakeRoot {
  constructor(private readonly selectors: Record<string, FakeControl[]>) {}

  querySelector<T>(selector: string): T | null {
    return (this.selectors[selector]?.[0] ?? null) as T | null;
  }

  querySelectorAll<T>(selector: string): T[] {
    return (this.selectors[selector] ?? []) as T[];
  }
}

function controls() {
  const discord = new FakeControl("discord:current-session");
  const telegram = new FakeControl("telegram:current-session");
  const autoScrub = new FakeControl("", true);
  const daily = new FakeControl("daily");
  const weekly = new FakeControl("weekly", true);
  const monthly = new FakeControl("monthly");
  const before = new FakeControl("before_scan", true);
  const after = new FakeControl("after_scan");
  const quiet = new FakeControl("quiet");
  const scanNow = new FakeControl();
  const continueButton = new FakeControl();
  const notNow = new FakeControl();
  const back = new FakeControl();
  const root = new FakeRoot({
    'input[name="scrub-setup-account"]': [discord, telegram],
    "#enable-autoscrub": [autoScrub],
    'input[name="scrub-setup-frequency"]': [daily, weekly, monthly],
    'input[name="scrub-setup-notice"]': [before, after, quiet],
    "#scan-now-scrub-setup": [scanNow],
    "#continue-scrub-setup": [continueButton],
    "#not-now-scrub-setup": [notNow],
    "#back-scrub-setup": [back],
  });
  return { root, discord, telegram, autoScrub, daily, weekly, monthly, before, after, quiet, scanNow, continueButton, notNow, back };
}

describe("TASK 0321 Scrub setup controls", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("connects account ticks, Scan now, AutoScrub, frequency, notices, Continue, and Back", async () => {
    const ui = controls();
    const draft = initialScrubSetupDraft();
    const scanNow = vi.fn();
    const continued: Array<{ summary: ScrubSetupSummary; draft: ScrubSetupDraft }> = [];
    const back = vi.fn();
    const errors = vi.fn();
    mocks.invoke.mockResolvedValue({ accountCount: 2, automaticSchedule: "weekly", noticeSetting: "before_scan" });

    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: scanNow,
      onContinue: (summary, visibleDraft) => continued.push({ summary, draft: visibleDraft }),
      onNotNow: vi.fn(),
      onBack: back,
      onError: errors,
    });

    expect(ui.scanNow.disabled).toBe(true);
    expect(ui.continueButton.disabled).toBe(true);
    ui.discord.checked = true;
    await ui.discord.dispatch("change");
    ui.telegram.checked = true;
    await ui.telegram.dispatch("change");
    expect(ui.scanNow.disabled).toBe(false);
    expect(ui.continueButton.disabled).toBe(false);

    await ui.scanNow.dispatch("click");
    expect(scanNow).toHaveBeenCalledWith(["discord:current-session", "telegram:current-session"]);

    ui.autoScrub.checked = false;
    await ui.autoScrub.dispatch("change");
    expect(ui.continueButton.disabled).toBe(true);
    expect(ui.weekly.disabled).toBe(true);
    ui.autoScrub.checked = true;
    await ui.autoScrub.dispatch("change");

    ui.monthly.checked = true;
    await ui.monthly.dispatch("change");
    ui.weekly.checked = true;
    await ui.weekly.dispatch("change");
    ui.after.checked = true;
    await ui.after.dispatch("change");
    ui.before.checked = true;
    await ui.before.dispatch("change");

    await ui.continueButton.dispatch("click");
    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("save_scrub_setup", {
      command: {
        selectedScanAccounts: ["discord:current-session", "telegram:current-session"],
        automaticSchedule: "weekly",
        noticeSetting: "before_scan",
        notNow: false,
      },
    });
    expect(continued).toHaveLength(1);
    expect(continued[0].summary).toEqual({ accountCount: 2, automaticSchedule: "weekly", noticeSetting: "before_scan" });
    expect([...continued[0].draft.selectedScanAccounts]).toEqual(["discord:current-session", "telegram:current-session"]);
    expect(continued[0].draft).toMatchObject({ autoScrub: true, automaticSchedule: "weekly", noticeSetting: "before_scan" });
    expect(errors).not.toHaveBeenCalled();

    await ui.back.dispatch("click");
    expect(back).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledTimes(1);

    console.info("TASK0321_SCAN_NOW_ACCOUNT_COUNT=2");
    console.info("TASK0321_CONTINUE_ACCOUNT_COUNT=2");
    console.info("TASK0321_CONTINUE_SCHEDULE=weekly");
    console.info("TASK0321_CONTINUE_NOTICE=before_scan");
    console.info("TASK0321_BACK_SAVE_COUNT=0");
  });

  it("Not now stores no scan accounts and no automatic choices", async () => {
    const ui = controls();
    const draft = initialScrubSetupDraft();
    draft.selectedScanAccounts = new Set(["discord:current-session", "telegram:current-session"]);
    const skipped: ScrubSetupSummary[] = [];
    mocks.invoke.mockResolvedValue({ accountCount: 0, automaticSchedule: null, noticeSetting: null });

    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: vi.fn(),
      onContinue: vi.fn(),
      onNotNow: (summary) => skipped.push(summary),
      onBack: vi.fn(),
    });
    await ui.notNow.dispatch("click");

    expect(mocks.invoke).toHaveBeenCalledWith("save_scrub_setup", {
      command: {
        selectedScanAccounts: [],
        automaticSchedule: null,
        noticeSetting: null,
        notNow: true,
      },
    });
    expect(skipped).toEqual([{ accountCount: 0, automaticSchedule: null, noticeSetting: null }]);
    console.info("TASK0321_NOT_NOW_ACCOUNT_COUNT=0");
    console.info("TASK0321_NOT_NOW_SCHEDULE=none");
    console.info("TASK0321_NOT_NOW_NOTICE=none");
  });

  it("does not continue when the native summary disagrees with the visible controls", async () => {
    const ui = controls();
    ui.discord.checked = true;
    ui.telegram.checked = true;
    const draft = initialScrubSetupDraft();
    const continued = vi.fn();
    const errors = vi.fn();
    mocks.invoke.mockResolvedValue({ accountCount: 1, automaticSchedule: "weekly", noticeSetting: "before_scan" });
    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: vi.fn(),
      onContinue: continued,
      onNotNow: vi.fn(),
      onBack: vi.fn(),
      onError: errors,
    });
    await ui.discord.dispatch("change");
    await ui.telegram.dispatch("change");
    await ui.continueButton.dispatch("click");

    expect(continued).not.toHaveBeenCalled();
    expect(errors).toHaveBeenCalledWith("Scrub setup did not store the visible choices");
  });

  it("renders every named control with safe defaults", () => {
    const markup = scrubSetupMarkup([
      { id: "discord:current-session", label: "Discord", detail: "Current session" },
      { id: "telegram:current-session", label: "Telegram", detail: "Current session" },
    ], initialScrubSetupDraft());
    expect(markup).toContain("Scrub and AutoScrub");
    expect(markup).toContain("Accounts");
    expect(markup).toContain("Scan now");
    expect(markup).toContain("AutoScrub");
    expect(markup).toContain("Frequency");
    expect(markup).toContain("Notices");
    expect(markup).toContain("Continue");
    expect(markup).toContain("Not now");
    expect(markup).toContain("Back");
    expect(markup).toContain('value="weekly" checked');
    expect(markup).toContain('value="before_scan" checked');
  });
});
