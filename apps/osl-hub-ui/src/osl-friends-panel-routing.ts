// TASK 0829. Task 0828 (crates/ipc/src/commands.rs, cmd_osl_read_home_friend_rows) writes the
// Home "OSL Friends" panel data: one row per accepted saved friend carrying the username, the
// permitted picture or coloured initial, and the saved friend identifier the row is routed by
// (`friend:<sha256 hex>`). This module is the UI-side connection for that panel: activating a row
// opens exactly the friend page of the friend that row names, and the panel's Back control returns
// to Home. A row id that is not on the panel opens nothing and is refused by name, so a stale or
// forged id can never land on some other friend's page.

/**
 * One panel row. Mirrors `HomeFriendRowDto` (crates/ipc/src/commands.rs, task 0828) in serde
 * camelCase, the shape every other hub DTO reaches this renderer in.
 */
export interface HomeFriendRow {
  /** The saved friend identifier, `friend:<sha256 hex>`. This is what a row routes by. */
  friendId: string;
  /** The friend's OSL identity id. Carried onto the friend page; never used to match a row. */
  oslUserId: string;
  username: string;
  /** The permitted inline picture, or null when the panel withheld it. */
  picture: string | null;
  pictureStatus: "image-present" | "image-absent";
  initial: string;
  initialColour: string;
}

/** The app's Home route (`Route` in main.ts). Back from a friend page lands exactly here. */
export const HOME_ROUTE = "home";

/** The per-friend page a panel row opens. */
export const OSL_FRIEND_PAGE_ROUTE = "osl-friend";

export type OslFriendsPanelRoute =
  | {
    name: typeof OSL_FRIEND_PAGE_ROUTE;
    /** The exact saved friend identifier of the row that was activated. */
    friendId: string;
    oslUserId: string;
    username: string;
  }
  | { name: typeof HOME_ROUTE };

/** Every control on the panel that can move the screen somewhere else. */
export type OslFriendsPanelActivation =
  | { control: "friend-row"; friendId: string }
  | { control: "back" };

export interface OslFriendsPanelRouteResult {
  /** Where the screen goes, or null when the activation was refused. */
  route: OslFriendsPanelRoute | null;
  /** Set exactly when `route` is null; the named refusal a screen can show. */
  refusal: string | null;
}

/** The name a refused friend-page route is refused by. */
export const UNKNOWN_FRIEND_REFUSAL = "OSL: no such friend on the OSL Friends panel";

/** 0828 stores only bounded inline images; anything else is not drawn as a picture. */
const FRIEND_PICTURE_PREFIX = "data:image/";

/** The attribute a row carries its saved friend identifier in. */
export const FRIEND_ROW_ATTRIBUTE = "data-open-osl-friend";

/** The attribute the panel's Back control carries. */
export const BACK_ATTRIBUTE = "data-osl-friends-back";

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/**
 * Activating one friend row opens that friend's page and no other: the row's saved identifier is
 * matched exactly against the panel data, and the matched row's own id is what the route carries.
 * An id that no row on the panel holds routes nowhere and is refused by name.
 */
export function routeForOslFriendRow(
  friendId: string,
  rows: readonly HomeFriendRow[],
): OslFriendsPanelRouteResult {
  const match = rows.find((row) => row.friendId === friendId);
  if (!match) {
    return { route: null, refusal: `${UNKNOWN_FRIEND_REFUSAL}: "${friendId}"` };
  }
  return {
    route: {
      name: OSL_FRIEND_PAGE_ROUTE,
      friendId: match.friendId,
      oslUserId: match.oslUserId,
      username: match.username,
    },
    refusal: null,
  };
}

/** Back from the panel's friend page returns to Home. It reads no row and can never be refused. */
export function routeForOslFriendsPanelBack(): OslFriendsPanelRouteResult {
  return { route: { name: HOME_ROUTE }, refusal: null };
}

/** The one connector every panel control goes through. */
export function resolveOslFriendsPanelRoute(
  activation: OslFriendsPanelActivation,
  rows: readonly HomeFriendRow[],
): OslFriendsPanelRouteResult {
  if (activation.control === "back") return routeForOslFriendsPanelBack();
  return routeForOslFriendRow(activation.friendId, rows);
}

function friendAvatarMarkup(row: HomeFriendRow): string {
  const permitted = row.picture !== null
    && row.pictureStatus === "image-present"
    && row.picture.startsWith(FRIEND_PICTURE_PREFIX);
  if (permitted) {
    return `<img class="osl-friend-picture" src="${escapeHtml(row.picture as string)}" alt=""/>`;
  }
  return `<span class="osl-friend-initial" style="background:${escapeHtml(row.initialColour)}">${escapeHtml(row.initial)}</span>`;
}

/**
 * The panel's rows and its Back control. Each row is one control carrying the saved friend
 * identifier it routes by, so what the screen shows and what
 * {@link resolveOslFriendsPanelRoute} accepts are the same ids.
 */
export function oslFriendsPanelMarkup(rows: readonly HomeFriendRow[]): string {
  const back = `<button class="text-back" type="button" ${BACK_ATTRIBUTE}="${HOME_ROUTE}">← Home</button>`;
  if (!rows.length) {
    return `<section class="osl-friends-panel" aria-label="OSL Friends"><h2>OSL Friends</h2><div class="empty-state"><strong>No friends yet</strong><p>Add one with an invite.</p></div>${back}</section>`;
  }
  const list = rows.map((row) => `<article class="osl-friend-row"><button class="osl-friend-open" type="button" ${FRIEND_ROW_ATTRIBUTE}="${escapeHtml(row.friendId)}">${friendAvatarMarkup(row)}<span class="osl-friend-name">${escapeHtml(row.username)}</span></button></article>`).join("");
  return `<section class="osl-friends-panel" aria-label="OSL Friends"><h2>OSL Friends</h2>${list}${back}</section>`;
}
