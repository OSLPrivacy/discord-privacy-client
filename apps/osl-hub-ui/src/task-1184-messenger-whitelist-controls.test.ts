import { describe, expect, it } from "vitest";
import {
  ADD_ALLOWED_PLACE_COMMAND,
  COMPARE_ALLOWED_PLACE_COMMAND,
  REMOVE_ALLOWED_PLACE_COMMAND,
  loadMessengerVerificationState,
  messengerVerificationTicked,
  messengerWhitelistControlsMarkup,
  setMessengerDirectMessageAllowed,
  type MessengerAllowedPlace,
  type MessengerVerificationState,
} from "./messenger-whitelist-controls";

const place = (allowed: boolean, kind = "direct_message"): MessengerAllowedPlace => ({
  app: "messenger",
  account: "messenger-alice-1184",
  kind,
  stableId: `messenger:messenger-alice-1184:${kind}:messenger-bob-1184`,
  personName: "Bob",
  placeName: "Bob's direct message",
  allowed,
});

const reciprocal: MessengerVerificationState = {
  app: "messenger",
  kind: "direct_message",
  firstAccount: "messenger-alice-1184",
  secondAccount: "messenger-bob-1184",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
  verificationTicked: true,
};

describe("TASK1184 Messenger whitelist controls", () => {
  it("gives an allowed two-way direct message one tick and an unallowed place no controls", () => {
    const allowed = messengerWhitelistControlsMarkup(place(true), reciprocal);
    const unallowed = messengerWhitelistControlsMarkup(place(false), reciprocal);
    const groupChat = messengerWhitelistControlsMarkup(place(true, "group_chat"), reciprocal);
    const community = messengerWhitelistControlsMarkup(place(true, "community"), reciprocal);

    expect(allowed).toContain("data-messenger-whitelist-controls");
    expect(allowed).toContain('data-messenger-whitelist-toggle="messenger:messenger-alice-1184:direct_message:messenger-bob-1184"');
    expect(allowed).toContain('data-messenger-verification-tick="visible"');
    expect((allowed.match(/data-messenger-verification-tick="visible"/gu) ?? []).length).toBe(1);
    expect(unallowed).toBe("");
    expect(groupChat).toBe("");
    expect(community).toBe("");

    console.log(`TASK1184 allowed_two_way_direct_message_controls=${(allowed.match(/data-messenger-whitelist-controls/gu) ?? []).length} verification_ticks=${(allowed.match(/data-messenger-verification-tick="visible"/gu) ?? []).length} unallowed_place_controls=${(unallowed.match(/data-messenger-whitelist-controls/gu) ?? []).length} group_chat_controls=${(groupChat.match(/data-messenger-whitelist-controls/gu) ?? []).length} community_controls=${(community.match(/data-messenger-whitelist-controls/gu) ?? []).length}`);
  });

  it("connects native compare, add, and remove while refusing unallowed places", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const dependencies = { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === COMPARE_ALLOWED_PLACE_COMMAND) return reciprocal;
      return {};
    } };

    const state = await loadMessengerVerificationState(place(true), "messenger-bob-1184", dependencies);
    expect(state).toEqual(reciprocal);
    expect(messengerVerificationTicked(state!)).toBe(true);
    expect(await setMessengerDirectMessageAllowed(place(true), true, dependencies)).toBe(true);
    expect(await setMessengerDirectMessageAllowed(place(true), false, dependencies)).toBe(true);
    expect(calls.map(({ command }) => command)).toEqual([
      COMPARE_ALLOWED_PLACE_COMMAND,
      ADD_ALLOWED_PLACE_COMMAND,
      REMOVE_ALLOWED_PLACE_COMMAND,
    ]);
    expect(await setMessengerDirectMessageAllowed(place(false), true, dependencies)).toBe(false);

    console.log(`TASK1184 command_sequence=${calls.map(({ command }) => command).join(",")} reciprocal_tick=${messengerVerificationTicked(state!)} unallowed_write=false`);
  });
});
