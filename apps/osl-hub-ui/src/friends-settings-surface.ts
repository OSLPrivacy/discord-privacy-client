/**
 * TASK 5049 — the Settings friends surface from the 2026-08-08 design export.
 *
 * The important boundary is structural, not just visual: OSL friends and OSL
 * Chats contacts are different collections. Every transition below receives a
 * list name and replaces only that collection. The sole cross-list transition
 * is the explicitly enabled `autoMirrorNewFriends` preference, which defaults
 * to false and only applies to additions (matching the design's wording).
 */

export type FriendListId = "osl" | "oslChats";
export type OslAddRule = "inviteOnly" | "anyoneWithId" | "nobody";
export type OslChatsAddRule = "existingOslFriends" | "anyoneWithUsername" | "nobody";
export type AddRoute = "invite" | "oslId" | "username" | "transfer";

export interface SettingsFriend {
  readonly id: string;
  readonly displayName: string;
  readonly address: string;
}

export interface SettingsFriendRequest {
  readonly requestId: string;
  readonly targetList: FriendListId;
  readonly route: AddRoute;
  readonly person: SettingsFriend;
  readonly note: string | null;
}

export interface FriendsSettingsState {
  readonly oslFriends: readonly SettingsFriend[];
  readonly oslChatsContacts: readonly SettingsFriend[];
  readonly pendingRequests: readonly SettingsFriendRequest[];
  readonly oslAddRule: OslAddRule;
  readonly oslChatsAddRule: OslChatsAddRule;
  readonly allowTransferRequests: boolean;
  readonly allowMessageRequests: boolean;
  readonly requireRequestNote: boolean;
  readonly autoMirrorNewFriends: boolean;
  readonly visibleAccountKeys: readonly string[];
}

export interface FriendVisibleAccount {
  readonly accountKey: string;
  readonly serviceName: string;
  readonly accountName: string;
  readonly handle: string;
}

interface AccountBearingService {
  readonly id: string;
  readonly displayName: string;
  readonly accounts: readonly {
    readonly id: string;
    readonly label: string;
    readonly displayHandle: string;
    readonly provider: string | null;
  }[];
}

export interface FriendsSettingsMarkupOptions {
  readonly escapeHtml: (value: string) => string;
  readonly accounts: readonly FriendVisibleAccount[];
}

export interface ReceiveFriendRequestResult {
  readonly state: FriendsSettingsState;
  readonly queued: boolean;
  readonly reason: "allowed" | "closed" | "wrongRoute" | "notOslFriend" | "requestsDisabled" | "noteRequired" | "duplicate";
}

export const OSL_ADD_RULES: readonly { value: OslAddRule; label: string; detail: string }[] = [
  { value: "inviteOnly", label: "Invite link only", detail: "Nobody finds you by name" },
  { value: "anyoneWithId", label: "Anyone with your OSL ID", detail: "Requests from your exact ID are allowed" },
  { value: "nobody", label: "Nobody", detail: "Your ID is claimed but closed" },
];

export const OSL_CHATS_ADD_RULES: readonly { value: OslChatsAddRule; label: string; detail: string }[] = [
  { value: "existingOslFriends", label: "Existing OSL friends", detail: "Only people already on your OSL list" },
  { value: "anyoneWithUsername", label: "Anyone with your username", detail: "Username requests are allowed" },
  { value: "nobody", label: "Nobody", detail: "OSL Chats requests are closed" },
];

export function defaultFriendsSettingsState(): FriendsSettingsState {
  return {
    oslFriends: [],
    oslChatsContacts: [],
    pendingRequests: [],
    oslAddRule: "inviteOnly",
    oslChatsAddRule: "existingOslFriends",
    allowTransferRequests: true,
    allowMessageRequests: true,
    requireRequestNote: false,
    autoMirrorNewFriends: false,
    visibleAccountKeys: [],
  };
}

