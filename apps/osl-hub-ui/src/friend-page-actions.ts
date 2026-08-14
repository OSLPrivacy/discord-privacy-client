/**
 * The compact action surface for an accepted friend.  The markers are kept on
 * the rendered elements (rather than inferred from their words) so the page
 * contract remains checkable when copy or layout changes.
 */

export const FRIEND_PAGE_ELEMENT_NAMES = [
  "picture",
  "name",
  "tick",
  "Message",
  "account reach",
  "whitelist",
  "warning",
  "remove",
  "block",
] as const;

export type FriendPageElementName = typeof FRIEND_PAGE_ELEMENT_NAMES[number];

export interface AcceptedFriendPageModel {
  readonly personId: string;
  readonly name: string;
  readonly pictureMarkup: string;
  readonly accepted: boolean;
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

/** Render every action that belongs on an accepted friend's page. */
export function acceptedFriendPageActionsMarkup(model: AcceptedFriendPageModel): string {
  const personId = escapeHtml(model.personId);
  const acceptedTick = model.accepted
    ? '<span class="friend-page-tick" data-friend-page-element="tick" aria-label="Accepted friend">✓</span>'
    : "";

  return `<article class="friend-page-actions" data-friend-page="${personId}"><header><span class="friend-page-picture" data-friend-page-element="picture">${model.pictureMarkup}</span><div><strong data-friend-page-element="name">${escapeHtml(model.name)}</strong>${acceptedTick}</div><button class="button compact" type="button" data-friend-page-element="Message" data-osl-chat-open="${personId}">Message</button></header><div class="friend-page-action-list"><button class="button compact" type="button" data-friend-page-element="account reach">Account reach</button><button class="button compact" type="button" data-friend-page-element="whitelist" data-whitelist-everywhere-person="${personId}">Whitelist everywhere</button><p class="warning" data-friend-page-element="warning" role="note"><strong>Warning</strong> Changes to friend access apply only on this device.</p><button class="button compact danger" type="button" data-friend-page-element="remove" data-remove-person="${personId}">Remove friend</button><button class="button compact danger" type="button" data-friend-page-element="block" data-block-person="${personId}">Block friend</button></div></article>`;
}

/** Count named elements on one rendered friend page, once each at most. */
export function acceptedFriendPageElementCount(markup: string): number {
  return FRIEND_PAGE_ELEMENT_NAMES.filter((name) =>
    markup.includes(`data-friend-page-element="${name}"`)).length;
}

export function hasCompleteAcceptedFriendPage(markup: string): boolean {
  return acceptedFriendPageElementCount(markup) === FRIEND_PAGE_ELEMENT_NAMES.length;
}
