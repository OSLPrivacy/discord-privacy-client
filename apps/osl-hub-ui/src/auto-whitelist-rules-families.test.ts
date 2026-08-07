/**
 * TASK 0742 - the nine named controls on the auto-whitelist rules screen.
 *
 * The screenshot check next door proves they are drawn and legible. This one
 * proves the seven families are a real cover of the place-kind catalogue
 * rather than a second list beside it, and that picking a rule on a family
 * moves every place in it and nothing else.
 */
import { describe, expect, it } from "vitest";

import {
  AUTO_WHITELIST_CHOICES,
  AUTO_WHITELIST_FAMILIES,
  AUTO_WHITELIST_PLACE_KINDS,
  autoWhitelistRulesState,
  changedRuleKeys,
  familyChoice,
  familyForKind,
  familyPlaceKinds,
  renderAutoWhitelistRulesScreen,
  setFamilyRule,
  setPlaceRule,
} from "./auto-whitelist-rules-screen";
import { AUTO_WHITELIST_RULES_SCREEN_SAVED } from "./auto-whitelist-rules-screen-data";

/** The nine names TASK 0742 is measured against, in screen order. */
const NAMED_CONTROLS = [
  "direct messages",
  "groups",
  "servers",
  "channels",
  "threads",
  "email",
  "posts",
  "Save rules",
  "Reset",
];

describe("TASK 0742 named controls", () => {
  const state = autoWhitelistRulesState(AUTO_WHITELIST_RULES_SCREEN_SAVED);
  const html = renderAutoWhitelistRulesScreen(state);

  it("names the seven families and the two buttons, in that order", () => {
    const families = [...html.matchAll(/<label class="rule-family-name"[^>]*>([^<]+)</gu)].map(
      (match) => match[1],
    );
    const buttons = [...html.matchAll(/data-rule-action="(?:save|reset)">([^<]+)</gu)].map(
      (match) => match[1],
    );

    expect([...families, ...buttons]).toEqual(NAMED_CONTROLS);
    expect(AUTO_WHITELIST_FAMILIES.map((family) => family.label)).toEqual(
      NAMED_CONTROLS.slice(0, 7),
    );
  });

  it("puts every place kind in exactly one family", () => {
    const covered = AUTO_WHITELIST_FAMILIES.flatMap((family) => family.kinds);
    const catalogue = [...new Set(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.kind))];

    expect(covered.length).toBe(new Set(covered).size);
    expect([...covered].sort()).toEqual([...catalogue].sort());
    for (const place of AUTO_WHITELIST_PLACE_KINDS) {
      expect(familyForKind(place.kind).kinds).toContain(place.kind);
    }
    expect(
      AUTO_WHITELIST_FAMILIES.flatMap((family) => familyPlaceKinds(family.id)).map(
        (place) => place.ruleKey,
      ).sort(),
    ).toEqual(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.ruleKey).sort());
  });

  it("counts the places each family covers", () => {
    expect(
      Object.fromEntries(
        AUTO_WHITELIST_FAMILIES.map((family) => [family.id, familyPlaceKinds(family.id).length]),
      ),
    ).toEqual({
      direct_messages: 9,
      groups: 14,
      servers: 1,
      channels: 4,
      threads: 3,
      email: 2,
      posts: 5,
    });
    expect(
      AUTO_WHITELIST_FAMILIES.reduce((total, family) => total + familyPlaceKinds(family.id).length, 0),
    ).toBe(38);
  });

  it("reads the rule its rows share, and says mixed when they disagree", () => {
    const fresh = autoWhitelistRulesState();
    for (const family of AUTO_WHITELIST_FAMILIES) {
      expect(familyChoice(fresh, family.id)).toBe("never");
    }

    const split = setPlaceRule(fresh, "discord:channel", "always");
    expect(familyChoice(split, "channels")).toBe("mixed");
    expect(familyChoice(split, "servers")).toBe("never");

    expect(
      Object.fromEntries(
        AUTO_WHITELIST_FAMILIES.map((family) => [family.id, familyChoice(state, family.id)]),
      ),
    ).toEqual({
      direct_messages: "mixed",
      groups: "mixed",
      servers: "ask_me",
      channels: "mixed",
      threads: "mixed",
      email: "mixed",
      posts: "never",
    });
  });

  it("sets every place in one family and leaves the rest alone", () => {
    const after = setFamilyRule(state, "channels", "always");
    const channels = familyPlaceKinds("channels").map((place) => place.ruleKey);

    expect(channels).toEqual(["discord:server_channel", "discord:channel", "whatsapp:channel", "telegram:channel"]);
    for (const ruleKey of channels) expect(after.draft[ruleKey]).toBe("always");
    expect(familyChoice(after, "channels")).toBe("always");
    for (const place of AUTO_WHITELIST_PLACE_KINDS) {
      if (channels.includes(place.ruleKey)) continue;
      expect(after.draft[place.ruleKey]).toBe(state.draft[place.ruleKey]);
    }
    expect(changedRuleKeys(after)).toEqual(
      channels.filter((ruleKey) => AUTO_WHITELIST_RULES_SCREEN_SAVED[ruleKey] !== "always"),
    );
  });

  it("refuses a family or a choice the screen does not have", () => {
    expect(() => setFamilyRule(state, "bulletin_boards", "always")).toThrow(/Unknown place family/u);
    expect(() => setFamilyRule(state, "channels", "sometimes")).toThrow(/Unknown rule choice/u);
    expect(() => familyChoice(state, "servers ")).toThrow(/Unknown place family/u);
    expect(() => familyForKind("bulletin_board")).toThrow(/is in no family/u);
  });

  it("offers every choice on each family control, plus mixed only when mixed", () => {
    const family = html.match(
      /<div class="rule-family" data-rule-family="channels"[\s\S]*?<\/select>/u,
    )?.[0];
    expect(family).toBeTruthy();
    const options = [...(family ?? "").matchAll(/<option value="([^"]+)"/gu)].map((match) => match[1]);

    expect(options).toEqual(["mixed", ...AUTO_WHITELIST_CHOICES.map((choice) => choice.id)]);
    expect(family).toContain(`<option value="mixed" selected>mixed</option>`);

    const settled = renderAutoWhitelistRulesScreen(setFamilyRule(state, "channels", "always"));
    const after = settled.match(
      /<div class="rule-family" data-rule-family="channels"[\s\S]*?<\/select>/u,
    )?.[0];
    expect(after).not.toContain(`value="mixed"`);
    expect(after).toContain(`<option value="always" selected>always</option>`);
  });
});
