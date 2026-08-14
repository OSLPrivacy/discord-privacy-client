import { describe, expect, it } from "vitest";
import {
  ADD_ALLOWED_PLACE_COMMAND,
  COMPARE_ALLOWED_PLACE_COMMAND,
  REMOVE_ALLOWED_PLACE_COMMAND,
  loadXVerificationState,
  setXDirectMessageAllowed,
  xVerificationTicked,
  xWhitelistControlsMarkup,
  type XAllowedPlace,
  type XVerificationState,
} from "./x-whitelist-controls";

const place = (allowed: boolean, kind = "direct_message"): XAllowedPlace => ({
  app: "x", account: "x-alice-1120", kind, stableId: `x:x-alice-1120:${kind}:x-bob-1120`,
  personName: "Bob", placeName: "Bob's direct message", allowed,
});
const reciprocal: XVerificationState = {
  app: "x", kind: "direct_message", firstAccount: "x-alice-1120", secondAccount: "x-bob-1120",
  firstToSecondAllowed: true, secondToFirstAllowed: true, state: "two-way",
};

describe("TASK1120 X whitelist controls", () => {
  it("shows one control and a visible verification tick only for an allowed direct message", () => {
    const allowed = xWhitelistControlsMarkup(place(true), reciprocal);
    const unallowed = xWhitelistControlsMarkup(place(false), reciprocal);
    const publicPost = xWhitelistControlsMarkup(place(true, "public_post"), reciprocal);

    expect(allowed).toContain('data-x-whitelist-controls');
    expect(allowed).toContain('data-x-whitelist-toggle="x:x-alice-1120:direct_message:x-bob-1120"');
    expect(allowed).toContain('data-x-verification-tick="visible"');
    expect(unallowed).toBe("");
    expect(publicPost).toBe("");
    console.log(`TASK1120 allowed_direct_message_controls=${(allowed.match(/data-x-whitelist-controls/g) ?? []).length} verification_tick=visible unallowed_place_controls=${(unallowed.match(/data-x-whitelist-controls/g) ?? []).length} public_post_controls=${(publicPost.match(/data-x-whitelist-controls/g) ?? []).length}`);
  });

  it("connects the direct-message toggle and only ticks a reciprocal native result", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const dependencies = { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === COMPARE_ALLOWED_PLACE_COMMAND) return reciprocal;
      return {};
    } };
    const state = await loadXVerificationState(place(true), "x-bob-1120", dependencies);
    expect(state).toEqual(reciprocal);
    expect(xVerificationTicked(state!)).toBe(true);
    expect(await setXDirectMessageAllowed(place(true), true, dependencies)).toBe(true);
    expect(await setXDirectMessageAllowed(place(true), false, dependencies)).toBe(true);
    expect(calls.map(({ command }) => command)).toEqual([
      COMPARE_ALLOWED_PLACE_COMMAND, ADD_ALLOWED_PLACE_COMMAND, REMOVE_ALLOWED_PLACE_COMMAND,
    ]);
    expect(await setXDirectMessageAllowed(place(false), true, dependencies)).toBe(false);
    console.log(`TASK1120 command_sequence=${calls.map(({ command }) => command).join(",")} reciprocal_tick=${xVerificationTicked(state!)} unallowed_write=false`);
  });
});
