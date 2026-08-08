import { beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";

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

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

// ---------------------------------------------------------------------------
// A faithful double of the three Tauri group-member permission commands from
// TASK 0114: same id charset, same shapes, same one-group list. Only the Rust
// boundary is doubled; every UI layer between a checkbox change and the
// stored list runs for real.
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

// Hand-built checkbox nodes implementing exactly the surface the binder
// touches, in the same style as the unlock-screen tests: anything the binder
// uses that is missing here throws instead of silently passing.
interface FakeCheckbox extends GroupMemberTickCheckbox {
  handlers: (() => void)[];
  setTicked(next: boolean): void;
}

function fakeCheckbox(memberId: string, checked: boolean): FakeCheckbox {
  const box: FakeCheckbox = {
    checked,
    dataset: { whitelistPersonCheckbox: memberId },
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

const GROUP = "group-0117";

function sorted(records: readonly GroupMemberPermissionRecord[]): GroupMemberPermissionRecord[] {
  return [...records].sort((a, b) => (a.memberId < b.memberId ? -1 : 1));
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
});

describe("TASK0117 group dropdown ticks", () => {
  it("wires every rendered tick box to the group-member add and remove commands", () => {
    expect(mainSource).toContain("bindWhitelistDropdownTicks(");
    expect(mainSource).toContain("(memberId, ticked) => void applyWhitelistRosterTick(memberId, ticked)");
    expect(mainSource).toContain("{ add: addGroupMemberPermission, remove: removeGroupMemberPermission }");
    expect(mainSource).toContain("await refreshWhitelistRosterMemberPermissions();");
    expect(mainSource).toContain("whitelistRosterMemberPermissions = groupId ? await listGroupMemberPermissions(groupId) : null;");
  });

  it("ticking and unticking one row changes only that member in a direct list query", async () => {
    const commandLog: { command: string; memberId: string }[] = [];
    installGroupMemberPermissionBackend(commandLog);

    // Seed the group the way the backend commands would have: two members
    // already allowed, one (grace) absent -- the mixed states 0116 renders.
    expect(await addGroupMemberPermission(GROUP, "ada")).toEqual({ groupId: GROUP, memberId: "ada", allowed: true });
    expect(await addGroupMemberPermission(GROUP, "katherine")).toEqual({ groupId: GROUP, memberId: "katherine", allowed: true });
    const before = await listGroupMemberPermissions(GROUP);
    expect(before).not.toBeNull();
    expect(before).toHaveLength(2);
    console.log(`TASK0117 before=${JSON.stringify(before)}`);

    // Three rendered rows, bound exactly as main.ts binds them: the binder
    // from whitelist-dropdown-ticks feeding applyGroupMemberTick with the
    // real adapter commands.
    const boxes = [fakeCheckbox("ada", true), fakeCheckbox("grace", false), fakeCheckbox("katherine", true)];
    const pending: Promise<unknown>[] = [];
    const commands = { add: addGroupMemberPermission, remove: removeGroupMemberPermission };
    const bound = bindWhitelistDropdownTicks(
      { querySelectorAll: (selector) => ({ forEach: (callback: (box: FakeCheckbox) => void) => { expect(selector).toBe("[data-whitelist-person-checkbox]"); boxes.forEach(callback); } }) },
      (memberId, ticked) => { pending.push(applyGroupMemberTick(commands, GROUP, memberId, ticked)); },
    );
    expect(bound).toBe(3);

    commandLog.length = 0;

    // Tick grace's row.
    boxes[1].setTicked(true);
    expect(await Promise.all(pending)).toEqual([{ status: "added", memberId: "grace" }]);
    const afterTick = await listGroupMemberPermissions(GROUP);
    expect(afterTick).not.toBeNull();
    console.log(`TASK0117 after_tick=${JSON.stringify(afterTick)}`);
    expect(afterTick).toHaveLength(3);
    expect(afterTick).toContainEqual({ groupId: GROUP, memberId: "grace", allowed: true });
    // Only that member changed: with grace removed from the result, the list
    // is byte-for-byte the list from before the tick.
    expect(JSON.stringify(sorted((afterTick ?? []).filter((record) => record.memberId !== "grace"))))
      .toBe(JSON.stringify(sorted(before ?? [])));

    // Untick the same row.
    pending.length = 0;
    boxes[1].setTicked(false);
    expect(await Promise.all(pending)).toEqual([{ status: "removed", memberId: "grace" }]);
    const afterUntick = await listGroupMemberPermissions(GROUP);
    console.log(`TASK0117 after_untick=${JSON.stringify(afterUntick)}`);
    expect(JSON.stringify(sorted(afterUntick ?? []))).toBe(JSON.stringify(sorted(before ?? [])));

    // The write commands only ever named grace; no other row was touched.
    console.log(`TASK0117 write_commands=${JSON.stringify(commandLog)}`);
    expect(commandLog).toEqual([
      { command: "add_group_member_permission", memberId: "grace" },
      { command: "remove_group_member_permission", memberId: "grace" },
    ]);
  });

  it("refuses a tick without a member id before any command is issued", async () => {
    const commandLog: { command: string; memberId: string }[] = [];
    installGroupMemberPermissionBackend(commandLog);
    const commands = { add: addGroupMemberPermission, remove: removeGroupMemberPermission };
    expect(await applyGroupMemberTick(commands, GROUP, "", true))
      .toEqual({ status: "refused", reason: "OSL member identifier is required" });
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
