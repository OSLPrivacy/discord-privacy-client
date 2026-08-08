import { describe, expect, it, vi } from "vitest";
import {
  bindAutoScrubDeletionPermission,
  openAutoScrubDeletionPermission,
  renderAutoScrubDeletionPermission,
  resolveAutoScrubDeletionPermission,
  setAutoScrubDeletionAgreement,
  type AutoScrubScheduleValues,
} from "./autoscrub-deletion-permission";

const savedSchedule: AutoScrubScheduleValues = {
  selectedAccountIds: ["discord:liam", "telegram:liam"],
  selectedRuleNames: ["personal address", "passport number"],
  mode: "find_only",
};

class FakeControl {
  checked = false;
  disabled = false;
  readonly dataset: Record<string, string>;
  private readonly listeners = new Map<string, Array<(event: { currentTarget: FakeControl }) => void>>();

  constructor(action?: string) { this.dataset = action ? { autoscrubDeletionAction: action } : {}; }
  addEventListener(type: string, listener: (event: { currentTarget: FakeControl }) => void): void {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  dispatch(type: string): void { this.listeners.get(type)?.forEach((listener) => listener({ currentTarget: this })); }
}

class FakeRoot {
  constructor(private readonly agreement: FakeControl, private readonly buttons: FakeControl[]) {}
  querySelector<T>(selector: string): T | null { return (selector === "#autoscrub-deletion-agreement" ? this.agreement : null) as T | null; }
  querySelectorAll<T>(selector: string): T[] { return (selector === "[data-autoscrub-deletion-action]" ? this.buttons : []) as T[]; }
}

describe("TASK 1470 connect deletion permission page", () => {
  it("shows selected accounts, rules, all risk text, agreement tick, modes, and Cancel", () => {
    const html = renderAutoScrubDeletionPermission(openAutoScrubDeletionPermission(savedSchedule));

    for (const value of [...savedSchedule.selectedAccountIds, ...savedSchedule.selectedRuleNames]) expect(html).toContain(value);
    expect(html).toContain("Deleted messages can be permanent.");
    expect(html).toContain("Service rules may forbid automated reading or deletion.");
    expect(html).toContain("Suspension or ban risk is real.");
    expect(html).toContain("id=\"autoscrub-deletion-agreement\"");
    expect(html).toContain(">Find only<");
    expect(html).toContain(">Find and delete<");
    expect(html).toContain(">Cancel<");
  });

  it("requires the agreement before Find and delete but leaves Find only available", () => {
    const opened = openAutoScrubDeletionPermission(savedSchedule);
    expect(resolveAutoScrubDeletionPermission(opened, "find-and-delete")).toEqual(opened);
    expect(resolveAutoScrubDeletionPermission(opened, "find-only").savedSchedule.mode).toBe("find_only");
    expect(resolveAutoScrubDeletionPermission(setAutoScrubDeletionAgreement(opened, true), "find-and-delete").savedSchedule.mode).toBe("find_and_delete");
  });

  it("Cancel returns to the schedule with 0 differences on a full read-back of saved values", () => {
    const before = structuredClone(savedSchedule);
    const returned = resolveAutoScrubDeletionPermission(setAutoScrubDeletionAgreement(openAutoScrubDeletionPermission(savedSchedule), true), "cancel");
    const differences = Object.entries(before).filter(([key, value]) => JSON.stringify(value) !== JSON.stringify(returned.savedSchedule[key as keyof AutoScrubScheduleValues]));

    expect(returned.screen).toBe("schedule");
    expect(returned.savedSchedule).toEqual(before);
    expect(differences).toHaveLength(0);
    console.info(`TASK1470_CANCEL_ROUTE=${returned.screen} TASK1470_SAVED_VALUE_DIFFERENCES=${differences.length}`);
  });

  it("connects the agreement tick and all three page buttons", () => {
    const agreement = new FakeControl();
    const cancel = new FakeControl("cancel");
    const findOnly = new FakeControl("find-only");
    const findAndDelete = new FakeControl("find-and-delete");
    const received = vi.fn();
    const changed = vi.fn();
    bindAutoScrubDeletionPermission(new FakeRoot(agreement, [cancel, findOnly, findAndDelete]) as unknown as ParentNode, openAutoScrubDeletionPermission(savedSchedule), { onStateChange: changed, onResolve: received });

    agreement.checked = true;
    agreement.dispatch("change");
    cancel.dispatch("click");
    findOnly.dispatch("click");
    findAndDelete.dispatch("click");
    expect(changed).toHaveBeenCalledWith(expect.objectContaining({ agreementChecked: true, screen: "deletion-permission" }));
    expect(received).toHaveBeenCalledTimes(3);
    expect(received.mock.calls.map(([, action]) => action)).toEqual(["cancel", "find-only", "find-and-delete"]);
  });
});
