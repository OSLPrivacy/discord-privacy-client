/**
 * TASK 0840 - the OSL friend page.
 *
 * This is a deliberately small, pure page renderer.  The account and
 * conversation approvals are shown together so a person can inspect and
 * change the scope before saving.  It also makes the destructive action and
 * the route exit explicit rather than hiding them in a menu.
 */

export interface FriendPageModel {
  readonly friendName: string;
  readonly account: string;
  readonly conversation: string;
  readonly newAccountAllowed: boolean;
}

export const FRIEND_PAGE_TITLE = "Friend";

export const builtInFriendPage: FriendPageModel = {
  friendName: "Avery Chen",
  account: "OSL Chat",
  conversation: "Avery · Private chat",
  newAccountAllowed: true,
};

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

export function friendPageEmptyStateMarkup(): string {
  return `<section class="friend-page friend-page-empty" aria-labelledby="friend-page-empty-title"><h1 id="friend-page-empty-title">No friend selected</h1><p>Choose a friend to review their approved account and conversation.</p></section>`;
}

export function friendPageMarkup(model: FriendPageModel = builtInFriendPage): string {
  const friend = escapeHtml(model.friendName);
  const account = escapeHtml(model.account);
  const conversation = escapeHtml(model.conversation);
  const newAccountState = model.newAccountAllowed ? "Allowed" : "Ask first";
  return `<section class="friend-page" aria-labelledby="friend-page-title">
    <header class="friend-page-header">
      <button class="friend-page-back" type="button" aria-label="back">Back</button>
      <div><p class="friend-page-eyebrow">${friend}</p><h1 id="friend-page-title">${FRIEND_PAGE_TITLE}</h1><p>Control where this friend can reach you.</p></div>
    </header>
    <nav class="friend-page-destinations" aria-label="Friend choices">
      <button type="button">Message</button>
      <button type="button">Pictures</button>
      <button type="button">Privacy</button>
    </nav>
    <div class="friend-page-scope">
      <section class="friend-page-card" aria-labelledby="friend-account-title">
        <div class="friend-page-card-heading"><h2 id="friend-account-title" aria-label="account">Account</h2><span class="friend-page-approved"><span role="img" aria-label="checkmark">✓</span> Approved</span></div>
        <p>${account}</p>
      </section>
      <section class="friend-page-card" aria-labelledby="friend-conversation-title">
        <div class="friend-page-card-heading"><h2 id="friend-conversation-title" aria-label="conversation">Conversation</h2><span class="friend-page-approved"><span role="img" aria-label="checkmark">✓</span> Approved</span></div>
        <p>${conversation}</p>
      </section>
    </div>
    <label class="friend-page-new-account" for="friend-new-account">
      <span><strong>New account</strong><small>Allow this friend’s new accounts in the approved conversation.</small></span>
      <input id="friend-new-account" type="checkbox" role="switch" aria-label="new-account"${model.newAccountAllowed ? " checked" : ""}/>
      <em>${newAccountState}</em>
    </label>
    <footer class="friend-page-actions">
      <button class="friend-page-remove" type="button" aria-label="remove">Remove friend</button>
      <button class="friend-page-block" type="button" aria-label="block">Block</button>
      <span class="friend-page-actions-spacer"></span>
      <button class="friend-page-cancel" type="button" aria-label="cancel">Cancel</button>
      <button class="friend-page-save" type="button" aria-label="save">Save</button>
    </footer>
  </section>`;
}
