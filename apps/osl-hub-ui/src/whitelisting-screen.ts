/**
 * The Whitelisting screen: the list of conversations OSL is allowed to touch,
 * and the six controls that change it.
 *
 * The whole screen is one pure function of one state object, so the shipped
 * Settings section (main.ts `whitelistingSettingsContent`) and the Linux capture
 * (screenshots/capture-linux-whitelisting.mjs) render the same pixels from the
 * same code. A screenshot-only copy of this markup would prove nothing about
 * what ships.
 *
 * Two rules the controls obey, because getting them wrong is how a user loses a
 * conversation they meant to keep:
 *   - "Select all" and "Clear all" act on what the search is showing, not on the
 *     whole list. A cleared search is the only way to touch every row.
 *   - Nothing is written until "Save". "Reset" throws away the unsaved edits and
 *     puts the saved answer back; it never clears the list.
 */

export interface WhitelistingConversation {
  readonly id: string;
  /** The app account the conversation lives in, e.g. "Discord · @ada". */
  readonly account: string;
  readonly name: string;
  /** "Group", "Direct messages", "Channel", "Space". */
  readonly kind: string;
}

export interface WhitelistingScreenState {
  readonly conversations: readonly WhitelistingConversation[];
  /** Ids that are allowed on disk right now. */
  readonly saved: readonly string[];
  /** Ids ticked on screen, saved or not. */
  readonly draft: readonly string[];
  readonly search: string;
  readonly busy: boolean;
}

export interface WhitelistingScreenView {
  readonly search: string;
  readonly searching: boolean;
  readonly matches: readonly WhitelistingConversation[];
  readonly matchCount: number;
  readonly totalCount: number;
  readonly selectedCount: number;
  readonly selectedMatchCount: number;
  readonly clearedMatchCount: number;
  /** At least one ticked and at least one unticked row is on screen. */
  readonly mixed: boolean;
  readonly dirty: boolean;
  readonly changeCount: number;
  readonly resultLine: string;
  readonly selectAllEnabled: boolean;
  readonly clearAllEnabled: boolean;
  readonly saveEnabled: boolean;
  readonly resetEnabled: boolean;
}

export const whitelistingSearchInputId = "whitelisting-search";
export const whitelistingTitle = "Whitelisting";

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function sameIds(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) return false;
  const other = new Set(right);
  return left.every((id) => other.has(id));
}

/** Search is a plain "does this text appear in it" over what the row shows. */
export function whitelistingMatches(
  conversations: readonly WhitelistingConversation[],
  search: string,
): readonly WhitelistingConversation[] {
  const needle = search.trim().toLowerCase();
  if (!needle) return conversations;
  return conversations.filter((conversation) =>
    `${conversation.name} ${conversation.account} ${conversation.kind}`.toLowerCase().includes(needle)
  );
}

/** Draft ids kept in list order, so two equal selections render identically. */
function orderedSelection(
  conversations: readonly WhitelistingConversation[],
  ids: Iterable<string>,
): readonly string[] {
  const wanted = new Set(ids);
  return conversations.map((conversation) => conversation.id).filter((id) => wanted.has(id));
}

export function whitelistingScreenView(state: WhitelistingScreenState): WhitelistingScreenView {
  const matches = whitelistingMatches(state.conversations, state.search);
  const draft = new Set(state.draft);
  const selectedMatchCount = matches.filter((conversation) => draft.has(conversation.id)).length;
  const clearedMatchCount = matches.length - selectedMatchCount;
  const selectedCount = state.conversations.filter((conversation) => draft.has(conversation.id)).length;
  const searching = state.search.trim().length > 0;
  const changed = state.conversations
    .map((conversation) => conversation.id)
    .filter((id) => draft.has(id) !== new Set(state.saved).has(id));
  const dirty = !sameIds(state.draft, state.saved);
  const totalCount = state.conversations.length;

  const resultLine = totalCount === 0
    ? "No conversations to search yet."
    : searching
      ? `${matches.length} of ${totalCount} conversations match "${state.search.trim()}" · ${selectedMatchCount} allowed, ${clearedMatchCount} not allowed`
      : `All ${totalCount} conversations · ${selectedCount} allowed, ${totalCount - selectedCount} not allowed`;

  return {
    search: state.search,
    searching,
    matches,
    matchCount: matches.length,
    totalCount,
    selectedCount,
    selectedMatchCount,
    clearedMatchCount,
    mixed: selectedMatchCount > 0 && clearedMatchCount > 0,
    dirty,
    changeCount: changed.length,
    resultLine,
    selectAllEnabled: !state.busy && clearedMatchCount > 0,
    clearAllEnabled: !state.busy && selectedMatchCount > 0,
    saveEnabled: !state.busy && dirty,
    resetEnabled: !state.busy && dirty,
  };
}

export function whitelistingSetSearch(state: WhitelistingScreenState, search: string): WhitelistingScreenState {
  return { ...state, search };
}

export function whitelistingToggleConversation(
  state: WhitelistingScreenState,
  id: string,
  allowed: boolean,
): WhitelistingScreenState {
  if (!state.conversations.some((conversation) => conversation.id === id)) return state;
  const next = new Set(state.draft);
  if (allowed) next.add(id);
  else next.delete(id);
  return { ...state, draft: orderedSelection(state.conversations, next) };
}

/** Ticks every row the search is showing, and leaves hidden rows alone. */
export function whitelistingSelectAll(state: WhitelistingScreenState): WhitelistingScreenState {
  const next = new Set(state.draft);
  for (const conversation of whitelistingMatches(state.conversations, state.search)) next.add(conversation.id);
  return { ...state, draft: orderedSelection(state.conversations, next) };
}

