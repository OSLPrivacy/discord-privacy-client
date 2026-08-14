import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauriRuntime: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  addGroupMemberPermission,
  listGroupMemberPermissions,
  removeGroupMemberPermission,
  type GroupMemberPermissionRecord,
} from "./adapters";
import {
  applyGroupMemberTick,
  bindWhitelistDropdownTicks,
  type GroupMemberTickCheckbox,
} from "./whitelist-dropdown-ticks";

// ---------------------------------------------------------------------------
// The same faithful double of the three Tauri group-member permission commands
// used by the TASK 0117 check: same id charset, same shapes, same one-group
// list. Only the Rust boundary is doubled; the binder, the tick flow and the
// adapters all run for real.
// ---------------------------------------------------------------------------
const PERMISSION_ID = /^[A-Za-z0-9_-]{1,128}$/;

function installGroupMemberPermissionBackend(commandLog: { command: string; memberId: string }[]): void {
  const groups = new Map<string, Map<string, boolean>>();
  mocks.invoke.mockImplementation((command: string, args?: Record<string, unknown>) => {
    const groupId = String(args?.groupId ?? "");
    if (!PERMISSION_ID.test(groupId)) return Promise.reject(new Error("OSL group identifier is invalid"));
    if (command === "list_group_member_permissions") {
      const members = groups.get(groupId) ?? new Map<string, boolean>();
      return Promise.resolve([...members.entries()]
        .sort(([a], [b]) => (a < b ? -1 : 1))
        .map(([memberId, allowed]) => ({ groupId, memberId, allowed })));
    }
    const memberId = String(args?.memberId ?? "");
    if (!PERMISSION_ID.test(memberId)) return Promise.reject(new Error("OSL member identifier is invalid"));
    commandLog.push({ command, memberId });
    if (command === "add_group_member_permission") {
      const members = groups.get(groupId) ?? new Map<string, boolean>();
      groups.set(groupId, members);
      members.set(memberId, args?.allowed === true);
      return Promise.resolve({ groupId, memberId, allowed: args?.allowed === true });
    }
    if (command === "remove_group_member_permission") {
      const members = groups.get(groupId);
      const removed = members?.delete(memberId) ?? false;
      if (members && members.size === 0) groups.delete(groupId);
      return Promise.resolve(removed);
    }
    return Promise.reject(new Error(`unexpected command ${command}`));
  });
}

interface FakeCheckbox extends GroupMemberTickCheckbox {
  handlers: (() => void)[];
  setTicked(next: boolean): void;
}

function fakeCheckbox(dataset: { whitelistPersonCheckbox?: string }, checked: boolean): FakeCheckbox {
  const box: FakeCheckbox = {
    checked,
    dataset: { ...dataset },
    handlers: [],
    addEventListener(type, handler) {
      if (type === "change") box.handlers.push(handler);
    },
    setTicked(next) {
      box.checked = next;
      for (const handler of box.handlers) handler();
    },
  };
  return box;
}

// Bind one row exactly as main.ts binds the rendered dropdown, and return the
// outcomes its ticks produce.
function bindRow(box: FakeCheckbox, group: string) {
  const outcomes: Promise<unknown>[] = [];
  const commands = { add: addGroupMemberPermission, remove: removeGroupMemberPermission };
  const bound = bindWhitelistDropdownTicks(
    { querySelectorAll: (selector) => ({ forEach: (callback: (row: FakeCheckbox) => void) => { expect(selector).toBe("[data-whitelist-person-checkbox]"); callback(box); } }) },
    (memberId, ticked) => { outcomes.push(applyGroupMemberTick(commands, group, memberId, ticked)); },
  );
  expect(bound).toBe(1);
  return outcomes;
}

const GROUP = "group-0119";

