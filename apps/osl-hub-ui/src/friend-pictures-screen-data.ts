import type { FriendPicturePreviewFriend, FriendPicturesSettings, OwnPicture } from "./friend-pictures-screen";

/**
 * Fixed data for the Linux Friend pictures screenshot (TASK 0788).
 *
 * A 1x1 GIF, same encoding gate 0237's fixtures use, stands in for a real
 * picture: the check only needs an `<img>` with image bytes in it, not a
 * particular photo.
 */
const FIXTURE_PICTURE = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";

export const FRIEND_PICTURES_SCREEN_WINDOW = { width: 900, height: 760 } as const;

// The fallback colours below stand in for backend-provided per-person data
// (a user's persisted avatar colour), not UI chrome — DELIBERATELY not
// osl-tokens.ts values.
export const FRIEND_PICTURES_SCREEN_OWN: OwnPicture = {
  picture: FIXTURE_PICTURE,
  fallbackLetter: "N",
  fallbackColour: "#7c5cff",
} as const;

export const FRIEND_PICTURES_SCREEN_SETTINGS: FriendPicturesSettings = {
  own: FRIEND_PICTURES_SCREEN_OWN,
  hidePictures: false,
  layout: "picture-and-name",
} as const;

/**
 * Two preview friends: one the query granted a picture, one that falls back
 * to the coloured initial, same split the gate 0237 fixtures test.
 */
export const FRIEND_PICTURES_SCREEN_FRIENDS: readonly FriendPicturePreviewFriend[] = [
  {
    personId: "hub-person-0788-ada",
    displayName: "Ada Owner",
    picture: FIXTURE_PICTURE,
    fallbackLetter: "A",
    fallbackColour: "#14b8a6",
  },
  {
    personId: "hub-person-0788-blythe",
    displayName: "Blythe Reyes",
    picture: null,
    fallbackLetter: "B",
    fallbackColour: "#f97316",
  },
] as const;
