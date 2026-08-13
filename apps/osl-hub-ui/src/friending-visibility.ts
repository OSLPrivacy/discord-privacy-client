/**
 * The social visibility switches shared by Settings → Privacy and onboarding.
 *
 * This is deliberately a Settings preference, not onboarding progress: both
 * surfaces read and write this one record and the admission checks below use
 * the same four bits.
 */
import { continueButton } from "./onboarding-controls";

export const FRIENDING_VISIBILITY_STORAGE_KEY = "osl.friending-visibility-v1";

export const FRIENDING_VISIBILITY_SWITCHES = [
  "findableByPublicUsername",
  "friendRequestsAllowed",
  "messageRequestsAllowed",
  "profileViewable",
] as const;

export type FriendingVisibilitySwitch = typeof FRIENDING_VISIBILITY_SWITCHES[number];
export type FriendingVisibilityPreset = "SILENT" | "VISIBLE";
export type FriendingVisibilityAction = "publicUsernameSearch" | "friendRequest" | "messageRequest" | "profileView";

export interface FriendingVisibilityState {
  readonly findableByPublicUsername: boolean;
  readonly friendRequestsAllowed: boolean;
  readonly messageRequestsAllowed: boolean;
  readonly profileViewable: boolean;
}

export interface FriendingVisibilityStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export const defaultFriendingVisibilityState = (): FriendingVisibilityState => ({
  findableByPublicUsername: false,
  friendRequestsAllowed: false,
  messageRequestsAllowed: false,
  profileViewable: false,
});

function isState(value: unknown): value is FriendingVisibilityState {
  return typeof value === "object" && value !== null
    && FRIENDING_VISIBILITY_SWITCHES.every((key) => typeof (value as Record<string, unknown>)[key] === "boolean")
    && Object.keys(value).length === FRIENDING_VISIBILITY_SWITCHES.length;
}

export function readFriendingVisibility(raw: string | null): FriendingVisibilityState {
  if (!raw) return defaultFriendingVisibilityState();
  try {
    const parsed: unknown = JSON.parse(raw);
    return isState(parsed) ? parsed : defaultFriendingVisibilityState();
  } catch {
    return defaultFriendingVisibilityState();
  }
}

export function loadFriendingVisibility(storage: Pick<FriendingVisibilityStorage, "getItem">): FriendingVisibilityState {
  return readFriendingVisibility(storage.getItem(FRIENDING_VISIBILITY_STORAGE_KEY));
}

export function saveFriendingVisibility(storage: Pick<FriendingVisibilityStorage, "setItem">, state: FriendingVisibilityState): void {
  storage.setItem(FRIENDING_VISIBILITY_STORAGE_KEY, JSON.stringify(state));
}

export function setFriendingVisibilitySwitch(
  state: FriendingVisibilityState,
  key: FriendingVisibilitySwitch,
  checked: boolean,
): FriendingVisibilityState {
  return { ...state, [key]: checked };
}

export function applyFriendingVisibilityPreset(
  state: FriendingVisibilityState,
  preset: FriendingVisibilityPreset,
): FriendingVisibilityState {
  const enabled = preset === "VISIBLE";
  return FRIENDING_VISIBILITY_SWITCHES.reduce(
    (next, key) => setFriendingVisibilitySwitch(next, key, enabled),
    state,
  );
}

export function activeFriendingVisibilityPreset(state: FriendingVisibilityState): FriendingVisibilityPreset | null {
  const enabled = FRIENDING_VISIBILITY_SWITCHES.filter((key) => state[key]).length;
  if (enabled === 0) return "SILENT";
  if (enabled === FRIENDING_VISIBILITY_SWITCHES.length) return "VISIBLE";
  return null;
}

/** The second identity's request reaches exactly the corresponding switch. */
export function friendingVisibilityAllows(state: FriendingVisibilityState, action: FriendingVisibilityAction): boolean {
  if (action === "publicUsernameSearch") return state.findableByPublicUsername;
  if (action === "friendRequest") return state.friendRequestsAllowed;
  if (action === "messageRequest") return state.messageRequestsAllowed;
  return state.profileViewable;
}

/** A second identity sees one permitted result/action, or no result at all. */
export function secondIdentityFriendingResultCount(state: FriendingVisibilityState, action: FriendingVisibilityAction): 0 | 1 {
  return friendingVisibilityAllows(state, action) ? 1 : 0;
}

const ROWS: readonly { key: FriendingVisibilitySwitch; label: string; detail: string }[] = [
  { key: "findableByPublicUsername", label: "findable by public username", detail: "Lets someone searching your exact public username receive this one result." },
  { key: "friendRequestsAllowed", label: "friend requests allowed", detail: "Lets someone who finds you send a friend request for you to approve." },
  { key: "messageRequestsAllowed", label: "message requests allowed", detail: "Lets someone who finds you send an OSL Chat request; it does not open a chat automatically." },
  { key: "profileViewable", label: "profile viewable", detail: "Lets someone who reaches your profile see the profile details you chose to publish." },
];

function presetChip(preset: FriendingVisibilityPreset, state: FriendingVisibilityState): string {
  const active = activeFriendingVisibilityPreset(state) === preset;
  return `<button class="button compact ${active ? "primary" : ""}" type="button" data-friending-visibility-preset="${preset}" aria-pressed="${active}">${preset}</button>`;
}

export function friendingVisibilityMarkup(
  state: FriendingVisibilityState,
  surface: "onboarding" | "settings",
): string {
  const titleTag = surface === "onboarding" ? "h1" : "h2";
  const titleId = surface === "onboarding" ? "route-heading" : "friending-visibility-title";
  const heading = surface === "onboarding"
    ? `<${titleTag} id="${titleId}" tabindex="-1">Can people tell you use OSL</${titleTag}>`
    : `<${titleTag} id="${titleId}">Privacy</${titleTag}><p>Can people tell you use OSL</p>`;
  const rows = ROWS.map((row) => `<label class="setting-line interactive" data-friending-visibility-row="${row.key}"><span><strong>${row.label}</strong><small>${row.detail}</small></span><input id="friending-visibility-${row.key}" type="checkbox" data-friending-visibility-switch="${row.key}" ${state[row.key] ? "checked" : ""}/></label>`).join("");
  const continueAction = surface === "onboarding"
    ? `<div class="setup-footer onboarding-actions">${continueButton('id="continue-friending-visibility"', "")}</div>`
    : "";
  return `<section class="friending-visibility-surface" data-friending-visibility-surface="${surface}" aria-labelledby="${titleId}">${heading}<div class="friending-visibility-presets" role="group" aria-label="Visibility presets">${presetChip("SILENT", state)}${presetChip("VISIBLE", state)}</div><div class="settings-list" data-friending-visibility-rows>${rows}</div><p class="friending-visibility-footnote">Strangers see nothing about how you use OSL either way. You can change these later in Settings, Privacy.</p>${continueAction}</section>`;
}