/** Unticks every row the search is showing, and leaves hidden rows alone. */
export function whitelistingClearAll(state: WhitelistingScreenState): WhitelistingScreenState {
  const next = new Set(state.draft);
  for (const conversation of whitelistingMatches(state.conversations, state.search)) next.delete(conversation.id);
  return { ...state, draft: orderedSelection(state.conversations, next) };
}

/** Throws away unsaved edits. It does not touch the search or the saved list. */
export function whitelistingReset(state: WhitelistingScreenState): WhitelistingScreenState {
  return { ...state, draft: orderedSelection(state.conversations, state.saved) };
}

/** Records that the draft is now what is on disk. */
export function whitelistingSaved(state: WhitelistingScreenState): WhitelistingScreenState {
  return { ...state, saved: orderedSelection(state.conversations, state.draft) };
}

/** What Save has to write: which rows turn on and which turn off. */
export function whitelistingPendingChanges(state: WhitelistingScreenState): {
  readonly allow: readonly string[];
  readonly remove: readonly string[];
} {
  const draft = new Set(state.draft);
  const saved = new Set(state.saved);
  const ids = state.conversations.map((conversation) => conversation.id);
  return {
    allow: ids.filter((id) => draft.has(id) && !saved.has(id)),
    remove: ids.filter((id) => !draft.has(id) && saved.has(id)),
  };
}

function rowMarkup(
  conversation: WhitelistingConversation,
  allowed: boolean,
  changed: boolean,
  busy: boolean,
): string {
  const id = escapeHtml(conversation.id);
  return `<li class="whitelisting-row${changed ? " changed" : ""}" data-whitelisting-row="${id}" data-whitelisting-allowed="${allowed}">`
    + `<label class="whitelisting-row-label">`
    + `<input class="whitelisting-row-tick" type="checkbox" data-whitelisting-conversation="${id}" ${allowed ? "checked " : ""}${busy ? "disabled " : ""}aria-label="Allow ${escapeHtml(conversation.name)} in ${escapeHtml(conversation.account)}"/>`
    + `<span class="whitelisting-row-text"><strong>${escapeHtml(conversation.name)}</strong>`
    + `<small>${escapeHtml(conversation.account)} · ${escapeHtml(conversation.kind)}</small></span>`
    + `<span class="whitelisting-row-state" data-whitelisting-row-state="${id}">${allowed ? "Allowed" : "Not allowed"}</span>`
    + `</label></li>`;
}

/**
 * The whole screen. It is deliberately one section: a user who has to scroll to
 * find out whether Save is even on the page cannot tell what state they are in.
 */
export function whitelistingScreenMarkup(state: WhitelistingScreenState): string {
  const view = whitelistingScreenView(state);
  const draft = new Set(state.draft);
  const saved = new Set(state.saved);
  const empty = state.conversations.length === 0;

  const list = empty
    ? `<div class="empty-state compact" data-whitelisting-empty><strong>No conversations yet</strong>`
      + `<p>Open a chat in a connected app and it appears here, ready to be allowed.</p></div>`
    : view.matchCount === 0
      ? `<div class="empty-state compact" data-whitelisting-no-match><strong>Nothing matches that search</strong>`
        + `<p>Clear the search box to see all ${view.totalCount} conversations again.</p></div>`
      : `<ul class="whitelisting-list" data-whitelisting-list>`
        + view.matches
          .map((conversation) =>
            rowMarkup(
              conversation,
              draft.has(conversation.id),
              draft.has(conversation.id) !== saved.has(conversation.id),
              state.busy,
            )
          )
          .join("")
        + `</ul>`;

  const unsaved = view.dirty
    ? `<p class="whitelisting-unsaved" data-whitelisting-unsaved role="status">${view.changeCount} ${view.changeCount === 1 ? "change is" : "changes are"} not saved yet.</p>`
    : `<p class="whitelisting-unsaved saved" data-whitelisting-unsaved role="status">Everything on this screen is saved.</p>`;

  return `<section class="settings-list whitelisting-screen" data-whitelisting-screen aria-labelledby="whitelisting-title">`
    + `<header><h2 id="whitelisting-title">${whitelistingTitle}</h2>`
    + `<p>Tick the conversations OSL may protect. OSL never touches a conversation that is not ticked here.</p></header>`
    + `<div class="whitelisting-search">`
    + `<label for="${whitelistingSearchInputId}">Search conversations</label>`
    + `<input id="${whitelistingSearchInputId}" data-whitelisting-search type="search" autocomplete="off" spellcheck="false" maxlength="64" placeholder="Search conversations" value="${escapeHtml(state.search)}" ${empty || state.busy ? "disabled" : ""}/>`
    + `<p class="whitelisting-result" data-whitelisting-result role="status">${escapeHtml(view.resultLine)}</p>`
    + `</div>`
    + `<div class="whitelisting-bulk" role="group" aria-label="Change every conversation the search is showing">`
    + `<button class="button compact" type="button" data-whitelisting-select-all ${view.selectAllEnabled ? "" : "disabled"}>Select all</button>`
    + `<button class="button compact" type="button" data-whitelisting-clear-all ${view.clearAllEnabled ? "" : "disabled"}>Clear all</button>`
    + `</div>`
    + list
    + `<div class="whitelisting-actions">`
    + `<button class="button primary" type="button" data-whitelisting-save ${view.saveEnabled ? "" : "disabled"}>Save</button>`
    + `<button class="button" type="button" data-whitelisting-reset ${view.resetEnabled ? "" : "disabled"}>Reset</button>`
    + unsaved
    + `<small class="whitelisting-actions-note">Save writes these ticks to this device. Reset undoes unsaved ticks and puts the saved list back.</small>`
    + `</div>`
    + `</section>`;
}