function appendUnique(list: readonly SettingsFriend[], person: SettingsFriend): readonly SettingsFriend[] {
  return list.some((candidate) => candidate.id === person.id) ? list : [...list, person];
}

/** Add to exactly one collection unless the user explicitly enabled mirroring. */
export function addSettingsFriend(
  state: FriendsSettingsState,
  list: FriendListId,
  person: SettingsFriend,
): FriendsSettingsState {
  if (list === "osl") {
    return {
      ...state,
      oslFriends: appendUnique(state.oslFriends, person),
      oslChatsContacts: state.autoMirrorNewFriends
        ? appendUnique(state.oslChatsContacts, person)
        : state.oslChatsContacts,
    };
  }
  return {
    ...state,
    oslChatsContacts: appendUnique(state.oslChatsContacts, person),
    oslFriends: state.autoMirrorNewFriends
      ? appendUnique(state.oslFriends, person)
      : state.oslFriends,
  };
}

/** Removal is always list-local; the design only offers mirroring for new friends. */
export function removeSettingsFriend(
  state: FriendsSettingsState,
  list: FriendListId,
  personId: string,
): FriendsSettingsState {
  return list === "osl"
    ? { ...state, oslFriends: state.oslFriends.filter((person) => person.id !== personId) }
    : { ...state, oslChatsContacts: state.oslChatsContacts.filter((person) => person.id !== personId) };
}

export function setOslAddRule(state: FriendsSettingsState, rule: OslAddRule): FriendsSettingsState {
  return { ...state, oslAddRule: rule };
}

export function setOslChatsAddRule(state: FriendsSettingsState, rule: OslChatsAddRule): FriendsSettingsState {
  return { ...state, oslChatsAddRule: rule };
}

export function requestMayBeQueued(state: FriendsSettingsState, request: SettingsFriendRequest): ReceiveFriendRequestResult["reason"] {
  if (state.pendingRequests.some((pending) => pending.requestId === request.requestId)) return "duplicate";
  if (state.requireRequestNote && !request.note?.trim()) return "noteRequired";
  if (request.targetList === "osl") {
    if (state.oslAddRule === "nobody") return "closed";
    if (state.oslAddRule === "inviteOnly" && request.route !== "invite") return "wrongRoute";
    if (state.oslAddRule === "anyoneWithId" && request.route !== "invite" && request.route !== "oslId") return "wrongRoute";
    return "allowed";
  }
  if (!state.allowMessageRequests && request.route !== "transfer") return "requestsDisabled";
  if (!state.allowTransferRequests && request.route === "transfer") return "requestsDisabled";
  if (state.oslChatsAddRule === "nobody") return "closed";
  if (state.oslChatsAddRule === "existingOslFriends"
    && !state.oslFriends.some((person) => person.id === request.person.id)) return "notOslFriend";
  if (state.oslChatsAddRule === "anyoneWithUsername"
    && request.route !== "username" && request.route !== "transfer") return "wrongRoute";
  return "allowed";
}

/** Apply the selected add rule before a request is allowed onto the pending list. */
export function receiveSettingsFriendRequest(
  state: FriendsSettingsState,
  request: SettingsFriendRequest,
): ReceiveFriendRequestResult {
  const reason = requestMayBeQueued(state, request);
  return reason === "allowed"
    ? { state: { ...state, pendingRequests: [...state.pendingRequests, request] }, queued: true, reason }
    : { state, queued: false, reason };
}

export function resolveSettingsFriendRequest(
  state: FriendsSettingsState,
  requestId: string,
  accept: boolean,
): FriendsSettingsState {
  const request = state.pendingRequests.find((candidate) => candidate.requestId === requestId);
  if (!request) return state;
  const withoutRequest = {
    ...state,
    pendingRequests: state.pendingRequests.filter((candidate) => candidate.requestId !== requestId),
  };
  return accept ? addSettingsFriend(withoutRequest, request.targetList, request.person) : withoutRequest;
}

