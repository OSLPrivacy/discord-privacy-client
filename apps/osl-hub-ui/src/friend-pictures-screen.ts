/**
 * TASK 0788 - the Friend pictures screen.
 *
 * Gate 0237 (`friend-row-pictures.ts`) decides, row by row, whether a viewer
 * gets the owner's protected picture or the coloured-initial fallback from
 * gate 0236 (`friend-picture.ts`). This module is the settings screen that
 * sits above that decision: it draws the six controls the task names --
 * own-picture, hide pictures, the three layout choices (picture-and-name,
 * compact, grid) and Reset -- and a live preview of a small friend list drawn
 * with the current draft so the effect of each control is visible before
 * Save.
 *
 * The module holds no state of its own beyond the draft/saved pair; drawing
 * is pure functions of that pair plus a fixed list of preview friends, same
 * shape as `renderAccountScreen`.
 */

import { friendPictureMarkup } from "./friend-picture";

export type PictureLayoutMode = "picture-and-name" | "compact" | "grid";

/** The three layout choices, in the order the screen offers them. */
export const PICTURE_LAYOUT_MODES: readonly PictureLayoutMode[] = [
  "picture-and-name",
  "compact",
  "grid",
] as const;

export const PICTURE_LAYOUT_LABELS: Readonly<Record<PictureLayoutMode, string>> = {
  "picture-and-name": "Picture and name",
  compact: "Compact",
  grid: "Grid",
};

export const DEFAULT_LAYOUT_MODE: PictureLayoutMode = "picture-and-name";
export const DEFAULT_HIDE_PICTURES = false;

function isLayoutMode(value: string): value is PictureLayoutMode {
  return (PICTURE_LAYOUT_MODES as readonly string[]).includes(value);
}

export function layoutLabel(mode: PictureLayoutMode): string {
  if (!isLayoutMode(mode)) throw new Error(`Unknown picture layout: ${mode}`);
  return PICTURE_LAYOUT_LABELS[mode];
}

/** One friend the preview draws. Fixed data, never the real friend list. */
export interface FriendPicturePreviewFriend {
  readonly personId: string;
  readonly displayName: string;
  readonly picture: string | null;
  readonly fallbackLetter: string;
  readonly fallbackColour: string;
}

/** What the screen knows about the signed-in account's own picture. */
export interface OwnPicture {
  readonly picture: string | null;
  readonly fallbackLetter: string;
  readonly fallbackColour: string;
}

export interface FriendPicturesSettings {
  readonly own: OwnPicture;
  readonly hidePictures: boolean;
  readonly layout: PictureLayoutMode;
}

export interface FriendPicturesScreenState {
  readonly saved: FriendPicturesSettings;
  readonly draft: FriendPicturesSettings;
}

/** Reset puts hide-pictures and layout back to these, and clears the own picture. */
export function defaultFriendPicturesSettings(own: OwnPicture): FriendPicturesSettings {
  return {
    own: { ...own, picture: null },
    hidePictures: DEFAULT_HIDE_PICTURES,
    layout: DEFAULT_LAYOUT_MODE,
  };
}

export function friendPicturesScreenState(settings: FriendPicturesSettings): FriendPicturesScreenState {
  return { saved: settings, draft: settings };
}

export function setOwnPicture(
  state: FriendPicturesScreenState,
  picture: string | null,
): FriendPicturesScreenState {
  return {
    saved: state.saved,
    draft: { ...state.draft, own: { ...state.draft.own, picture } },
  };
}

export function setHidePictures(
  state: FriendPicturesScreenState,
  hidePictures: boolean,
): FriendPicturesScreenState {
  return { saved: state.saved, draft: { ...state.draft, hidePictures } };
}

export function setLayout(
  state: FriendPicturesScreenState,
  layout: PictureLayoutMode,
): FriendPicturesScreenState {
  if (!isLayoutMode(layout)) throw new Error(`Unknown picture layout: ${layout}`);
  return { saved: state.saved, draft: { ...state.draft, layout } };
}

export const RESET_FRIEND_PICTURES_EXPLANATION =
  "Removes your own picture, shows friend pictures again, and sets the layout back to picture and name.";

export function resetFriendPicturesSettings(state: FriendPicturesScreenState): FriendPicturesScreenState {
  return { saved: state.saved, draft: defaultFriendPicturesSettings(state.draft.own) };
}

export interface SavedFriendPicturesSettings {
  readonly ownPictureSet: boolean;
  readonly hidePictures: boolean;
  readonly layout: PictureLayoutMode;
}

