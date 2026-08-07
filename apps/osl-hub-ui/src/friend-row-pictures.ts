/**
 * TASK 0237: connect picture visibility to friend rows.
 *
 * A friend row never carries the owner's picture in the list payload:
 * `PersonDto.picture` (apps/osl-hub/src/security.rs) is deliberately empty, and
 * the only way to obtain the image is the protected picture query added by task
 * 0232 (`read_owner_profile_picture_for_friend` in apps/osl-hub/src/main.rs ->
 * `osl_profile::read_active_profile_picture_for_reader`). That query answers for
 * one owner and one reader: it returns `image-present` with the image only when
 * the reader is on the owner's accepted-friend list, and `image-absent` with no
 * image otherwise -- a stranger is redacted, not told the picture exists.
 *
 * This module is the row-level binding for that query. Each row asks the query
 * once, as the signed-in friend, for the picture owned by that row's person, and
 * draws the answer with the task 0236 markup: the picture when the query grants
 * it, the stable coloured first-letter circle from task 0234
 * (`PersonPictureFallbackDto`) when it does not. Nothing else in the row may
 * supply the image, so a row can only ever show a picture the backend agreed to
 * hand this reader.
 */

import { friendPictureMarkup } from "./friend-picture";

/** Mirrors `OwnerProfilePictureDto` (apps/osl-hub/src/osl_profile.rs, task 0231/0232). */
export interface OwnerProfilePictureDto {
  status: string;
  image: string | null;
}

export const OWNER_PICTURE_PRESENT = "image-present";
export const OWNER_PICTURE_ABSENT = "image-absent";

/** Mirrors `PersonPictureFallbackDto` (apps/osl-hub/src/security.rs, task 0234). */
export interface FriendRowPictureFallback {
  letter: string;
  colour: string;
}

/** The part of `PersonDto` a friend row needs to draw its picture cell. */
export interface FriendRowPerson {
  personId: string;
  /** Owner of the protected picture this row would show. */
  oslUserId: string;
  alias: string | null;
  pictureFallback: FriendRowPictureFallback;
}

/**
 * Host boundary for the protected picture query. The real implementation is the
 * Tauri command `read_owner_profile_picture_for_friend`, which fixes the owner
 * to the unlocked account and takes the reader id; the owner is passed here as
 * well so a row can never quietly ask about a different person than the one it
 * draws.
 */
export interface ProtectedFriendPictureQuery {
  readOwnerProfilePictureForFriend(request: {
    ownerOslUserId: string;
    readerOslUserId: string;
  }): Promise<OwnerProfilePictureDto>;
}

export interface FriendRowPictureState {
  personId: string;
  ownerOslUserId: string;
  readerOslUserId: string;
  displayName: string;
  /** Status the protected query answered with, verbatim. */
  status: string;
  /** The owner picture, only ever set from an `image-present` answer. */
  picture: string | null;
  fallbackLetter: string;
  fallbackColour: string;
}

function displayNameFor(person: FriendRowPerson): string {
  const alias = person.alias?.trim();
  return alias && alias.length > 0 ? alias : person.oslUserId;
}

function absentState(person: FriendRowPerson, readerOslUserId: string, status: string): FriendRowPictureState {
  return {
    personId: person.personId,
    ownerOslUserId: person.oslUserId,
    readerOslUserId,
    displayName: displayNameFor(person),
    status,
    picture: null,
    fallbackLetter: person.pictureFallback.letter,
    fallbackColour: person.pictureFallback.colour,
  };
}

/**
 * Ask the protected query for one row. A refused, failed or malformed answer is
 * treated exactly like an absent picture: the row falls back to the coloured
 * initial rather than reusing anything it was handed elsewhere.
 */
export async function connectFriendRowPicture(
  person: FriendRowPerson,
  query: ProtectedFriendPictureQuery,
  signedInFriendOslUserId: string,
): Promise<FriendRowPictureState> {
  let answer: OwnerProfilePictureDto;
  try {
    answer = await query.readOwnerProfilePictureForFriend({
      ownerOslUserId: person.oslUserId,
      readerOslUserId: signedInFriendOslUserId,
    });
  } catch {
    return absentState(person, signedInFriendOslUserId, OWNER_PICTURE_ABSENT);
  }
  if (answer?.status !== OWNER_PICTURE_PRESENT || !answer.image) {
    return absentState(person, signedInFriendOslUserId, answer?.status ?? OWNER_PICTURE_ABSENT);
  }
  return {
    ...absentState(person, signedInFriendOslUserId, answer.status),
    picture: answer.image,
  };
}

/** Bind every row in a friend list to the protected query for the signed-in friend. */
export async function connectFriendRowPictures(
  people: readonly FriendRowPerson[],
  query: ProtectedFriendPictureQuery,
  signedInFriendOslUserId: string,
): Promise<FriendRowPictureState[]> {
  return Promise.all(
    people.map((person) => connectFriendRowPicture(person, query, signedInFriendOslUserId)),
  );
}

export function ownerPictureVisible(state: FriendRowPictureState): boolean {
  return state.picture !== null;
}

export function friendRowPictureMarkup(state: FriendRowPictureState): string {
  return friendPictureMarkup({
    picture: state.picture,
    fallbackLetter: state.fallbackLetter,
    fallbackColour: state.fallbackColour,
  });
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

export function friendRowMarkup(state: FriendRowPictureState): string {
  const visibility = ownerPictureVisible(state) ? "present" : "absent";
  return `<div class="friend-row" data-person-id="${escapeHtml(state.personId)}" data-owner-picture="${visibility}">${friendRowPictureMarkup(state)}<span class="friend-row-name">${escapeHtml(state.displayName)}</span></div>`;
}

export function friendRowsMarkup(states: readonly FriendRowPictureState[]): string {
  return states.map((state) => friendRowMarkup(state)).join("");
}

/**
 * Fold the connected pictures back onto the OSL Chat friend list rows, keyed by
 * person id. A row with no answer keeps its picture unset, so it draws initials.
 */
export function applyFriendRowPictures<Row extends { personId: string; picture?: string | null }>(
  rows: readonly Row[],
  states: readonly FriendRowPictureState[],
): Row[] {
  const pictureByPerson = new Map(states.map((state) => [state.personId, state.picture]));
  return rows.map((row) => ({ ...row, picture: pictureByPerson.get(row.personId) ?? null }));
}
