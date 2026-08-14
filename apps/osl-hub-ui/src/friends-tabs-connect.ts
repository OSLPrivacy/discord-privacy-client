/**
 * TASK 0208: connect the friends tabs.
 *
 * `crates/ipc` (task 0204) already exposes one query,
 * `cmd_osl_query_friends_tabs`, that returns all four tab row sets
 * (`online`, `all`, `pending`, `blocked`) in one shot, mutually exclusive per
 * task 0206. This module is the host-driven controller that binds a rendered
 * tab bar to that query and re-runs it after any command that can move a
 * person between tabs (accept / decline / block / revoke), so the UI never
 * needs a restart to reflect the new state.
 */

export type OslFriendsTabId = "online" | "all" | "pending" | "blocked";

export const OSL_FRIENDS_TAB_IDS: readonly OslFriendsTabId[] = ["online", "all", "pending", "blocked"];

export interface OslFriendsTabRow {
  requestId: string;
  targetId: string;
  displayName: string;
}

export interface OslFriendsTabsQueryResult {
  online: readonly OslFriendsTabRow[];
  all: readonly OslFriendsTabRow[];
  pending: readonly OslFriendsTabRow[];
  blocked: readonly OslFriendsTabRow[];
}

const EMPTY_TABS_QUERY_RESULT: OslFriendsTabsQueryResult = {
  online: [],
  all: [],
  pending: [],
  blocked: [],
};

/** Host boundary: the real implementation calls the Tauri IPC commands. */
export interface OslFriendsTabsHost {
  queryFriendsTabs(): Promise<OslFriendsTabsQueryResult>;
  acceptFriendRequest(requestId: string): Promise<unknown>;
  declineFriendRequest(requestId: string): Promise<unknown>;
  blockFriendRequest(requestId: string): Promise<unknown>;
}

export interface OslFriendsTabsState {
  activeTab: OslFriendsTabId;
  rowsByTab: OslFriendsTabsQueryResult;
}

export function oslFriendsTabsBlankState(activeTab: OslFriendsTabId = "all"): OslFriendsTabsState {
  return { activeTab, rowsByTab: EMPTY_TABS_QUERY_RESULT };
}

export function oslFriendsTabRows(state: OslFriendsTabsState, tab: OslFriendsTabId): readonly OslFriendsTabRow[] {
  return state.rowsByTab[tab];
}

export function oslFriendsTabCount(state: OslFriendsTabsState, tab: OslFriendsTabId): number {
  return state.rowsByTab[tab].length;
}

/**
 * Controller bound to a single friends-tabs surface. Holds the last query
 * result in memory so a state change (accept/decline/block) can refresh it
 * in place instead of requiring the caller to reload the whole view.
 */
export class OslFriendsTabsConnector {
  private host: OslFriendsTabsHost;
  state: OslFriendsTabsState;

  constructor(host: OslFriendsTabsHost, initialTab: OslFriendsTabId = "all") {
    this.host = host;
    this.state = oslFriendsTabsBlankState(initialTab);
  }

  setActiveTab(tab: OslFriendsTabId): void {
    this.state = { ...this.state, activeTab: tab };
  }

  async refresh(): Promise<OslFriendsTabsState> {
    const rowsByTab = await this.host.queryFriendsTabs();
    this.state = { ...this.state, rowsByTab };
    return this.state;
  }

  async acceptRequest(requestId: string): Promise<OslFriendsTabsState> {
    await this.host.acceptFriendRequest(requestId);
    return this.refresh();
  }

  async declineRequest(requestId: string): Promise<OslFriendsTabsState> {
    await this.host.declineFriendRequest(requestId);
    return this.refresh();
  }

  async blockRequest(requestId: string): Promise<OslFriendsTabsState> {
    await this.host.blockFriendRequest(requestId);
    return this.refresh();
  }
}
