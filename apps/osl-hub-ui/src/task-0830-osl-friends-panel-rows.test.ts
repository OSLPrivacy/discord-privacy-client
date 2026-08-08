// TASK 0830 - prove OSL Friends panel.
//
// Two friends are created with *different picture permissions*: one permitted the panel to show
// the picture stored for them, the other did not (their picture is on disk all the same). The
// Home rows are then read straight off the panel task 0829 draws - no clicking, no screenshot,
// no snapshot file - and each row is checked for the correct name and the correct avatar: the
// permitted picture for the friend who permitted it, the coloured initial for the friend who did
// not, with the withheld picture's bytes proved absent from the whole panel.
//
// The same check body is then pointed at a second fixture built *without* the permitted picture
// rule (permission ignored; whatever is stored is carried), and the check has to go red - the
// finish line's second clause. `checkHomeFriendRows` is deliberately one function used by both,
// so the passing run and the failing run are the same check, not two different ones.
//
// Note on where the rule lives. Task 0828 (crates/ipc, lane g) applies the permitted picture rule
// when it *writes* a row; task 0829 (this package) applies it again when it *draws* a row - a
// picture reaches the screen only if the row says `image-present` and the value is a bounded
// inline image. 0828's Rust is not on this branch (`git merge-base --is-ancestor` of its commit
// against HEAD says no), so the write side is reproduced here as the fixture builder
// `createdFriendRow`, using the same rule and the same recorded values 0828's direct read printed
// (evidence/0828.md); the draw side is the shipped module under test. Both are checked.

import { describe, expect, it } from "vitest";
import {
  FRIEND_ROW_ATTRIBUTE,
  oslFriendsPanelMarkup,
  routeForOslFriendRow,
  type HomeFriendRow,
} from "./osl-friends-panel-routing";

/** 0828's storage rule: only a bounded inline image is ever a picture. */
const FRIEND_PICTURE_PREFIX = "data:image/";

/** The picture Ada Friend permitted - byte-for-byte the one 0828's read carried (66 bytes). */
const PERMITTED_PICTURE = "data:image/gif;base64,R0lGODlhAQABAIAAAAD/ACwAAAAAAQABAAACAkQBADs=";

/**
 * The picture stored for Cleo Friend, who did **not** permit it. A perfectly valid inline image,
 * so the only reason it must not appear on the panel is the permission - not the format.
 */
const WITHHELD_PICTURE = "data:image/gif;base64,R0lGODlhAQABAIABAP8AACwAAAAAAQABAAACAkQBADs=";

/** A friend as created: what is stored for them, and whether they permitted it to be shown. */
interface CreatedFriend {
  /** The saved friend identifier the row is routed by, `friend:<sha256 hex>`. */
  friendId: string;
  oslUserId: string;
  username: string;
  /** The picture on disk for this friend, if any. */
  storedPicture: string | null;
  /** Whether this friend permitted the panel to show it. */
  picturePermitted: boolean;
  initialColour: string;
}

/**
 * The two friends. Same people, ids, usernames and initial colours 0828's direct read printed,
 * differing in exactly the thing this task varies: the picture permission.
 */
const CREATED_FRIENDS: readonly CreatedFriend[] = [
  {
    friendId: "friend:f8e88443aa9253d65fd9fe5e9c05b855407d0133446703dd45408d0567b8e7c7",
    oslUserId: "900000000000082801",
    username: "Ada Friend",
    storedPicture: PERMITTED_PICTURE,
    picturePermitted: true,
    initialColour: "#2563eb",
  },
  {
    friendId: "friend:065a8d10a8f0d9b449c53523ce8b36b8880f2fdbf618903dd017d66a075dda72",
    oslUserId: "900000000000082803",
    username: "Cleo Friend",
    storedPicture: WITHHELD_PICTURE,
    picturePermitted: false,
    initialColour: "#dc2626",
  },
];

/** The initial a row falls back to: the first visible character of the username, upper-cased. */
function initialFor(username: string): string {
  return (username.trim()[0] ?? "?").toUpperCase();
}

/**
 * The Home row for a created friend, **with** the permitted picture rule: the stored picture is
 * carried only when that friend permitted it and it really is a bounded inline image; otherwise
 * the row carries no picture at all and the panel falls back to the coloured initial.
 */
function createdFriendRow(friend: CreatedFriend): HomeFriendRow {
  const carried = friend.picturePermitted
    && friend.storedPicture !== null
    && friend.storedPicture.startsWith(FRIEND_PICTURE_PREFIX);
  return {
    friendId: friend.friendId,
    oslUserId: friend.oslUserId,
    username: friend.username,
    picture: carried ? friend.storedPicture : null,
    pictureStatus: carried ? "image-present" : "image-absent",
    initial: initialFor(friend.username),
    initialColour: friend.initialColour,
  };
}

