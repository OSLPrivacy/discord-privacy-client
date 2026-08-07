import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  OWNER_PICTURE_ABSENT,
  OWNER_PICTURE_PRESENT,
  type FriendRowPerson,
  type OwnerProfilePictureDto,
  type ProtectedFriendPictureQuery,
  connectFriendRowPictures,
  friendRowsMarkup,
  ownerPictureVisible,
} from "./friend-row-pictures";
import { applyFriendRowPictures } from "./friend-row-pictures";
import { type OslChatFriend, oslChatsViewMarkup } from "./osl-chats-view";

/**
 * TASK 0237: connect picture visibility to friend rows.
 *
 * Two fixtures differing only in who is signed in: a stranger the owner has
 * never accepted, and an accepted friend. The rows, the owner, the stored
 * picture and the markup path are identical in both.
 */

const OWNER_OSL_USER_ID = "900000000000023700";
const FRIEND_OSL_USER_ID = "900000000000023701";
const STRANGER_OSL_USER_ID = "900000000000023799";
const OWNER_PICTURE =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";
const OWNER_PICTURE_BODY = OWNER_PICTURE.slice("data:image/gif;base64,".length);

/**
 * Fixture backend for `read_owner_profile_picture_for_friend`, mirroring
 * `osl_profile::read_profile_picture_for_reader_with_key` (task 0232): the
 * reader must be on the owner's accepted-friend list, otherwise the answer is
 * `image-absent` with no image and no hint that a picture exists.
 */
function fixtureProtectedPictureBackend(options: {
  acceptedFriendIds: readonly string[];
  storedPictures: Readonly<Record<string, string>>;
}): ProtectedFriendPictureQuery & { calls: { ownerOslUserId: string; readerOslUserId: string }[] } {
  const calls: { ownerOslUserId: string; readerOslUserId: string }[] = [];
  return {
    calls,
    async readOwnerProfilePictureForFriend(request): Promise<OwnerProfilePictureDto> {
      calls.push(request);
      if (!options.acceptedFriendIds.includes(request.readerOslUserId)) {
        return { status: OWNER_PICTURE_ABSENT, image: null };
      }
      const image = options.storedPictures[request.ownerOslUserId] ?? null;
      return image
        ? { status: OWNER_PICTURE_PRESENT, image }
        : { status: OWNER_PICTURE_ABSENT, image: null };
    },
  };
}

const FIXTURE_ROWS: readonly FriendRowPerson[] = [
  {
    personId: "hub-person-0237-owner",
    oslUserId: OWNER_OSL_USER_ID,
    alias: "Ada Owner",
    pictureFallback: { letter: "A", colour: "#14b8a6" },
  },
];

function backend() {
  return fixtureProtectedPictureBackend({
    acceptedFriendIds: [FRIEND_OSL_USER_ID],
    storedPictures: { [OWNER_OSL_USER_ID]: OWNER_PICTURE },
  });
}

function countOf(markup: string, needle: string): number {
  return markup.split(needle).length - 1;
}

function chatFriends(): OslChatFriend[] {
  return [{
    personId: "hub-person-0237-owner",
    nickname: "Ada Owner",
    verified: true,
    ready: true,
    preview: null,
    previewVisible: true,
    unreadCount: 0,
  }];
}

const ARTIFACT_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "screenshots", "artifacts");