export function friendPicturesSavePayload(settings: FriendPicturesSettings): SavedFriendPicturesSettings {
  return {
    ownPictureSet: settings.own.picture !== null,
    hidePictures: settings.hidePictures,
    layout: settings.layout,
  };
}

export function saveFriendPicturesSettings(state: FriendPicturesScreenState): {
  state: FriendPicturesScreenState;
  saved: SavedFriendPicturesSettings;
} {
  return {
    state: { saved: state.draft, draft: state.draft },
    saved: friendPicturesSavePayload(state.draft),
  };
}

function settingsEqual(a: FriendPicturesSettings, b: FriendPicturesSettings): boolean {
  return a.own.picture === b.own.picture && a.hidePictures === b.hidePictures && a.layout === b.layout;
}

export function friendPicturesChanged(state: FriendPicturesScreenState): boolean {
  return !settingsEqual(state.draft, state.saved);
}

export function friendPicturesStatusLine(state: FriendPicturesScreenState): string {
  return friendPicturesChanged(state) ? "Changed - not saved yet." : "Friend picture settings saved.";
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

function ownPictureMarkup(state: FriendPicturesScreenState): string {
  const own = state.draft.own;
  const preview = friendPictureMarkup({
    picture: own.picture,
    fallbackLetter: own.fallbackLetter,
    fallbackColour: own.fallbackColour,
  });
  return [
    `<div class="friend-pictures-control" data-friend-pictures-control="own-picture">`,
    `<span class="friend-pictures-control-head">`,
    `<span class="friend-pictures-name">Own picture</span>`,
    `<span class="friend-pictures-detail">Shown to friends who are on your accepted list. Strangers never see it.</span>`,
    `</span>`,
    `<span class="friend-pictures-own-preview" data-own-picture="${own.picture ? "set" : "unset"}">${preview}</span>`,
    `<span class="friend-pictures-own-actions">`,
    `<button type="button" class="friend-pictures-action" data-friend-pictures-action="own-picture-choose">Choose picture</button>`,
    own.picture
      ? `<button type="button" class="friend-pictures-action friend-pictures-action-quiet" data-friend-pictures-action="own-picture-remove">Remove picture</button>`
      : "",
    `</span>`,
    `</div>`,
  ].join("");
}

function hidePicturesMarkup(state: FriendPicturesScreenState): string {
  const checked = state.draft.hidePictures;
  return [
    `<div class="friend-pictures-control" data-friend-pictures-control="hide-pictures">`,
    `<span class="friend-pictures-control-head">`,
    `<label class="friend-pictures-name" for="friend-pictures-hide-toggle">Hide pictures</label>`,
    `<span class="friend-pictures-detail">Turns every friend picture in your rows into the coloured initial, on this device only.</span>`,
    `</span>`,
    `<span class="friend-pictures-toggle-slot">`,
    `<input type="checkbox" id="friend-pictures-hide-toggle" class="friend-pictures-toggle"`,
    ` data-friend-pictures-toggle="hide-pictures" role="switch"`,
    ` aria-checked="${checked ? "true" : "false"}"${checked ? " checked" : ""}/>`,
    `</span>`,
    `</div>`,
  ].join("");
}

function layoutMarkup(state: FriendPicturesScreenState): string {
  const options = PICTURE_LAYOUT_MODES.map((mode) => {
    const checked = state.draft.layout === mode;
    const id = `friend-pictures-layout-${mode}`;
    return [
      `<label class="friend-pictures-layout-option" for="${id}" data-friend-pictures-layout-option="${mode}">`,
      `<input type="radio" id="${id}" name="friend-pictures-layout" class="friend-pictures-layout-input"`,
      ` data-friend-pictures-layout="${mode}" value="${mode}"${checked ? " checked" : ""}/>`,
      `<span>${escapeHtml(layoutLabel(mode))}</span>`,
      `</label>`,
    ].join("");
  }).join("");
  return [
    `<fieldset class="friend-pictures-control friend-pictures-layout" data-friend-pictures-control="layout">`,
    `<legend class="friend-pictures-name">Layout</legend>`,
    `<span class="friend-pictures-detail">How friend rows show a picture next to the name.</span>`,
    `<div class="friend-pictures-layout-options">${options}</div>`,
    `</fieldset>`,
  ].join("");
}

function previewFriendMarkup(state: FriendPicturesScreenState, friend: FriendPicturePreviewFriend): string {
  const picture = state.draft.hidePictures ? null : friend.picture;
  const markup = friendPictureMarkup({
    picture,
    fallbackLetter: friend.fallbackLetter,
    fallbackColour: friend.fallbackColour,
  });
  return [
    `<li class="friend-pictures-preview-row" data-person-id="${escapeHtml(friend.personId)}"`,
    ` data-owner-picture="${picture ? "present" : "absent"}">`,
    markup,
    state.draft.layout === "compact" ? "" : `<span class="friend-pictures-preview-name">${escapeHtml(friend.displayName)}</span>`,
    `</li>`,
  ].join("");
}

function previewMarkup(
  state: FriendPicturesScreenState,
  friends: readonly FriendPicturePreviewFriend[],
): string {
  return [
    `<section class="friend-pictures-preview" aria-label="Preview" data-friend-pictures-layout="${state.draft.layout}">`,
    `<h3 class="friend-pictures-preview-heading">Preview</h3>`,
    `<ul class="friend-pictures-preview-list" data-layout="${state.draft.layout}">`,
    friends.map((friend) => previewFriendMarkup(state, friend)).join(""),
    `</ul>`,
    `</section>`,
  ].join("");
}

export const FRIEND_PICTURES_SCREEN_TITLE = "Friend pictures";

export function renderFriendPicturesScreen(
  state: FriendPicturesScreenState,
  friends: readonly FriendPicturePreviewFriend[],
): string {
  return [
    `<section class="friend-pictures-screen">`,
    `<header class="friend-pictures-screen-header">`,
    `<h1 class="friend-pictures-screen-heading">${FRIEND_PICTURES_SCREEN_TITLE}</h1>`,
    `<p class="friend-pictures-screen-intro">Own picture, whether friend pictures show at all, and the layout friend rows draw them with.</p>`,
    `</header>`,
    ownPictureMarkup(state),
    hidePicturesMarkup(state),
    layoutMarkup(state),
    previewMarkup(state, friends),
    `<section class="friend-pictures-actions" aria-label="Save or reset friend picture settings">`,
    `<button type="button" class="friend-pictures-action-button" data-friend-pictures-action="save">Save</button>`,
    `<button type="button" class="friend-pictures-action-button friend-pictures-action-button-quiet" data-friend-pictures-action="reset">Reset</button>`,
    `<p class="friend-pictures-reset-text">${escapeHtml(RESET_FRIEND_PICTURES_EXPLANATION)}</p>`,
    `<p class="friend-pictures-status" role="status" data-changed="${friendPicturesChanged(state) ? "yes" : "no"}">${escapeHtml(friendPicturesStatusLine(state))}</p>`,
    `</section>`,
    `</section>`,
  ].join("");
}

/**
 * Mount the screen and keep its state. Same shape as `attachAccountScreen`:
 * one state value for a mounted screen, redrawn after every change.
 */
export function attachFriendPicturesScreen(
  mount: HTMLElement,
  settings: FriendPicturesSettings,
  friends: readonly FriendPicturePreviewFriend[],
  handlers: {
    onChoosePicture?: () => Promise<string | null> | string | null;
    onSave?: (saved: SavedFriendPicturesSettings) => void;
  } = {},
): void {
  let state = friendPicturesScreenState(settings);
  const draw = (): void => {
    mount.innerHTML = renderFriendPicturesScreen(state, friends);
  };
  mount.addEventListener("change", (event) => {
    const target = event.target as HTMLInputElement | null;
    if (!target) return;
    if (target.dataset.friendPicturesToggle === "hide-pictures") {
      state = setHidePictures(state, target.checked);
      draw();
      return;
    }
    const layout = target.dataset.friendPicturesLayout;
    if (layout && target.checked) {
      state = setLayout(state, layout as PictureLayoutMode);
      draw();
    }
  });
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest?.("[data-friend-pictures-action]") as HTMLElement | null;
    if (!button) return;
    const action = button.dataset.friendPicturesAction;
    if (action === "own-picture-choose") {
      const result = handlers.onChoosePicture?.();
      Promise.resolve(result).then((picture) => {
        if (picture === undefined) return;
        state = setOwnPicture(state, picture);
        draw();
      });
      return;
    }
    if (action === "own-picture-remove") {
      state = setOwnPicture(state, null);
      draw();
      return;
    }
    if (action === "reset") {
      state = resetFriendPicturesSettings(state);
      draw();
      return;
    }
    if (action === "save") {
      const result = saveFriendPicturesSettings(state);
      state = result.state;
      handlers.onSave?.(result.saved);
      draw();
    }
  });
  draw();
}
