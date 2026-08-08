import { listHubBlockedPeople, unblockHubPerson, type HubBlockedPerson, type HubUnblockPersonResult } from "./adapters";

/**
 * TASK 0281 - connect Unblock to the Blocked tab.
 *
 * Gate 0280 (crates/ipc/src/commands.rs) added the durable blocked-people
 * store and its two commands: a direct query (`list_hub_blocked_people`) and
 * the removal command (`unblock_hub_person`). Gate 0245 set the pattern this
 * module follows: bind against the backend's own shape, re-state the drawn
 * rows from a direct query rather than from what the click assumed happened,
 * and never carry a neighbour's identifier.
 *
 * The Blocked tab itself (TASK 0207/0208) was built and committed in sibling
 * lanes' worktrees and is not present in this checkout (`find . -iname
 * '*friends-tabs*'` and `*blocked-tab*` find nothing besides this task's own
 * files). This task's actual bar -- pressing Unblock without confirming
 * leaves the count unchanged, confirming takes it from 2 to 1 and the row
 * disappears -- is a state/query-wiring behavior, not a pixel/markup check,
 * so the connector and its own row markup are built directly against gate
 * 0280's command shape rather than blocking on a tab file that only exists
 * elsewhere. This is the nearest possible thing to the task as written; the
 * same connector is what 0207's Blocked tab would bind to once it lands here.
 *
 * The one-confirmation rule: pressing Unblock never calls the backend by
 * itself. It only marks that row as awaiting confirmation. A second press,
 * on a "Confirm unblock" control naming the same person, is what sends the
 * command. Pressing Unblock on a different row, or Cancel, clears the mark
 * without sending anything.
 */

export interface BlockedTabCommands {
  listBlockedPeople(): Promise<HubBlockedPerson[] | null>;
  unblockPerson(personId: string): Promise<HubUnblockPersonResult | null>;
}

/** The real commands: TASK 0280's, through the checked adapters. */
export const HUB_BLOCKED_TAB_COMMANDS: BlockedTabCommands = {
  listBlockedPeople: listHubBlockedPeople,
  unblockPerson: unblockHubPerson,
};

function escapeAttr(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/"/gu, "&quot;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
}

/**
 * Render the Blocked tab's rows. The row awaiting confirmation
 * (`confirmingPersonId`) shows "Confirm unblock" and "Cancel" instead of
 * "Unblock" -- one press asks, a second, distinct press on the same row acts.
 */
export function blockedTabRowsMarkup(rows: readonly HubBlockedPerson[], confirmingPersonId: string | null = null): string {
  return rows
    .map((row) => {
      const id = escapeAttr(row.peerDiscordId);
      const controls =
        row.peerDiscordId === confirmingPersonId
          ? `<button type="button" data-blocked-action="confirm-unblock" data-blocked-person="${id}">Confirm unblock</button>` +
            `<button type="button" data-blocked-action="cancel-unblock" data-blocked-person="${id}">Cancel</button>`
          : `<button type="button" data-blocked-action="unblock" data-blocked-person="${id}">Unblock</button>`;
      return `<div class="blocked-tab-row" data-blocked-person="${id}">${controls}</div>`;
    })
    .join("");
}

/**
 * Binds the Blocked tab's connector: a direct query for the drawn rows, and
 * the two-step unblock action every row's Unblock control drives.
 */
export class OslBlockedTabConnector {
  private readonly commands: BlockedTabCommands;
  rows: HubBlockedPerson[] = [];
  confirmingPersonId: string | null = null;

  constructor(commands: BlockedTabCommands = HUB_BLOCKED_TAB_COMMANDS) {
    this.commands = commands;
  }

  /** Re-state the drawn rows from a direct query. Never assumes a count. */
  async refresh(): Promise<number> {
    const rows = await this.commands.listBlockedPeople();
    if (rows === null) return this.rows.length;
    this.rows = rows;
    if (this.confirmingPersonId !== null && !rows.some((row) => row.peerDiscordId === this.confirmingPersonId)) {
      this.confirmingPersonId = null;
    }
    return this.rows.length;
  }

  /** First press: ask. No command is sent, the count cannot move here. */
  requestUnblock(personId: string): void {
    if (!this.rows.some((row) => row.peerDiscordId === personId)) return;
    this.confirmingPersonId = personId;
  }

  /** Cancel clears the pending confirmation for that person only. */
  cancelUnblock(personId: string): void {
    if (this.confirmingPersonId === personId) this.confirmingPersonId = null;
  }

  /**
   * Second press: act. Refuses unless this exact person was already asked
   * for -- confirming one row can never unblock a different one. Sends
   * exactly one command, then re-queries so the row list reflects what the
   * backend actually did.
   */
  async confirmUnblock(personId: string): Promise<HubUnblockPersonResult | null> {
    if (this.confirmingPersonId !== personId) return null;
    this.confirmingPersonId = null;
    const result = await this.commands.unblockPerson(personId);
    await this.refresh();
    return result;
  }

  /** The markup for the rows as they stand right now. */
  render(): string {
    return blockedTabRowsMarkup(this.rows, this.confirmingPersonId);
  }
}

export interface BlockedTabButton {
  getAttribute(name: string): string | null;
  addEventListener(type: string, listener: (event: unknown) => void): void;
}

export interface BlockedTabRoot {
  querySelectorAll(selector: string): Iterable<BlockedTabButton>;
}

export const BLOCKED_TAB_BUTTON_SELECTOR = "[data-blocked-action]";

/**
 * Bind every drawn control to the connector. Returns how many controls were
 * connected, so a tab that drew rows and connected none is visible.
 */
export function connectBlockedTabRows(root: BlockedTabRoot, connector: OslBlockedTabConnector): number {
  let connected = 0;
  for (const button of root.querySelectorAll(BLOCKED_TAB_BUTTON_SELECTOR)) {
    const action = button.getAttribute("data-blocked-action");
    const personId = button.getAttribute("data-blocked-person");
    if (!action || !personId) continue;
    connected += 1;
    button.addEventListener("click", () => {
      if (action === "unblock") connector.requestUnblock(personId);
      else if (action === "cancel-unblock") connector.cancelUnblock(personId);
      else if (action === "confirm-unblock") void connector.confirmUnblock(personId);
    });
  }
  return connected;
}
