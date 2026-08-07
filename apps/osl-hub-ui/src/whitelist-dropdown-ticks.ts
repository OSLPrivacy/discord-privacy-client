/**
 * The group whitelist dropdown's tick boxes.
 *
 * A tick is a durable write, so the flow is deliberately narrow: one checkbox
 * change maps to exactly one group-member add or remove command for the person
 * on that row, and nothing else. The id rule mirrors the backend's
 * [A-Za-z0-9_-]{1,128} charset, so a malformed or missing id is refused here
 * and no command is ever issued for it.
 */
const GROUP_MEMBER_TICK_ID = /^[A-Za-z0-9_-]{1,128}$/;

export interface GroupMemberTickRecord {
  groupId: string;
  memberId: string;
  allowed: boolean;
}

export interface GroupMemberTickCommands {
  add(groupId: string, memberId: string): Promise<GroupMemberTickRecord | null>;
  remove(groupId: string, memberId: string): Promise<boolean | null>;
}

export type GroupMemberTickOutcome =
  | { status: "added"; memberId: string }
  | { status: "removed"; memberId: string }
  | { status: "refused"; reason: string };

/**
 * Apply one tick change. Ticked issues the add command; unticked issues the
 * remove command. The saved record is checked against what was asked for, so a
 * backend that answered for a different member or group reads as a refusal.
 */
export async function applyGroupMemberTick(
  commands: GroupMemberTickCommands,
  groupId: string,
  memberId: string,
  ticked: boolean,
): Promise<GroupMemberTickOutcome> {
  if (!GROUP_MEMBER_TICK_ID.test(groupId)) return { status: "refused", reason: "OSL group identifier is required" };
  if (!GROUP_MEMBER_TICK_ID.test(memberId)) return { status: "refused", reason: "OSL member identifier is required" };
  if (ticked) {
    const saved = await commands.add(groupId, memberId);
    if (!saved || saved.groupId !== groupId || saved.memberId !== memberId || saved.allowed !== true) {
      return { status: "refused", reason: "the group-member add command failed closed" };
    }
    return { status: "added", memberId };
  }
  const removed = await commands.remove(groupId, memberId);
  if (removed !== true) return { status: "refused", reason: "the group-member remove command failed closed" };
  return { status: "removed", memberId };
}

/** The DOM surface the binder touches; hand-built nodes satisfy it in tests. */
export interface GroupMemberTickCheckbox {
  checked: boolean;
  dataset: { whitelistPersonCheckbox?: string };
  addEventListener(type: "change", handler: () => void): void;
}

export interface GroupMemberTickRoot {
  querySelectorAll(selector: string): { forEach(callback: (box: GroupMemberTickCheckbox) => void): void };
}

/**
 * Connect every rendered tick box to the apply callback. The member id rides
 * on the row's own data attribute, so each checkbox can only ever change the
 * person it was rendered for. Returns how many boxes were bound.
 */
export function bindWhitelistDropdownTicks(
  root: GroupMemberTickRoot,
  apply: (memberId: string, ticked: boolean) => void,
): number {
  let bound = 0;
  root.querySelectorAll("[data-whitelist-person-checkbox]").forEach((box) => {
    box.addEventListener("change", () => {
      apply(box.dataset.whitelistPersonCheckbox ?? "", box.checked);
    });
    bound += 1;
  });
  return bound;
}
