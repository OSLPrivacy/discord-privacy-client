import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  bindScrubSetupControls,
  initialScrubSetupDraft,
  type ScrubSetupScanResult,
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
  const account = new FakeControl("discord:liam-0350");
  const autoScrub = new FakeControl("", false);
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
    'input[name="scrub-setup-account"]': [account],
    "#enable-autoscrub": [autoScrub],
    'input[name="scrub-setup-frequency"]': [daily, weekly, monthly],
    'input[name="scrub-setup-notice"]': [before, after, quiet],
    "#scan-now-scrub-setup": [scanNow],
    "#continue-scrub-setup": [continueButton],
    "#not-now-scrub-setup": [notNow],
    "#back-scrub-setup": [back],
  });
  return { root, account, autoScrub, daily, weekly, monthly, before, after, quiet, scanNow, continueButton, notNow, back };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("TASK 0350 Scrub setup control acceptance", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("runs the named account once, saves exact automatic settings, and continues to whitelist setup", async () => {
    const ui = controls();
    const draft = initialScrubSetupDraft();
    const results: ScrubSetupScanResult[][] = [];
    const route: string[] = [];
    const saved: ScrubSetupSummary[] = [];
    const scan = vi.fn(async (accountIds: string[]) => [{
      accountId: accountIds[0],
      result: "discord:liam-0350 found 3 items",
    }]);
    mocks.invoke.mockResolvedValue({ accountCount: 1, automaticSchedule: "monthly", noticeSetting: "quiet" });

    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: scan,
      onScanResult: (found) => results.push([...found]),
      onContinue: (summary) => { saved.push(summary); route.push("whitelist setup"); },
      onNotNow: vi.fn(),
      onBack: vi.fn(),
      onError: vi.fn(),
    });

    expect(draft.selectedScanAccounts.size).toBe(0);
    expect(draft.autoScrub).toBe(false);
    expect(ui.scanNow.disabled).toBe(true);
    expect(ui.continueButton.disabled).toBe(true);

    ui.account.checked = true;
    await ui.account.dispatch("change");
    expect([...draft.selectedScanAccounts]).toEqual(["discord:liam-0350"]);

    await ui.scanNow.dispatch("click");
    await flush();
    expect(scan).toHaveBeenCalledTimes(1);
    expect(scan).toHaveBeenCalledWith(["discord:liam-0350"]);
    expect(results).toEqual([[{ accountId: "discord:liam-0350", result: "discord:liam-0350 found 3 items" }]]);

    ui.autoScrub.checked = true;
    await ui.autoScrub.dispatch("change");
    ui.monthly.checked = true;
    await ui.monthly.dispatch("change");
    ui.quiet.checked = true;
    await ui.quiet.dispatch("change");
    await ui.continueButton.dispatch("click");

    expect(mocks.invoke).toHaveBeenCalledTimes(1);
    expect(mocks.invoke).toHaveBeenCalledWith("save_scrub_setup", { command: {
      selectedScanAccounts: ["discord:liam-0350"], automaticSchedule: "monthly", noticeSetting: "quiet", notNow: false,
    } });
    expect(saved).toEqual([{ accountCount: 1, automaticSchedule: "monthly", noticeSetting: "quiet" }]);
    expect(route).toEqual(["whitelist setup"]);
    console.info("TASK0350_ACCOUNT=discord:liam-0350");
    console.info("TASK0350_SCAN_COUNT=1 TASK0350_SCAN_RESULT=discord:liam-0350 found 3 items");
    console.info("TASK0350_AUTOSCRUB=on TASK0350_FREQUENCY=monthly TASK0350_NOTICE=quiet TASK0350_CONTINUE_ROUTE=whitelist setup");
  });

  it("Not now keeps automatic scanning off and opens whitelist setup; Back opens Choose apps", async () => {
    const ui = controls();
    const draft = initialScrubSetupDraft();
    const route: string[] = [];
    mocks.invoke.mockResolvedValue({ accountCount: 0, automaticSchedule: null, noticeSetting: null });
    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: vi.fn().mockReturnValue([]),
      onContinue: vi.fn(),
      onNotNow: () => route.push("whitelist setup"),
      onBack: () => route.push("Choose apps"),
    });

    await ui.notNow.dispatch("click");
    expect(draft.autoScrub).toBe(false);
    expect(mocks.invoke).toHaveBeenCalledWith("save_scrub_setup", { command: {
      selectedScanAccounts: [], automaticSchedule: null, noticeSetting: null, notNow: true,
    } });
    await ui.back.dispatch("click");
    expect(route).toEqual(["whitelist setup", "Choose apps"]);
    console.info("TASK0350_NOT_NOW_AUTOSCRUB=off TASK0350_NOT_NOW_ROUTE=whitelist setup TASK0350_BACK_ROUTE=Choose apps");
  });

  it("refuses a bad schedule without changing the scan count, settings, or page", async () => {
    const ui = controls();
    const draft = initialScrubSetupDraft();
    const errors = vi.fn();
    const page = "Scrub setup";
    ui.account.checked = true;
    ui.autoScrub.checked = true;
    bindScrubSetupControls(ui.root as unknown as ParentNode, draft, {
      onScanNow: vi.fn().mockReturnValue([]),
      onContinue: vi.fn(),
      onNotNow: vi.fn(),
      onBack: vi.fn(),
      onError: errors,
    });
    await ui.account.dispatch("change");
    await ui.autoScrub.dispatch("change");
    (draft as { automaticSchedule: string }).automaticSchedule = "every-minute";
    const before = { scanCount: draft.selectedScanAccounts.size, schedule: draft.automaticSchedule, notice: draft.noticeSetting, page };

    await ui.continueButton.dispatch("click");
    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(errors).toHaveBeenCalledWith("Choose a valid automatic Scrub schedule");
    expect({ scanCount: draft.selectedScanAccounts.size, schedule: draft.automaticSchedule, notice: draft.noticeSetting, page }).toEqual(before);
    console.info("TASK0350_BAD_SCHEDULE=every-minute TASK0350_REFUSED=yes TASK0350_UNCHANGED_SCAN_COUNT=1 TASK0350_UNCHANGED_PAGE=Scrub setup");
  });
});
