import type { HubPerson, HubPersonWhitelistScope } from "./adapters";
import { inDomTooltipMarkup } from "./in-dom-tooltip";
import { escapeHtml } from "./services";

export interface WhitelistRosterState {
  open: boolean;
  activePersonId: string | null;
  activeScopeApproved: boolean;
  busy: boolean;
  people: readonly HubPerson[];
  scopeLimit: number;
}

export interface WhitelistRosterLabels {
  friendScopeLabel: (scope: HubPersonWhitelistScope) => string;
  narrowedScopeLabel: (storageKey: string) => string;
  whitelistReachLine: (person: HubPerson) => string;
}

export function whitelistRosterPersonMarkup(
  person: HubPerson,
  state: WhitelistRosterState,
  labels: WhitelistRosterLabels,
): string {
  const nickname = person.alias ?? "Unnamed friend";
  const isActive = state.activePersonId === person.personId;
  const visibleScopes = person.whitelistedScopes.slice(0, state.scopeLimit);
  const hiddenScopeCount = Math.max(0, person.whitelistCount - visibleScopes.length);
  const scopeRows = visibleScopes.map((scope) => {
    const label = labels.friendScopeLabel(scope);
    return `<div class="whitelist-roster-scope"><span class="friend-scope">${escapeHtml(label)}${scope.userSpecific ? ` <small>only this person</small>` : ""}</span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>+${inDomTooltipMarkup("Already approved")}</button><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(scope.storageKey)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" ${!isActive || state.busy ? "disabled" : ""}>−${inDomTooltipMarkup(isActive ? "Revoke this chat now" : "Open this person's protected chat to revoke")}</button></div></div>`;
  }).join("");
  const narrowedRows = person.reachNarrowedScopes.slice(0, state.scopeLimit).map((key) => {
    const label = labels.narrowedScopeLabel(key);
    return `<div class="whitelist-roster-scope narrowed"><span class="friend-scope narrowed">${escapeHtml(label)} <small>taken back</small></span><div class="discord-qa-whitelist" role="group" aria-label="Trust for ${escapeHtml(label)}"><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Approve ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>+${inDomTooltipMarkup("Approve this chat from inside it")}</button><button class="in-dom-tooltip-anchor" type="button" data-whitelist-scope-remove="${escapeHtml(person.personId)}" data-whitelist-scope-key="${escapeHtml(key)}" aria-label="Revoke ${escapeHtml(label)} for ${escapeHtml(nickname)}" disabled>−${inDomTooltipMarkup("Not approved")}</button></div></div>`;
  }).join("");
  const scopes = scopeRows || `<span class="friend-none">No chats approved</span>`;
  const truncated = hiddenScopeCount > 0 || person.whitelistedScopesTruncated
    ? `<small class="whitelist-roster-truncated">${hiddenScopeCount > 0 ? `${hiddenScopeCount} more approved ${hiddenScopeCount === 1 ? "chat is" : "chats are"}` : "More approved chats are"} stored locally and not listed here.</small>`
    : "";
  const reachDisabled = !isActive || state.busy || (!person.reachBroadened && person.whitelistCount === 0 && !state.activeScopeApproved);
  const reachButton = `<button class="button compact in-dom-tooltip-anchor" type="button" data-whitelist-reach="${escapeHtml(person.personId)}" data-whitelist-reach-next="${person.reachBroadened ? "off" : "on"}" aria-pressed="${person.reachBroadened}" ${reachDisabled ? "disabled" : ""}>${person.reachBroadened ? "Limit reach" : "Extend reach"}${inDomTooltipMarkup(person.reachBroadened ? "Withdraw reach across the chats you share" : "Extend this trust to the other chats you share")}</button>`;
  const reachNote = isActive ? "" : `<small class="whitelist-roster-note">Open this person's protected chat to change their reach or revoke a chat.</small>`;
  return `<article class="whitelist-roster-row person-row" data-whitelist-person="${escapeHtml(person.personId)}"><header><div><strong>${escapeHtml(nickname)}</strong><small>${escapeHtml(labels.whitelistReachLine(person))}</small></div>${reachButton}</header><div class="whitelist-roster-scopes">${scopes}${narrowedRows}</div>${truncated}${reachNote}</article>`;
}

export function whitelistRosterMarkup(state: WhitelistRosterState, labels: WhitelistRosterLabels): string {
  if (!state.open) return "";
  const roster = state.people.filter((person) => person.whitelistCount > 0 || person.reachNarrowedScopes.length > 0 || person.personId === state.activePersonId);
  const rows = roster.length
    ? roster.map((person) => whitelistRosterPersonMarkup(person, state, labels)).join("")
    : `<div class="empty-state"><strong>Nobody is whitelisted yet</strong><p>Approve a verified friend inside a chat; they appear here with the chats they cover.</p></div>`;
  return `<dialog class="friends-dialog whitelist-roster-dialog" id="whitelist-roster-dialog" aria-labelledby="whitelist-roster-title"><div class="friends-dialog-card"><header><h2 id="whitelist-roster-title">Whitelisted people</h2><button class="icon-button" id="whitelist-roster-close" type="button" aria-label="Close whitelist">×</button></header><p class="scope-approval-note">Approving a chat never widens anyone's reach. Extending reach is a separate, recorded choice, and a chat you take back stays revoked even while reach is on.</p><div class="whitelist-roster-list">${rows}</div></div></dialog>`;
}