export function setAccountVisible(
  state: FriendsSettingsState,
  accountKey: string,
  visible: boolean,
): FriendsSettingsState {
  const keys = state.visibleAccountKeys.filter((key) => key !== accountKey);
  return { ...state, visibleAccountKeys: visible ? [...keys, accountKey] : keys };
}

/** Flatten services without grouping: two Discord accounts must remain two rows. */
export function visibleAccountsFromServices(services: readonly AccountBearingService[]): FriendVisibleAccount[] {
  return services.flatMap((service) => service.accounts.map((account) => ({
    accountKey: `${service.id}:${account.id}`,
    serviceName: account.provider
      ? account.provider[0].toUpperCase() + account.provider.slice(1)
      : service.displayName,
    accountName: account.label,
    handle: account.displayHandle,
  })));
}

function friendListMarkup(
  list: FriendListId,
  people: readonly SettingsFriend[],
  other: readonly SettingsFriend[],
  escapeHtml: (value: string) => string,
): string {
  const rows = people.length
    ? people.map((person) => {
      const canCopyToChats = list === "osl" && !other.some((candidate) => candidate.id === person.id);
      const addToChats = canCopyToChats
        ? `<button class="button compact" type="button" data-add-to-osl-chats="${escapeHtml(person.id)}">Add to OSL Chats</button>`
        : "";
      return `<article class="friends-settings-person" data-settings-friend="${escapeHtml(person.id)}"><span class="friends-settings-avatar" aria-hidden="true">${escapeHtml((person.displayName[0] || "?").toUpperCase())}</span><span><strong>${escapeHtml(person.displayName)}</strong><small>${escapeHtml(person.address)}</small></span><span class="friends-settings-row-actions">${addToChats}<button class="button compact danger" type="button" data-remove-settings-friend="${escapeHtml(person.id)}" data-friend-list="${list}">Remove</button></span></article>`;
    }).join("")
    : `<div class="friends-settings-empty"><strong>${list === "osl" ? "No OSL friends yet" : "No OSL Chats contacts yet"}</strong><small>${list === "osl" ? "Add someone with their invite or OSL ID." : "Add an OSL friend here only when you also want to message them."}</small></div>`;
  return `<section class="friends-settings-list" data-friends-list="${list}" aria-labelledby="friends-${list}-title"><header><div><h3 id="friends-${list}-title">${list === "osl" ? "OSL friends" : "OSL Chats contacts"}</h3><p>${list === "osl" ? "People you trust on this device." : "People you message in OSL Chats."}</p></div>${list === "osl" ? '<button class="button compact" type="button" data-open-friends>Add OSL friend</button>' : ""}</header>${rows}</section>`;
}

function ruleMarkup<T extends string>(
  name: string,
  legend: string,
  value: T,
  options: readonly { value: T; label: string; detail: string }[],
  dataName: string,
  escapeHtml: (value: string) => string,
): string {
  const rows = options.map((option) => `<label class="friends-settings-choice"><span><strong>${escapeHtml(option.label)}</strong><small>${escapeHtml(option.detail)}</small></span><input type="radio" name="${name}" value="${option.value}" data-${dataName} ${value === option.value ? "checked" : ""}/></label>`).join("");
  return `<fieldset class="friends-settings-policy"><legend>${escapeHtml(legend)}</legend>${rows}</fieldset>`;
}

