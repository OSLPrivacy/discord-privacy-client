import { describe, expect, it } from "vitest";
import {
  ADD_ALLOWED_PLACE_COMMAND,
  COMPARE_ALLOWED_PLACE_COMMAND,
  REMOVE_ALLOWED_PLACE_COMMAND,
  loadSignalVerificationState,
  setSignalDirectMessageAllowed,
  signalVerificationTicked,
  signalWhitelistControlsMarkup,
  type SignalAllowedPlace,
  type SignalVerificationState,
} from "./signal-whitelist-controls";

const ALICE = "signal-alice-1048";
const BOB = "signal-bob-1048";

const place = (allowed: boolean, kind = "direct_message"): SignalAllowedPlace => ({
  app: "signal",
  account: ALICE,
  kind,
  stableId: `signal:${ALICE}:${kind}:${BOB}`,
  personName: "Bob & friends",
  placeName: "Bob's direct message",
  allowed,
});

const reciprocal: SignalVerificationState = {
  app: "signal",
  kind: "direct_message",
  firstAccount: ALICE,
  secondAccount: BOB,
  firstToSecondStableId: `signal:${ALICE}:direct_message:${BOB}`,
  secondToFirstStableId: `signal:${BOB}:direct_message:${ALICE}`,
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
  verificationTicked: true,
};

describe("TASK1048 Signal whitelist controls", () => {
  it("gives a two-way allowed direct message one tick and an unallowed one no controls", () => {
    const allowed = signalWhitelistControlsMarkup(place(true), reciprocal);
    const unallowed = signalWhitelistControlsMarkup(place(false), reciprocal);
    const group = signalWhitelistControlsMarkup(place(true, "group_chat"), reciprocal);

    expect(allowed).toContain("data-signal-whitelist-controls");
    expect(allowed).toContain(`data-signal-whitelist-toggle="signal:${ALICE}:direct_message:${BOB}"`);
    expect(allowed).toContain('data-signal-verification-tick="visible"');
    expect(allowed).toContain("✓ Both people have allowed this direct message");
    expect(allowed).toContain("Bob &amp; friends");
    expect((allowed.match(/data-signal-whitelist-controls/gu) ?? []).length).toBe(1);
    expect((allowed.match(/data-signal-verification-tick="visible"/gu) ?? []).length).toBe(1);
    expect(unallowed).toBe("");
    expect(group).toBe("");

    console.log(`TASK1048 allowed_two_way_direct_message_controls=${(allowed.match(/data-signal-whitelist-controls/gu) ?? []).length} verification_ticks=${(allowed.match(/data-signal-verification-tick="visible"/gu) ?? []).length} unallowed_place_controls=${(unallowed.match(/data-signal-whitelist-controls/gu) ?? []).length} group_chat_controls=${(group.match(/data-signal-whitelist-controls/gu) ?? []).length}`);
  });

  it("connects compare, add, and remove while refusing every unallowed operation", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const dependencies = { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      return command === COMPARE_ALLOWED_PLACE_COMMAND ? reciprocal : {};
    } };

    const state = await loadSignalVerificationState(place(true), BOB, dependencies);
    expect(state).toEqual(reciprocal);
    expect(signalVerificationTicked(state)).toBe(true);
    expect(await setSignalDirectMessageAllowed(place(true), true, dependencies)).toBe(true);
    expect(await setSignalDirectMessageAllowed(place(true), false, dependencies)).toBe(true);
    expect(calls.map(({ command }) => command)).toEqual([
      COMPARE_ALLOWED_PLACE_COMMAND,
      ADD_ALLOWED_PLACE_COMMAND,
      REMOVE_ALLOWED_PLACE_COMMAND,
    ]);
    expect(calls[0]?.args).toEqual({
      app: "signal", kind: "direct_message", firstAccount: ALICE, secondAccount: BOB,
    });
    expect(calls[1]?.args).toEqual({ record: {
      app: "signal", account: ALICE, kind: "direct_message",
      stable_id: `signal:${ALICE}:direct_message:${BOB}`,
      person_name: "Bob & friends", place_name: "Bob's direct message",
    } });
    expect(calls[2]?.args).toEqual({ stableId: `signal:${ALICE}:direct_message:${BOB}` });

    expect(await loadSignalVerificationState(place(false), BOB, dependencies)).toBeNull();
    expect(await setSignalDirectMessageAllowed(place(false), true, dependencies)).toBe(false);
    expect(calls).toHaveLength(3);

    console.log(`TASK1048 command_sequence=${calls.map(({ command }) => command).join(",")} reciprocal_tick=${signalVerificationTicked(state)} unallowed_compare=false unallowed_write=false`);
  });

  it("keeps one-way and inconsistent reciprocal responses unticked", async () => {
    const oneWay: SignalVerificationState = {
      ...reciprocal,
      secondToFirstAllowed: false,
      state: "one-way",
      verificationTicked: false,
    };
    const state = await loadSignalVerificationState(place(true), BOB, { invoke: async () => oneWay });
    const oneWayMarkup = signalWhitelistControlsMarkup(place(true), state);

    expect(signalVerificationTicked(state)).toBe(false);
    expect(oneWayMarkup).toContain('data-signal-verification-tick="hidden"');
    expect((oneWayMarkup.match(/data-signal-verification-tick="visible"/gu) ?? []).length).toBe(0);
    expect(await loadSignalVerificationState(place(true), BOB, {
      invoke: async () => ({ ...reciprocal, firstToSecondAllowed: false }),
    })).toBeNull();
    expect(await loadSignalVerificationState(place(true), BOB, {
      invoke: async () => ({ ...reciprocal, firstToSecondStableId: "signal:wrong:direct_message:ids" }),
    })).toBeNull();

    console.log("TASK1048 one_way_verification_ticks=0 inconsistent_two_way_response=refused mismatched_stable_ids=refused");
  });
});