/**
 * The same row **without** the permitted picture rule: whatever is stored is carried, permission
 * ignored. This is the fixture the finish line requires the check to fail against.
 */
function rowWithoutPermittedPictureRule(friend: CreatedFriend): HomeFriendRow {
  return {
    friendId: friend.friendId,
    oslUserId: friend.oslUserId,
    username: friend.username,
    picture: friend.storedPicture,
    pictureStatus: friend.storedPicture === null ? "image-absent" : "image-present",
    initial: initialFor(friend.username),
    initialColour: friend.initialColour,
  };
}

/** One Home row as read back off the panel. */
interface ReadRow {
  friendId: string;
  name: string;
  /** The `src` of the picture this row draws, or null when it draws none. */
  pictureSrc: string | null;
  /** The letter this row draws instead, or null when it draws a picture. */
  initial: string | null;
  initialColour: string | null;
}

/**
 * Read the Home rows directly: split the drawn panel into its row elements and pull the id, the
 * name and the avatar out of each one. Row-scoped on purpose - a whole-panel `toContain` cannot
 * tell whose picture ended up in whose row.
 */
function readHomeRowsDirectly(markup: string): ReadRow[] {
  return [...markup.matchAll(/<article class="osl-friend-row">([\s\S]*?)<\/article>/g)].map((row) => {
    const html = row[1];
    const id = html.match(new RegExp(`${FRIEND_ROW_ATTRIBUTE}="([^"]*)"`));
    const name = html.match(/<span class="osl-friend-name">([\s\S]*?)<\/span>/);
    const picture = html.match(/<img class="osl-friend-picture" src="([^"]*)"/);
    const initial = html.match(/<span class="osl-friend-initial" style="background:([^"]*)">([\s\S]*?)<\/span>/);
    return {
      friendId: id ? id[1] : "",
      name: name ? name[1] : "",
      pictureSrc: picture ? picture[1] : null,
      initial: initial ? initial[2] : null,
      initialColour: initial ? initial[1] : null,
    };
  });
}

/**
 * THE CHECK. Draws the panel from the rows it is handed, reads the rows back directly, and
 * requires every one of them to carry the correct name and either the permitted picture or the
 * coloured initial - judged against the *created friends*, i.e. against what each friend actually
 * permitted, not against what the row happens to say.
 */
function checkHomeFriendRows(rows: readonly HomeFriendRow[], label: string): ReadRow[] {
  const markup = oslFriendsPanelMarkup(rows);
  const read = readHomeRowsDirectly(markup);

  console.log(`TASK_0830 fixture=${label} created_friends=${CREATED_FRIENDS.length} rows_read=${read.length}`);
  for (const seen of read) {
    console.log(
      `TASK_0830 fixture=${label} row friend_id=${seen.friendId} name=${seen.name}`
      + ` picture_bytes=${seen.pictureSrc === null ? 0 : seen.pictureSrc.length}`
      + ` picture_src=${seen.pictureSrc === null ? "none" : seen.pictureSrc}`
      + ` initial=${seen.initial ?? "none"} initial_colour=${seen.initialColour ?? "none"}`,
    );
  }

  expect(read).toHaveLength(CREATED_FRIENDS.length);

  CREATED_FRIENDS.forEach((friend, index) => {
    const seen = read[index];

    // The row is this friend's row, and it carries this friend's name and nobody else's.
    expect(seen.friendId).toBe(friend.friendId);
    expect(seen.name).toBe(friend.username);
    for (const other of CREATED_FRIENDS.filter((c) => c.friendId !== friend.friendId)) {
      expect(seen.name).not.toBe(other.username);
      expect(seen.friendId).not.toBe(other.friendId);
    }
    // The name on the row is the name the row's own id routes to.
    const route = routeForOslFriendRow(seen.friendId, rows);
    expect(route.refusal).toBeNull();
    expect((route.route as { username: string }).username).toBe(friend.username);

    if (friend.picturePermitted) {
      // Permitted: the exact stored picture, and no initial standing in for it.
      expect(seen.pictureSrc).toBe(friend.storedPicture);
      expect(seen.initial).toBeNull();
    } else {
      // Not permitted: the coloured initial, and no picture at all.
      expect(seen.pictureSrc).toBeNull();
      expect(seen.initial).toBe(initialFor(friend.username));
      expect(seen.initialColour).toBe(friend.initialColour);
      // ... and the withheld bytes are nowhere on the panel, not merely off this row.
      if (friend.storedPicture !== null) {
        expect(markup.split(friend.storedPicture).length - 1).toBe(0);
      }
    }
  });

  return read;
}