/** Fixed-size shell so task 0238 can pair the two screenshots at one size. */
function fixtureDocument(title: string, rowsMarkup: string): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<title>${title}</title>
<style>
  body { width: 480px; height: 220px; margin: 0; padding: 24px; box-sizing: border-box; background: #0b1220; color: #e6edf7; font-family: system-ui, sans-serif; }
  h1 { font-size: 14px; margin: 0 0 16px; }
  .friend-row { display: grid; grid-template-columns: 38px minmax(0, 1fr); align-items: center; gap: 10px; padding: 8px 0; }
  .friend-picture { width: 38px; height: 38px; display: block; object-fit: cover; border: 1px solid #2b3b57; }
  .friend-picture-fallback { width: 38px; height: 38px; display: grid; place-items: center; color: #06251f; font-size: 15px; font-weight: 700; }
  .friend-row-name { font-size: 13px; font-weight: 600; }
</style>
</head>
<body>
<h1>${title}</h1>
<div class="friend-list">${rowsMarkup}</div>
</body>
</html>
`;
}

function writeFixture(name: string, title: string, rowsMarkup: string): string {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const path = join(ARTIFACT_DIR, name);
  writeFileSync(path, fixtureDocument(title, rowsMarkup), "utf8");
  return readFileSync(path, "utf8");
}

describe("task 0237: connect picture visibility to friend rows", () => {
  it("a stranger fixture renders no owner picture while a friend fixture does", async () => {
    const strangerBackend = backend();
    const friendBackend = backend();

    const strangerRows = await connectFriendRowPictures(
      FIXTURE_ROWS,
      strangerBackend,
      STRANGER_OSL_USER_ID,
    );
    const friendRows = await connectFriendRowPictures(
      FIXTURE_ROWS,
      friendBackend,
      FRIEND_OSL_USER_ID,
    );

    // Every row went through the protected query, for its own owner, as the
    // signed-in reader -- the row has no other source for the image.
    expect(strangerBackend.calls).toEqual([
      { ownerOslUserId: OWNER_OSL_USER_ID, readerOslUserId: STRANGER_OSL_USER_ID },
    ]);
    expect(friendBackend.calls).toEqual([
      { ownerOslUserId: OWNER_OSL_USER_ID, readerOslUserId: FRIEND_OSL_USER_ID },
    ]);

    const strangerFixture = writeFixture(
      "task-0237-stranger-fixture.html",
      "Friends - signed in as a stranger",
      friendRowsMarkup(strangerRows),
    );
    const friendFixture = writeFixture(
      "task-0237-friend-fixture.html",
      "Friends - signed in as an accepted friend",
      friendRowsMarkup(friendRows),
    );

    const strangerPictures = countOf(strangerFixture, 'class="friend-picture"');
    const friendPictures = countOf(friendFixture, 'class="friend-picture"');
    const strangerFallbacks = countOf(strangerFixture, 'class="friend-picture-fallback"');
    const friendFallbacks = countOf(friendFixture, 'class="friend-picture-fallback"');
    const strangerLeaks = countOf(strangerFixture, OWNER_PICTURE_BODY);
    const friendImages = countOf(friendFixture, OWNER_PICTURE_BODY);

    console.log(
      `TASK_0237 stranger.status=${strangerRows[0].status} stranger.owner_picture_count=${strangerPictures} stranger.fallback_count=${strangerFallbacks} stranger.owner_image_bytes_in_markup=${strangerLeaks}`,
    );
    console.log(
      `TASK_0237 friend.status=${friendRows[0].status} friend.owner_picture_count=${friendPictures} friend.fallback_count=${friendFallbacks} friend.owner_image_bytes_in_markup=${friendImages}`,
    );

    // Stranger fixture: no owner picture anywhere in the rendered row.
    expect(strangerRows[0].status).toBe(OWNER_PICTURE_ABSENT);
    expect(ownerPictureVisible(strangerRows[0])).toBe(false);
    expect(strangerPictures).toBe(0);
    expect(strangerLeaks).toBe(0);
    expect(strangerFixture).toContain('data-owner-picture="absent"');
    expect(strangerFallbacks).toBe(1);
    expect(strangerFixture).toContain("background-color: #14b8a6");
    expect(strangerFixture).toContain(">A</span>");

    // Friend fixture: the owner picture, from the protected answer.
    expect(friendRows[0].status).toBe(OWNER_PICTURE_PRESENT);
    expect(ownerPictureVisible(friendRows[0])).toBe(true);
    expect(friendRows[0].picture).toBe(OWNER_PICTURE);
    expect(friendPictures).toBe(1);
    expect(friendImages).toBe(1);
    expect(friendFixture).toContain('data-owner-picture="present"');
    expect(friendFallbacks).toBe(0);
  });

  it("the OSL Chat friend list draws the same protected answer", async () => {
    const strangerRows = await connectFriendRowPictures(FIXTURE_ROWS, backend(), STRANGER_OSL_USER_ID);
    const friendRows = await connectFriendRowPictures(FIXTURE_ROWS, backend(), FRIEND_OSL_USER_ID);

    const strangerView = oslChatsViewMarkup({
      friends: applyFriendRowPictures(chatFriends(), strangerRows),
      activePersonId: null,
      messages: [],
      draft: "",
      busy: false,
      viewOnce: false,
    });
    const friendView = oslChatsViewMarkup({
      friends: applyFriendRowPictures(chatFriends(), friendRows),
      activePersonId: null,
      messages: [],
      draft: "",
      busy: false,
      viewOnce: false,
    });

    const strangerAvatarPictures = countOf(strangerView, "img class=\"osl-chat-avatar friend-picture");
    const friendAvatarPictures = countOf(friendView, "img class=\"osl-chat-avatar friend-picture");
    console.log(
      `TASK_0237 chat_list stranger.avatar_picture_count=${strangerAvatarPictures} friend.avatar_picture_count=${friendAvatarPictures}`,
    );

    expect(strangerAvatarPictures).toBe(0);
    expect(countOf(strangerView, OWNER_PICTURE_BODY)).toBe(0);
    expect(strangerView).toContain('class="osl-chat-avatar "');
    expect(friendAvatarPictures).toBe(1);
    expect(countOf(friendView, OWNER_PICTURE_BODY)).toBe(1);
  });

  it("a query refusal leaves the row on its coloured initial", async () => {
    const refusing: ProtectedFriendPictureQuery = {
      async readOwnerProfilePictureForFriend() {
        throw new Error("OSL: picture access blocked");
      },
    };
    const rows = await connectFriendRowPictures(FIXTURE_ROWS, refusing, FRIEND_OSL_USER_ID);
    expect(rows[0].status).toBe(OWNER_PICTURE_ABSENT);
    expect(rows[0].picture).toBeNull();
    expect(friendRowsMarkup(rows)).toContain('class="friend-picture-fallback"');
    expect(friendRowsMarkup(rows)).not.toContain('class="friend-picture"');
  });
});
