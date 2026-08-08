import { describe, expect, it } from "vitest";
import {
  ADD_ALLOWED_PLACE_COMMAND,
  COMPARE_ALLOWED_PLACE_COMMAND,
  REMOVE_ALLOWED_PLACE_COMMAND,
  instagramVerificationTicked,
  instagramWhitelistControlsMarkup,
  loadInstagramVerificationState,
  setInstagramDirectMessageAllowed,
  type InstagramAllowedPlace,
  type InstagramVerificationState,
} from "./instagram-whitelist-controls";

const place = (allowed: boolean, kind = "direct_message"): InstagramAllowedPlace => ({
  app: "instagram",
  account: "instagram-alice-1149",
  kind,
  stableId: `instagram:instagram-alice-1149:${kind}:instagram-bob-1149`,
  personName: "Bob",
  placeName: "Bob's direct message",
  allowed,
});

const reciprocal: InstagramVerificationState = {
  app: "instagram",
  kind: "direct_message",
  firstAccount: "instagram-alice-1149",
  secondAccount: "instagram-bob-1149",
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  savedDirections: 2,
  verificationState: "visible",
  state: "two-way",
};

describe("TASK1149 Instagram whitelist controls", () => {
  it("gives an allowed two-way direct message one tick and an unallowed place no controls", () => {
    const allowed = instagramWhitelistControlsMarkup(place(true), reciprocal);
    const unallowed = instagramWhitelistControlsMarkup(place(false), reciprocal);
    const publicPost = instagramWhitelistControlsMarkup(place(true, "public_post"), reciprocal);

    expect(allowed).toContain("data-instagram-whitelist-controls");
    expect(allowed).toContain('data-instagram-whitelist-toggle="instagram:instagram-alice-1149:direct_message:instagram-bob-1149"');
    expect(allowed).toContain('data-instagram-verification-tick="visible"');
    expect((allowed.match(/data-instagram-verification-tick="visible"/gu) ?? []).length).toBe(1);
    expect(unallowed).toBe("");
    expect(publicPost).toBe("");

    console.log(`TASK1149 allowed_two_way_direct_message_controls=${(allowed.match(/data-instagram-whitelist-controls/gu) ?? []).length} verification_ticks=${(allowed.match(/data-instagram-verification-tick="visible"/gu) ?? []).length} unallowed_place_controls=${(unallowed.match(/data-instagram-whitelist-controls/gu) ?? []).length} public_post_controls=${(publicPost.match(/data-instagram-whitelist-controls/gu) ?? []).length}`);
  });

  it("connects native compare, add, and remove while refusing unallowed places", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const dependencies = { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === COMPARE_ALLOWED_PLACE_COMMAND) return reciprocal;
      return {};
    } };

    const state = await loadInstagramVerificationState(place(true), "instagram-bob-1149", dependencies);
    expect(state).toEqual(reciprocal);
    expect(instagramVerificationTicked(state!)).toBe(true);
    expect(await setInstagramDirectMessageAllowed(place(true), true, dependencies)).toBe(true);
    expect(await setInstagramDirectMessageAllowed(place(true), false, dependencies)).toBe(true);
    expect(calls.map(({ command }) => command)).toEqual([
      COMPARE_ALLOWED_PLACE_COMMAND,
      ADD_ALLOWED_PLACE_COMMAND,
      REMOVE_ALLOWED_PLACE_COMMAND,
    ]);
    expect(await setInstagramDirectMessageAllowed(place(false), true, dependencies)).toBe(false);

    console.log(`TASK1149 command_sequence=${calls.map(({ command }) => command).join(",")} reciprocal_tick=${instagramVerificationTicked(state!)} unallowed_write=false`);
  });
});