describe("TASK 0830 - prove OSL Friends panel", () => {
  it("two friends with different picture permissions: one permitted, one withheld", () => {
    const permitted = CREATED_FRIENDS.filter((friend) => friend.picturePermitted);
    const withheld = CREATED_FRIENDS.filter((friend) => !friend.picturePermitted);
    console.log(
      `TASK_0830 created=${CREATED_FRIENDS.length} permitted=${permitted.length}`
      + ` withheld=${withheld.length} withheld_stored_bytes=${withheld[0].storedPicture?.length ?? 0}`
      + ` permitted_stored_bytes=${permitted[0].storedPicture?.length ?? 0}`,
    );
    // Both really have a picture stored; they differ only in the permission.
    expect(permitted).toHaveLength(1);
    expect(withheld).toHaveLength(1);
    for (const friend of CREATED_FRIENDS) {
      expect(friend.storedPicture).not.toBeNull();
      expect((friend.storedPicture as string).startsWith(FRIEND_PICTURE_PREFIX)).toBe(true);
    }
    expect(permitted[0].storedPicture).not.toBe(withheld[0].storedPicture);
  });

  it("each Home row read directly has the correct name and permitted picture or initial", () => {
    const rows = CREATED_FRIENDS.map(createdFriendRow);
    const read = checkHomeFriendRows(rows, "permitted-picture-rule");

    const pictures = read.filter((row) => row.pictureSrc !== null);
    const initials = read.filter((row) => row.initial !== null);
    console.log(
      `TASK_0830 names=${read.map((row) => row.name).join("|")}`
      + ` pictures_drawn=${pictures.length} initials_drawn=${initials.length}`
      + ` withheld_picture_occurrences=${oslFriendsPanelMarkup(rows).split(WITHHELD_PICTURE).length - 1}`,
    );
    expect(read.map((row) => row.name)).toEqual(["Ada Friend", "Cleo Friend"]);
    expect(pictures).toHaveLength(1);
    expect(pictures[0].pictureSrc).toBe(PERMITTED_PICTURE);
    expect(initials).toHaveLength(1);
    expect(initials[0].initial).toBe("C");
    expect(initials[0].initialColour).toBe("#dc2626");
  });

  it("pointing the check at a fixture without the permitted picture rule makes the check fail", () => {
    const withRule = CREATED_FRIENDS.map(createdFriendRow);
    const withoutRule = CREATED_FRIENDS.map(rowWithoutPermittedPictureRule);
    // The two fixtures differ, and only on the withheld friend's picture.
    expect(withoutRule).not.toEqual(withRule);
    expect(withoutRule[0]).toEqual(withRule[0]);
    expect(withoutRule[1].picture).toBe(WITHHELD_PICTURE);
    expect(withRule[1].picture).toBeNull();

    // The rule fixture passes the check.
    expect(() => checkHomeFriendRows(withRule, "permitted-picture-rule")).not.toThrow();

    // The no-rule fixture fails it.
    let failure = "";
    try {
      checkHomeFriendRows(withoutRule, "no-permitted-picture-rule");
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
    console.log(
      `TASK_0830 no_rule_check_failed=${failure !== ""}`
      + ` no_rule_withheld_picture_occurrences=${oslFriendsPanelMarkup(withoutRule).split(WITHHELD_PICTURE).length - 1}`
      + ` no_rule_failure=${failure.split("\n")[0]}`,
    );
    expect(failure).not.toBe("");
    // It failed for the right reason: the withheld picture reached Cleo Friend's row.
    expect(oslFriendsPanelMarkup(withoutRule)).toContain(WITHHELD_PICTURE);
    expect(readHomeRowsDirectly(oslFriendsPanelMarkup(withoutRule))[1].pictureSrc).toBe(WITHHELD_PICTURE);
    expect(() => checkHomeFriendRows(withoutRule, "no-permitted-picture-rule")).toThrow();
  });

  it("a row that claims image-absent but still carries bytes draws the initial, not the bytes", () => {
    // The panel's own half of the rule (0829's `friendAvatarMarkup`): a picture is drawn only for
    // a row that says `image-present`. A row that smuggles bytes past the write-side rule is still
    // drawn as an initial, and the bytes never reach the screen.
    const smuggled: HomeFriendRow[] = CREATED_FRIENDS.map(createdFriendRow).map((row, index) =>
      index === 1 ? { ...row, picture: WITHHELD_PICTURE } : row
    );
    expect(smuggled[1].pictureStatus).toBe("image-absent");
    const markup = oslFriendsPanelMarkup(smuggled);
    const read = readHomeRowsDirectly(markup);
    console.log(
      `TASK_0830 smuggled_row_picture=${read[1].pictureSrc ?? "none"} smuggled_row_initial=${read[1].initial ?? "none"}`
      + ` smuggled_bytes_on_panel=${markup.split(WITHHELD_PICTURE).length - 1}`,
    );
    expect(read[1].pictureSrc).toBeNull();
    expect(read[1].initial).toBe("C");
    expect(markup.split(WITHHELD_PICTURE).length - 1).toBe(0);
    // The permitted row is untouched by that.
    expect(read[0].pictureSrc).toBe(PERMITTED_PICTURE);
  });
});
