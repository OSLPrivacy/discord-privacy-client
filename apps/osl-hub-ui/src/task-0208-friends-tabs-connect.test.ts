import { describe, expect, it } from "vitest";
import {
  OslFriendsTabsConnector,
  type OslFriendsTabRow,
  type OslFriendsTabsHost,
  type OslFriendsTabsQueryResult,
  oslFriendsTabCount,
  oslFriendsTabRows,
} from "./friends-tabs-connect";

/**
 * Fixture backend that mirrors the mutually-exclusive tab shape
 * `cmd_osl_query_friends_tabs` returns (task 0204/0206): a request lives in
 * exactly one of pending/all/online/blocked at a time, and accepting it moves
 * it from pending straight into `all`.
 */
function fixtureFriendsTabsBackend(): OslFriendsTabsHost {
  const row: OslFriendsTabRow = {
    requestId: "req-0208-pending",
    targetId: "900000000000020801",
    displayName: "Pending Fixture Friend",
  };
  let pending: OslFriendsTabRow[] = [row];
  let all: OslFriendsTabRow[] = [];

  return {
    async queryFriendsTabs(): Promise<OslFriendsTabsQueryResult> {
      return { online: [], all: [...all], pending: [...pending], blocked: [] };
    },
    async acceptFriendRequest(requestId: string) {
      const index = pending.findIndex((entry) => entry.requestId === requestId);
      if (index === -1) throw new Error("OSL: friend request is not pending");
      const [accepted] = pending.splice(index, 1);
      all = [...all, accepted];
      return accepted;
    },
    async declineFriendRequest(requestId: string) {
      pending = pending.filter((entry) => entry.requestId !== requestId);
    },
    async blockFriendRequest(requestId: string) {
      pending = pending.filter((entry) => entry.requestId !== requestId);
    },
  };
}

describe("task 0208: connect the friends tabs", () => {
  it("accepting a fixture request moves it from Pending to All without restart", async () => {
    const connector = new OslFriendsTabsConnector(fixtureFriendsTabsBackend(), "pending");
    await connector.refresh();

    const pendingBefore = oslFriendsTabCount(connector.state, "pending");
    const allBefore = oslFriendsTabCount(connector.state, "all");
    expect(pendingBefore).toBe(1);
    expect(allBefore).toBe(0);

    await connector.acceptRequest("req-0208-pending");

    const pendingAfter = oslFriendsTabCount(connector.state, "pending");
    const allAfter = oslFriendsTabCount(connector.state, "all");
    const movedRow = oslFriendsTabRows(connector.state, "all")[0];

    expect(pendingAfter).toBe(0);
    expect(allAfter).toBe(1);
    expect(movedRow.requestId).toBe("req-0208-pending");

    console.log(
      `TASK_0208_FRIENDS_TABS_CONNECT pending_before=${pendingBefore} all_before=${allBefore} pending_after=${pendingAfter} all_after=${allAfter} moved_request=${movedRow.requestId}`,
    );
  });

  it("each tab query is independently filtered per tab id", async () => {
    const connector = new OslFriendsTabsConnector(fixtureFriendsTabsBackend(), "all");
    await connector.refresh();

    expect(oslFriendsTabRows(connector.state, "online")).toHaveLength(0);
    expect(oslFriendsTabRows(connector.state, "blocked")).toHaveLength(0);
    expect(oslFriendsTabRows(connector.state, "pending")).toHaveLength(1);
  });
});
