import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  BACK_ATTRIBUTE,
  FRIEND_ROW_ATTRIBUTE,
  HOME_ROUTE,
  OSL_FRIEND_PAGE_ROUTE,
  UNKNOWN_FRIEND_REFUSAL,
  oslFriendsPanelMarkup,
  resolveOslFriendsPanelRoute,
  routeForOslFriendRow,
  routeForOslFriendsPanelBack,
  type HomeFriendRow,
} from "./osl-friends-panel-routing";

/**
 * The three rows task 0828's direct read returned (evidence/0828.md): the same saved friend
 * identifiers, OSL ids, usernames, permitted picture and coloured initials.
 */
const PERMITTED_PICTURE = "data:image/gif;base64,R0lGODlhAQABAIAAAAD/ACwAAAAAAQABAAACAkQBADs=";

function fixtureRows(): HomeFriendRow[] {
  return [
    {
      friendId: "friend:f8e88443aa9253d65fd9fe5e9c05b855407d0133446703dd45408d0567b8e7c7",
      oslUserId: "900000000000082801",
      username: "Ada Friend",
      picture: PERMITTED_PICTURE,
      pictureStatus: "image-present",
      initial: "A",
      initialColour: "#2563eb",
    },
    {
      friendId: "friend:42de6e37fc1de7206d09ccbd5e799d752dbcbbe21064679b5cc033ff3c68b17a",
      oslUserId: "900000000000082802",
      username: "Bo Friend",
      picture: null,
      pictureStatus: "image-absent",
      initial: "B",
      initialColour: "#2563eb",
    },
    {
      friendId: "friend:065a8d10a8f0d9b449c53523ce8b36b8880f2fdbf618903dd017d66a075dda72",
      oslUserId: "900000000000082803",
      username: "Cleo Friend",
      picture: null,
      pictureStatus: "image-absent",
      initial: "C",
      initialColour: "#dc2626",
    },
  ];
}

/** The non-friend from 0828's fixture, whose saved id 0828 proved never reaches the panel. */
const UNKNOWN_FRIEND_ID = "friend:7a0465fb5129e6e0f7f41c96fa0a779a7670930793596572324a9b2794eeddf9";

function rowIdsInMarkup(markup: string): string[] {
  return [...markup.matchAll(new RegExp(`${FRIEND_ROW_ATTRIBUTE}="([^"]+)"`, "g"))].map((m) => m[1]);
}

function readMain(): string {
  return readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
}