function sortedBytes(records: readonly GroupMemberPermissionRecord[]): string[] {
  return [...records].sort((a, b) => (a.memberId < b.memberId ? -1 : 1)).map((record) => JSON.stringify(record));
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("TASK0119 group tick without a member id", () => {
  it("refuses the missing-id copy of a valid tick and leaves both permissions byte-for-byte unchanged", async () => {
    const commandLog: { command: string; memberId: string }[] = [];
    installGroupMemberPermissionBackend(commandLog);

    // Seed permission MAPLE-0119 and read it back directly: readable, count 1.
    expect(await addGroupMemberPermission(GROUP, "MAPLE-0119"))
      .toEqual({ groupId: GROUP, memberId: "MAPLE-0119", allowed: true });
    const before = await listGroupMemberPermissions(GROUP);
    expect(before).not.toBeNull();
    expect(before).toHaveLength(1);
    expect(before).toContainEqual({ groupId: GROUP, memberId: "MAPLE-0119", allowed: true });
    console.log(`TASK0119 before_count=${before?.length} before=${JSON.stringify(before)}`);

    // Tick valid member MEMBER-0119 through the bound row.
    const validRowFields = { checked: false, dataset: { whitelistPersonCheckbox: "MEMBER-0119" } };
    const validRow = fakeCheckbox(validRowFields.dataset, validRowFields.checked);
    const validOutcomes = bindRow(validRow, GROUP);
    validRow.setTicked(true);
    const validOutcome = await Promise.all(validOutcomes);
    expect(validOutcome).toEqual([{ status: "added", memberId: "MEMBER-0119" }]);
    const afterValid = await listGroupMemberPermissions(GROUP);
    expect(afterValid).not.toBeNull();
    expect(afterValid).toHaveLength(2);
    expect(afterValid).toContainEqual({ groupId: GROUP, memberId: "MEMBER-0119", allowed: true });
    console.log(`TASK0119 valid_tick_outcome=${JSON.stringify(validOutcome)}`);
    console.log(`TASK0119 after_valid_count=${afterValid?.length} after_valid=${JSON.stringify(afterValid)}`);
    const bytesAfterValid = sortedBytes(afterValid ?? []);

    // A copy of the valid tick's row whose ONLY changed field is its missing
    // member id: same unticked starting state, same tick action, but the
    // member-id data attribute is gone.
    const { whitelistPersonCheckbox: droppedMemberId, ...datasetWithoutMemberId } = validRowFields.dataset;
    const missingIdRowFields = { ...validRowFields, dataset: datasetWithoutMemberId };
    expect(droppedMemberId).toBe("MEMBER-0119");
    expect(missingIdRowFields).toEqual({ checked: false, dataset: {} });
    const missingIdRow = fakeCheckbox(missingIdRowFields.dataset, missingIdRowFields.checked);

    const invokesBefore = mocks.invoke.mock.calls.length;
    const commandsBefore = commandLog.length;
    const missingIdOutcomes = bindRow(missingIdRow, GROUP);
    missingIdRow.setTicked(true);
    const refused = await Promise.all(missingIdOutcomes);
    expect(refused).toEqual([{ status: "refused", reason: "OSL member identifier is required" }]);
    console.log(`TASK0119 missing_id_tick=${JSON.stringify(refused)}`);

    // The refusal happened before any command: nothing crossed the boundary.
    expect(mocks.invoke.mock.calls.length).toBe(invokesBefore);
    expect(commandLog.length).toBe(commandsBefore);

    // Count stays 2 and both permissions are byte-for-byte unchanged.
    const afterRefused = await listGroupMemberPermissions(GROUP);
    expect(afterRefused).not.toBeNull();
    expect(afterRefused).toHaveLength(2);
    const bytesAfterRefused = sortedBytes(afterRefused ?? []);
    expect(bytesAfterRefused).toEqual(bytesAfterValid);
    console.log(`TASK0119 after_refused_count=${afterRefused?.length}`);
    console.log(`TASK0119 bytes_unchanged=${JSON.stringify(bytesAfterRefused)}`);
  });
});
