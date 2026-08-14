/**
 * TASK 0788 - the Friend pictures screen controls, and the two things the
 * screenshot check next door cannot prove on its own: that hide-pictures
 * actually blanks every preview picture, and that Reset puts every control
 * back including the own picture.
 */
import { describe, expect, it } from "vitest";

import {
  DEFAULT_HIDE_PICTURES,
  DEFAULT_LAYOUT_MODE,
  FRIEND_PICTURES_SCREEN_TITLE,
  PICTURE_LAYOUT_LABELS,
  PICTURE_LAYOUT_MODES,
  friendPicturesChanged,
  friendPicturesSavePayload,
  friendPicturesScreenState,
  friendPicturesStatusLine,
  layoutLabel,
  renderFriendPicturesScreen,
  resetFriendPicturesSettings,
  saveFriendPicturesSettings,
  setHidePictures,
  setLayout,
  setOwnPicture,
} from "./friend-pictures-screen";
import {
  FRIEND_PICTURES_SCREEN_FRIENDS,
  FRIEND_PICTURES_SCREEN_SETTINGS,
} from "./friend-pictures-screen-data";

const state = () => friendPicturesScreenState(FRIEND_PICTURES_SCREEN_SETTINGS);

describe("the six Friend pictures controls", () => {
  it("draws the own-picture, hide-pictures, layout and reset controls", () => {
    const html = renderFriendPicturesScreen(state(), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(html).toContain(FRIEND_PICTURES_SCREEN_TITLE);
    expect(html).toContain('data-friend-pictures-control="own-picture"');
    expect(html).toContain('data-friend-pictures-control="hide-pictures"');
    expect(html).toContain('data-friend-pictures-control="layout"');
    expect(html).toContain('data-friend-pictures-action="reset"');
    expect(html).toContain('data-friend-pictures-action="save"');
  });

  it("offers exactly the three named layouts, in order", () => {
    expect(PICTURE_LAYOUT_MODES).toEqual(["picture-and-name", "compact", "grid"]);
    expect(Object.keys(PICTURE_LAYOUT_LABELS)).toEqual(["picture-and-name", "compact", "grid"]);
    const html = renderFriendPicturesScreen(state(), FRIEND_PICTURES_SCREEN_FRIENDS);
    for (const mode of PICTURE_LAYOUT_MODES) {
      expect(html).toContain(`data-friend-pictures-layout-option="${mode}"`);
      expect(html).toContain(`data-friend-pictures-layout="${mode}"`);
      expect(html).toContain(layoutLabel(mode));
    }
  });

  it("rejects an unknown layout", () => {
    expect(() => setLayout(state(), "list" as never)).toThrow("Unknown picture layout: list");
    expect(() => layoutLabel("list" as never)).toThrow("Unknown picture layout: list");
  });
});

describe("own picture", () => {
  it("shows the fixture picture, and clears back to the fallback", () => {
    const withPicture = state();
    expect(withPicture.draft.own.picture).not.toBeNull();
    const cleared = setOwnPicture(withPicture, null);
    expect(cleared.draft.own.picture).toBeNull();
    const html = renderFriendPicturesScreen(cleared, FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(html).toContain('data-own-picture="unset"');
    expect(html).not.toContain("Remove picture");
  });

  it("offers Remove picture only once a picture is set", () => {
    const html = renderFriendPicturesScreen(state(), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(html).toContain('data-own-picture="set"');
    expect(html).toContain("Remove picture");
  });
});

describe("hide pictures", () => {
  it("blanks every preview picture, leaving only fallbacks, when turned on", () => {
    const hidden = setHidePictures(state(), true);
    const html = renderFriendPicturesScreen(hidden, FRIEND_PICTURES_SCREEN_FRIENDS);
    const preview = html.slice(html.indexOf('class="friend-pictures-preview"'));
    expect(preview).not.toContain('<img class="friend-picture"');
    expect(preview.match(/data-owner-picture="present"/gu)).toBeNull();
    expect(preview.match(/data-owner-picture="absent"/gu)).toHaveLength(FRIEND_PICTURES_SCREEN_FRIENDS.length);
    // The own-picture control is not affected: it shows what you set, not what friends see.
    expect(html).toContain('data-own-picture="set"');
  });

  it("shows the granted picture and the fallback side by side when off", () => {
    const html = renderFriendPicturesScreen(state(), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(html).toContain('data-owner-picture="present"');
    expect(html).toContain('data-owner-picture="absent"');
    expect(html).toContain('class="friend-picture-fallback"');
  });
});

describe("layout changes the preview markup", () => {
  it("keeps the name in picture-and-name and grid, and drops it in compact", () => {
    const withName = renderFriendPicturesScreen(state(), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(withName).toContain("friend-pictures-preview-name");

    const compact = renderFriendPicturesScreen(setLayout(state(), "compact"), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(compact).not.toContain("friend-pictures-preview-name");
    expect(compact).toContain('data-layout="compact"');

    const grid = renderFriendPicturesScreen(setLayout(state(), "grid"), FRIEND_PICTURES_SCREEN_FRIENDS);
    expect(grid).toContain("friend-pictures-preview-name");
    expect(grid).toContain('data-layout="grid"');
  });
});

describe("save and reset", () => {
  it("reports changed only once a control differs from saved", () => {
    expect(friendPicturesChanged(state())).toBe(false);
    expect(friendPicturesStatusLine(state())).toBe("Friend picture settings saved.");
    const changed = setHidePictures(state(), true);
    expect(friendPicturesChanged(changed)).toBe(true);
    expect(friendPicturesStatusLine(changed)).toBe("Changed - not saved yet.");
  });

  it("resets hide-pictures, layout and the own picture together", () => {
    const messed = setOwnPicture(setLayout(setHidePictures(state(), true), "grid"), null);
    const reset = resetFriendPicturesSettings(messed);
    expect(reset.draft.hidePictures).toBe(DEFAULT_HIDE_PICTURES);
    expect(reset.draft.layout).toBe(DEFAULT_LAYOUT_MODE);
    expect(reset.draft.own.picture).toBeNull();
    // Reset does not touch the saved half, so a follow-up Save still records the change.
    expect(reset.saved).toBe(messed.saved);
  });

  it("hands the native side facts only, no picture data URI", () => {
    const payload = friendPicturesSavePayload(state().draft);
    expect(payload).toEqual({ ownPictureSet: true, hidePictures: false, layout: "picture-and-name" });
    const saved = saveFriendPicturesSettings(setHidePictures(state(), true));
    expect(saved.saved).toEqual({ ownPictureSet: true, hidePictures: true, layout: "picture-and-name" });
    expect(friendPicturesChanged(saved.state)).toBe(false);
  });
});

describe("escaping", () => {
  it("escapes an attacker-controlled preview name", () => {
    const hostile = [
      { ...FRIEND_PICTURES_SCREEN_FRIENDS[0], displayName: '<script>alert(1)</script>' },
    ];
    const html = renderFriendPicturesScreen(state(), hostile);
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
  });
});
