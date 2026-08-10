import { describe, expect, it } from "vitest";
import {
  addSettingsFriend,
  defaultFriendsSettingsState,
  friendsSettingsSurfaceMarkup,
  receiveSettingsFriendRequest,
  removeSettingsFriend,
  resolveSettingsFriendRequest,
  setOslAddRule,
  setOslChatsAddRule,
  visibleAccountsFromServices,
  type FriendVisibleAccount,
  type SettingsFriend,
  type SettingsFriendRequest,
} from "./friends-settings-surface";

const escapeHtml = (value: string): string => value;
const alice: SettingsFriend = { id: "alice", displayName: "Alice", address: "@alice" };
const bob: SettingsFriend = { id: "bob", displayName: "Bob", address: "@bob" };
const carol: SettingsFriend = { id: "carol", displayName: "Carol", address: "@carol" };
const accounts: FriendVisibleAccount[] = visibleAccountsFromServices([
  {
    id: "discord", displayName: "Discord", accounts: [
      { id: "main", label: "Main", displayHandle: "kestrel#0042", provider: null },
      { id: "alt", label: "Alt", displayHandle: "kestrel_alt", provider: null },
    ],
  },
  {
    id: "signal", displayName: "Signal", accounts: [
      { id: "phone", label: "Phone", displayHandle: "+1 ••• ••• 4417", provider: null },
    ],
  },
]);

function request(overrides: Partial<SettingsFriendRequest> = {}): SettingsFriendRequest {
  return {
    requestId: "request-1",
    targetList: "osl",
    route: "oslId",
    person: carol,
    note: "We met in the project room",
    ...overrides,
  };
}

describe("TASK 5049 friends and friending Settings surface", () => {
  it("renders separate OSL and OSL Chats lists plus the design's policy groups", () => {
    let state = addSettingsFriend(defaultFriendsSettingsState(), "osl", alice);
    state = addSettingsFriend(state, "oslChats", bob);
    const markup = friendsSettingsSurfaceMarkup(state, { escapeHtml, accounts });

    expect(markup).toContain('data-friends-list="osl"');
    expect(markup).toContain('data-friends-list="oslChats"');
    expect(markup).toContain("OSL friends and OSL Chats contacts are separate");
    expect(markup).toContain("Who can add you");
    expect(markup).toContain("Friend requests");
    expect(markup).toContain("Auto-mirror new friends");
    expect(state.autoMirrorNewFriends).toBe(false);
  });

  it("adds to either list without changing the other while mirroring is off", () => {
    let state = addSettingsFriend(defaultFriendsSettingsState(), "osl", alice);
    expect(state.oslFriends.map((person) => person.id)).toEqual(["alice"]);
    expect(state.oslChatsContacts).toEqual([]);

    state = addSettingsFriend(state, "oslChats", bob);
    expect(state.oslFriends.map((person) => person.id)).toEqual(["alice"]);
    expect(state.oslChatsContacts.map((person) => person.id)).toEqual(["bob"]);

    state = removeSettingsFriend(state, "osl", "alice");
    expect(state.oslFriends).toEqual([]);
    expect(state.oslChatsContacts.map((person) => person.id)).toEqual(["bob"]);
    console.log("TASK5049_INDEPENDENCE add_osl=1 chats_after_osl=0 add_chats=1 osl_after_chats=1 remove_osl=0 chats_after_remove=1 mirror=false");
  });

  it("mirrors only after the user explicitly turns the option on", () => {
    const state = { ...defaultFriendsSettingsState(), autoMirrorNewFriends: true };
    const added = addSettingsFriend(state, "osl", alice);
    expect(added.oslFriends.map((person) => person.id)).toEqual(["alice"]);
    expect(added.oslChatsContacts.map((person) => person.id)).toEqual(["alice"]);
  });

  it("applies the selected who-may-add-you rule before queuing a request", () => {
    const initial = defaultFriendsSettingsState();
    const refused = receiveSettingsFriendRequest(setOslAddRule(initial, "nobody"), request());
    expect(refused.queued).toBe(false);
    expect(refused.reason).toBe("closed");
    expect(refused.state.pendingRequests).toEqual([]);

    const open = setOslAddRule(initial, "anyoneWithId");
    const queued = receiveSettingsFriendRequest(open, request());
    expect(queued.queued).toBe(true);
    expect(queued.state.pendingRequests).toHaveLength(1);
    const accepted = resolveSettingsFriendRequest(queued.state, "request-1", true);
    expect(accepted.oslFriends.map((person) => person.id)).toEqual(["carol"]);
    expect(accepted.oslChatsContacts).toEqual([]);
    console.log("TASK5049_RULE nobody=refused anyoneWithId=queued pending=1 accepted_osl=1 chats_unchanged=0");
  });

  it("enforces the OSL Chats existing-friend rule too", () => {
    const initial = setOslChatsAddRule(defaultFriendsSettingsState(), "existingOslFriends");
    const transfer = request({ requestId: "chat-request", targetList: "oslChats", route: "transfer" });
    const refused = receiveSettingsFriendRequest(initial, transfer);
    expect(refused).toMatchObject({ queued: false, reason: "notOslFriend" });

    const known = addSettingsFriend(initial, "osl", carol);
    const queued = receiveSettingsFriendRequest(known, transfer);
    expect(queued).toMatchObject({ queued: true, reason: "allowed" });
  });

  it("renders one visibility row per connected account rather than per service", () => {
    const markup = friendsSettingsSurfaceMarkup(defaultFriendsSettingsState(), { escapeHtml, accounts });
    const rows = [...markup.matchAll(/data-visible-account="([^"]+)"/gu)].map((match) => match[1]);
    expect(rows).toEqual(["discord:main", "discord:alt", "signal:phone"]);
    expect(rows.filter((key) => key.startsWith("discord:"))).toHaveLength(2);
    console.log(`TASK5049_ACCOUNTS rows=${rows.length} services=2 discord_rows=2 keys=${rows.join(",")}`);
  });
});