export function friendsSettingsSurfaceMarkup(
  state: FriendsSettingsState,
  options: FriendsSettingsMarkupOptions,
): string {
  const { escapeHtml, accounts } = options;
  const requests = state.pendingRequests.length
    ? state.pendingRequests.map((request) => `<article class="friends-settings-request" data-friend-request="${escapeHtml(request.requestId)}"><span><strong>${escapeHtml(request.person.displayName)}</strong><small>${request.targetList === "osl" ? "OSL friend request" : "OSL Chats message request"}${request.note ? ` · ${escapeHtml(request.note)}` : ""}</small></span><span><button class="button compact primary" type="button" data-resolve-friend-request="accept" data-request-id="${escapeHtml(request.requestId)}">Accept</button><button class="button compact" type="button" data-resolve-friend-request="decline" data-request-id="${escapeHtml(request.requestId)}">Decline</button></span></article>`).join("")
    : `<div class="friends-settings-empty"><strong>No friend requests</strong><small>Allowed requests will appear here before anyone is added.</small></div>`;
  const accountRows = accounts.length
    ? accounts.map((account) => `<label class="friends-settings-account" data-visible-account="${escapeHtml(account.accountKey)}"><span class="friends-settings-account-mark" aria-hidden="true">${escapeHtml(account.serviceName.slice(0, 2).toUpperCase())}</span><span><strong>${escapeHtml(account.accountName)}</strong><small>${escapeHtml(account.serviceName)} · ${escapeHtml(account.handle)}</small></span><input type="checkbox" data-friend-account-visible="${escapeHtml(account.accountKey)}" ${state.visibleAccountKeys.includes(account.accountKey) ? "checked" : ""}/></label>`).join("")
    : `<div class="friends-settings-empty"><strong>No connected accounts</strong><small>Connect an account before sharing it on your friend card.</small></div>`;

  return `<section class="friends-settings-surface" aria-labelledby="friends-settings-title"><header class="friends-settings-heading"><h2 id="friends-settings-title">Friends</h2><p>OSL friends and OSL Chats contacts are separate. Adding or removing someone on one list never changes the other unless you turn on mirroring.</p><span class="status-tag">Separate lists</span></header><div class="friends-settings-lists">${friendListMarkup("osl", state.oslFriends, state.oslChatsContacts, escapeHtml)}${friendListMarkup("oslChats", state.oslChatsContacts, state.oslFriends, escapeHtml)}</div><section class="friends-settings-group" aria-labelledby="who-can-add-title"><header><h3 id="who-can-add-title">Who can add you</h3><p>Requests that do not match these rules are refused before they reach your list.</p></header><div class="friends-settings-policies">${ruleMarkup("osl-add-rule", "OSL", state.oslAddRule, OSL_ADD_RULES, "osl-add-rule", escapeHtml)}${ruleMarkup("osl-chats-add-rule", "OSL Chats", state.oslChatsAddRule, OSL_CHATS_ADD_RULES, "osl-chats-add-rule", escapeHtml)}</div></section><section class="friends-settings-group" aria-labelledby="friend-requests-title"><header><h3 id="friend-requests-title">Friend requests</h3></header><div class="friends-settings-toggles"><label class="setting-line interactive"><span><strong>Allow transfer requests</strong><small>An OSL friend can ask to also become an OSL Chats contact.</small></span><input type="checkbox" data-friend-request-setting="allowTransferRequests" ${state.allowTransferRequests ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Allow message requests</strong><small>Someone allowed by your OSL Chats rule can ask to message you.</small></span><input type="checkbox" data-friend-request-setting="allowMessageRequests" ${state.allowMessageRequests ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Require a note with requests</strong><small>One line saying who they are.</small></span><input type="checkbox" data-friend-request-setting="requireRequestNote" ${state.requireRequestNote ? "checked" : ""}/></label><label class="setting-line interactive"><span><strong>Auto-mirror new friends</strong><small>Off by default. When on, a new addition is copied to the other list.</small></span><input type="checkbox" data-friend-request-setting="autoMirrorNewFriends" ${state.autoMirrorNewFriends ? "checked" : ""}/></label></div><div class="friends-settings-requests">${requests}</div></section><section class="friends-settings-group" aria-labelledby="friend-accounts-title"><header><h3 id="friend-accounts-title">Accounts friends can see</h3><p>Each connected account has its own row, including multiple accounts from the same service.</p></header><div class="friends-settings-accounts">${accountRows}</div></section></section>`;
}
