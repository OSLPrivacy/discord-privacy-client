import { describe, expect, it } from "vitest";
import {
  addSettingsFriend,
  defaultFriendsSettingsState,
  removeSettingsFriend,
  type FriendListId,
  type SettingsFriend,
} from "./friends-settings-surface";

const person: SettingsFriend = { id: "same-person", displayName: "Same Person", address: "@same_person" };

function ids(state: ReturnType<typeof defaultFriendsSettingsState>, list: FriendListId): string[] {
  return (list === "osl" ? state.oslFriends : state.oslChatsContacts).map((entry) => entry.id);
}

describe("TASK 5049b list separation", () => {
  it("adds and removes the same person on each list without leaking into the other", () => {
    let state = defaultFriendsSettingsState();

    state = addSettingsFriend(state, "osl", person);
    expect(ids(state, "osl")).toEqual(["same-person"]);
    expect(ids(state, "oslChats")).toEqual([]);
    state = removeSettingsFriend(state, "osl", person.id);
    expect(ids(state, "osl")).toEqual([]);
    expect(ids(state, "oslChats")).toEqual([]);

    state = addSettingsFriend(state, "oslChats", person);
    expect(ids(state, "osl")).toEqual([]);
    expect(ids(state, "oslChats")).toEqual(["same-person"]);
    state = removeSettingsFriend(state, "oslChats", person.id);
    expect(ids(state, "osl")).toEqual([]);
    expect(ids(state, "oslChats")).toEqual([]);

    console.log("TASK5049B same_person add_osl:other=0 remove_osl:other=0 add_chats:other=0 remove_chats:other=0");
  });
});
