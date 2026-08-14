import { describe, expect, it } from "vitest";
import {
  ADD_ALLOWED_PLACE_COMMAND,
  COMPARE_ALLOWED_PLACE_COMMAND,
  REMOVE_ALLOWED_PLACE_COMMAND,
  loadTelegramVerificationState,
  setTelegramDirectMessageAllowed,
  telegramVerificationTicked,
  telegramWhitelistControlsMarkup,
  type TelegramAllowedPlace,
  type TelegramVerificationState,
} from "./telegram-whitelist-controls";

const ALICE = "telegram-alice-1021";
const BOB = "telegram-bob-1021";

const place = (allowed: boolean, kind = "direct_message"): TelegramAllowedPlace => ({
  app: "telegram",
  account: ALICE,
  kind,
  stableId: `telegram:${ALICE}:${kind}:${BOB}`,
  personName: "Bob & friends",
  placeName: "Bob's direct message",
  allowed,
});

const reciprocal: TelegramVerificationState = {
  app: "telegram",
  kind: "direct_message",
  firstAccount: ALICE,
  secondAccount: BOB,
  firstToSecondStableId: `telegram:${ALICE}:direct_message:${BOB}`,
  secondToFirstStableId: `telegram:${BOB}:direct_message:${ALICE}`,
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
};

describe("TASK1021 Telegram whitelist controls", () => {
  it("shows one verification tick for a two-way allowed DM and no OSL controls for an unallowed place", () => {
    const allowed = telegramWhitelistControlsMarkup(place(true), reciprocal);
    const unallowed = telegramWhitelistControlsMarkup(place(false), reciprocal);
    const group = telegramWhitelistControlsMarkup(place(true, "group_chat"), reciprocal);

    expect(allowed).toContain("data-telegram-whitelist-controls");
    expect(allowed).toContain(`data-telegram-whitelist-toggle="telegram:${ALICE}:direct_message:${BOB}"`);
    expect(allowed).toContain('data-telegram-verification-tick="visible"');
    expect(allowed).toContain("✓ Both people have allowed this direct message");
    expect(allowed).toContain("Bob &amp; friends");
    expect((allowed.match(/data-telegram-whitelist-controls/gu) ?? []).length).toBe(1);
    expect((allowed.match(/data-telegram-verification-tick="visible"/gu) ?? []).length).toBe(1);
    expect(unallowed).toBe("");
    expect(group).toContain("data-telegram-whitelist-controls");
    expect(group).toContain('data-telegram-place-kind="group_chat"');

    console.log(`TASK1021 allowed_two_way_direct_message_controls=${(allowed.match(/data-telegram-whitelist-controls/gu) ?? []).length} verification_ticks=${(allowed.match(/data-telegram-verification-tick="visible"/gu) ?? []).length} unallowed_place_controls=${(unallowed.match(/data-telegram-whitelist-controls/gu) ?? []).length} group_chat_controls=${(group.match(/data-telegram-whitelist-controls/gu) ?? []).length}`);
  });

  it("connects compare, add, and remove and refuses every unallowed operation", async () => {
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const dependencies = { invoke: async (command: string, args: Record<string, unknown>) => {
      calls.push({ command, args });
      return command === COMPARE_ALLOWED_PLACE_COMMAND ? reciprocal : {};
    } };

    const state = await loadTelegramVerificationState(place(true), BOB, dependencies);
    expect(state).toEqual(reciprocal);
    expect(telegramVerificationTicked(state)).toBe(true);
    expect(await setTelegramDirectMessageAllowed(place(true), true, dependencies)).toBe(true);
    expect(await setTelegramDirectMessageAllowed(place(true), false, dependencies)).toBe(true);
    expect(calls.map(({ command }) => command)).toEqual([
      COMPARE_ALLOWED_PLACE_COMMAND,
      ADD_ALLOWED_PLACE_COMMAND,
      REMOVE_ALLOWED_PLACE_COMMAND,
    ]);
    expect(calls[0]?.args).toEqual({
      app: "telegram", kind: "direct_message", firstAccount: ALICE, secondAccount: BOB,
    });
    expect(calls[1]?.args).toEqual({ record: {
      app: "telegram", account: ALICE, kind: "direct_message",
      stable_id: `telegram:${ALICE}:direct_message:${BOB}`,
      person_name: "Bob & friends", place_name: "Bob's direct message",
    } });
    expect(calls[2]?.args).toEqual({ stableId: `telegram:${ALICE}:direct_message:${BOB}` });

    expect(await loadTelegramVerificationState(place(false), BOB, dependencies)).toBeNull();
    expect(await setTelegramDirectMessageAllowed(place(false), true, dependencies)).toBe(false);
    expect(calls).toHaveLength(3);

    console.log(`TASK1021 command_sequence=${calls.map(({ command }) => command).join(",")} reciprocal_tick=${telegramVerificationTicked(state)} unallowed_compare=false unallowed_write=false`);
  });

  it("keeps one-way and inconsistent reciprocal responses unticked", async () => {
    const oneWay: TelegramVerificationState = {
      ...reciprocal,
      secondToFirstAllowed: false,
      state: "one-way",
    };
    const dependencies = { invoke: async () => oneWay };
    const state = await loadTelegramVerificationState(place(true), BOB, dependencies);
    const oneWayMarkup = telegramWhitelistControlsMarkup(place(true), state);

    expect(telegramVerificationTicked(state)).toBe(false);
    expect(oneWayMarkup).toContain('data-telegram-verification-tick="hidden"');
    expect(await loadTelegramVerificationState(place(true), BOB, {
      invoke: async () => ({ ...reciprocal, firstToSecondAllowed: false }),
    })).toBeNull();

    console.log("TASK1021 one_way_verification_ticks=0 inconsistent_two_way_response=refused");
  });
});