describe("TASK 0829 - connect OSL Friends panel", () => {
  it("each panel row routes to that friend's page carrying that friend's exact id", () => {
    const rows = fixtureRows();
    const routedIds: string[] = [];
    for (const row of rows) {
      const result = routeForOslFriendRow(row.friendId, rows);
      expect(result.refusal).toBeNull();
      expect(result.route).not.toBeNull();
      const route = result.route as { name: string; friendId: string; oslUserId: string; username: string };
      expect(route.name).toBe(OSL_FRIEND_PAGE_ROUTE);
      expect(route.friendId).toBe(row.friendId);
      expect(route.oslUserId).toBe(row.oslUserId);
      expect(route.username).toBe(row.username);
      routedIds.push(route.friendId);
      // No other friend's id can ride along on this route.
      for (const other of rows.filter((candidate) => candidate.friendId !== row.friendId)) {
        expect(JSON.stringify(route)).not.toContain(other.friendId);
        expect(JSON.stringify(route)).not.toContain(other.oslUserId);
      }
    }
    console.log(`TASK_0829 rows=${rows.length} routed_exact=${routedIds.length} routed_ids=${routedIds.join(",")}`);
    expect(routedIds).toEqual(rows.map((row) => row.friendId));
  });

  it("the ids the drawn rows carry are exactly the ids the route accepts", () => {
    const rows = fixtureRows();
    const markup = oslFriendsPanelMarkup(rows);
    const drawn = rowIdsInMarkup(markup);
    expect(drawn).toEqual(rows.map((row) => row.friendId));

    const roundTripped = drawn.map((friendId) => {
      const result = resolveOslFriendsPanelRoute({ control: "friend-row", friendId }, rows);
      expect(result.refusal).toBeNull();
      return (result.route as { friendId: string }).friendId;
    });
    console.log(`TASK_0829 drawn_rows=${drawn.length} round_tripped=${roundTripped.length} back_controls=${rowIdsInMarkup(markup).length ? markup.split(`${BACK_ATTRIBUTE}="`).length - 1 : 0}`);
    expect(roundTripped).toEqual(drawn);
    expect(markup).toContain(`${BACK_ATTRIBUTE}="${HOME_ROUTE}"`);
  });

  it("Back routes to Home, and Home is the app's real Home route", () => {
    const back = resolveOslFriendsPanelRoute({ control: "back" }, fixtureRows());
    expect(back.refusal).toBeNull();
    expect(back.route).toEqual({ name: "home" });
    expect(routeForOslFriendsPanelBack().route).toEqual({ name: "home" });
    expect(HOME_ROUTE).toBe("home");

    const main = readMain();
    const union = main.slice(main.indexOf("export type Route ="));
    expect(union.slice(0, union.indexOf(";"))).toContain(`"${HOME_ROUTE}"`);
    console.log(`TASK_0829 back_route=${(back.route as { name: string }).name} back_refusal=${back.refusal}`);
  });

  it("an unknown friend id opens no friend page and is refused by name", () => {
    const rows = fixtureRows();
    const result = resolveOslFriendsPanelRoute({ control: "friend-row", friendId: UNKNOWN_FRIEND_ID }, rows);
    console.log(`TASK_0829 unknown_id=${UNKNOWN_FRIEND_ID} unknown_route=${result.route === null ? "none" : JSON.stringify(result.route)} refusal=${result.refusal}`);
    expect(result.route).toBeNull();
    expect(result.refusal).toBe(`${UNKNOWN_FRIEND_REFUSAL}: "${UNKNOWN_FRIEND_ID}"`);
    expect(result.refusal).toContain("OSL: no such friend on the OSL Friends panel");
    expect(rowIdsInMarkup(oslFriendsPanelMarkup(rows))).not.toContain(UNKNOWN_FRIEND_ID);
  });

  it("a blank id, and a near-miss of a real id, are refused the same way", () => {
    const rows = fixtureRows();
    const blank = resolveOslFriendsPanelRoute({ control: "friend-row", friendId: "" }, rows);
    expect(blank.route).toBeNull();
    expect(blank.refusal).toBe(`${UNKNOWN_FRIEND_REFUSAL}: ""`);

    const nearMiss = `${rows[0].friendId.slice(0, -1)}0`;
    expect(nearMiss).not.toBe(rows[0].friendId);
    const off = resolveOslFriendsPanelRoute({ control: "friend-row", friendId: nearMiss }, rows);
    console.log(`TASK_0829 blank_refusal=${blank.refusal} near_miss_route=${off.route === null ? "none" : "opened"} near_miss_refusal=${off.refusal}`);
    expect(off.route).toBeNull();
    expect(off.refusal).toBe(`${UNKNOWN_FRIEND_REFUSAL}: "${nearMiss}"`);
  });

  it("draws the permitted picture on one row and the coloured initial on the others", () => {
    const markup = oslFriendsPanelMarkup(fixtureRows());
    expect(markup.split("osl-friend-picture").length - 1).toBe(1);
    expect(markup.split("osl-friend-initial").length - 1).toBe(2);
    expect(markup).toContain("Ada Friend");
    expect(markup).toContain("background:#dc2626");
  });

  it("an empty panel still offers Back to Home", () => {
    const markup = oslFriendsPanelMarkup([]);
    expect(rowIdsInMarkup(markup)).toHaveLength(0);
    expect(markup).toContain(`${BACK_ATTRIBUTE}="${HOME_ROUTE}"`);
    expect(markup).toContain("No friends yet");
  });
});
