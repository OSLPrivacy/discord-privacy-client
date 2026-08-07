import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

import {
  AUTO_WHITELIST_CHOICES,
  AUTO_WHITELIST_PLACE_GROUPS,
  AUTO_WHITELIST_PLACE_KINDS,
  autoWhitelistRulePayload,
  autoWhitelistRulesState,
  changedRuleKeys,
  placeRuleRows,
  renderAutoWhitelistRulesScreen,
  resetRules,
  ruleStatusLine,
  saveRules,
  setPlaceRule,
} from "./auto-whitelist-rules-screen";
import {
  AUTO_WHITELIST_RULES_SCREEN_REQUIRED_TEXT,
  AUTO_WHITELIST_RULES_SCREEN_SAVED,
} from "./auto-whitelist-rules-screen-data";

const RUST_PATH = path.resolve(
  import.meta.dirname,
  "../../../crates/ipc/src/auto_whitelist_rules.rs",
);
const RUST = readFileSync(RUST_PATH, "utf8");

/** The body of an `impl <name> { ... }` block, by brace balance. */
function implBlock(name: string): string {
  const start = RUST.indexOf(`impl ${name} {`);
  expect(start, `impl ${name} in ${RUST_PATH}`).toBeGreaterThan(-1);
  const open = RUST.indexOf("{", start);
  let depth = 0;
  for (let index = open; index < RUST.length; index += 1) {
    if (RUST[index] === "{") depth += 1;
    else if (RUST[index] === "}") {
      depth -= 1;
      if (depth === 0) return RUST.slice(open + 1, index);
    }
  }
  throw new Error(`unbalanced impl ${name}`);
}

/** The string arms of a `pub fn <fn>(self) -> &'static str` in a block. */
function strArms(block: string, fnName: string): string[] {
  const match = block.match(
    new RegExp(`pub fn ${fnName}\\(self\\) -> &'static str \\{([\\s\\S]*?)\\n    \\}`, "u"),
  );
  expect(match, `fn ${fnName}`).not.toBeNull();
  return [...(match as RegExpMatchArray)[1].matchAll(/Self::\w+ => "([^"]+)"/gu)].map(
    (arm) => arm[1],
  );
}

/** Every place kind the Rust side knows, as `app/kind` pairs. */
function rustPlaceKinds(): { app: string; kind: string; name: string | null }[] {
  const kinds: { app: string; kind: string; name: string | null }[] = [];
  for (const match of RUST.matchAll(/pub enum (\w+)WhitelistKind \{/gu)) {
    const enumName = `${match[1]}WhitelistKind`;
    const block = implBlock(enumName);
    const ids = strArms(block, "id");
    const names = strArms(block, "name");
    expect(names.length, `${enumName} name() arms`).toBe(ids.length);
    const app = match[1].toLowerCase();
    ids.forEach((id, index) => kinds.push({ app, kind: id, name: names[index] }));
  }
  // X keeps its kinds as consts rather than an enum.
  const consts = new Map(
    [...RUST.matchAll(/pub const (X_\w+_PLACE_KIND): &str = "([a-z_]+)";/gu)].map((match) => [
      match[1],
      match[2],
    ]),
  );
  const list = RUST.match(/pub const X_PLACE_KINDS: \[&str; \d+\] = \[([\s\S]*?)\];/u);
  expect(list, "X_PLACE_KINDS").not.toBeNull();
  for (const match of (list as RegExpMatchArray)[1].matchAll(/(X_\w+_PLACE_KIND)/gu)) {
    const kind = consts.get(match[1]);
    expect(kind, match[1]).toBeTruthy();
    kinds.push({ app: "x", kind: kind as string, name: null });
  }
  // Email is not an enum either: `auto_whitelist_app_kind_for_place` maps its
  // two place kinds straight to their rule keys.
  const email = RUST.match(/"email" => match kind\.as_str\(\) \{([\s\S]*?)\n        \},/u);
  expect(email, "email arm of auto_whitelist_app_kind_for_place").not.toBeNull();
  for (const match of (email as RegExpMatchArray)[1].matchAll(
    /Ok\("(email_\w+)"\.to_string\(\)\)/gu,
  )) {
    kinds.push({ app: "email", kind: match[1], name: null });
  }
  return kinds;
}

const RUST_PLACE_KINDS = rustPlaceKinds();

function key(entry: { app: string; kind: string }): string {
  return `${entry.app}/${entry.kind}`;
}

describe("TASK 0740 auto-whitelist rules screen catalogue", () => {
  it("shows every place kind the native side knows, and no invented one", () => {
    const rust = RUST_PLACE_KINDS.map(key).sort();
    const screen = AUTO_WHITELIST_PLACE_KINDS.map(key).sort();

    expect(screen).toEqual(rust);
    expect(new Set(screen).size).toBe(screen.length);
    expect(screen.length).toBe(38);
  });

  it("names each place kind the way the native side names it", () => {
    const named = RUST_PLACE_KINDS.filter((entry) => entry.name !== null);
    const byKey = new Map(AUTO_WHITELIST_PLACE_KINDS.map((place) => [key(place), place.kindLabel]));

    expect(named.length).toBe(32);
    for (const entry of named) {
      expect(byKey.get(key(entry)), key(entry)).toBe(entry.name);
    }
  });

  it("builds each rule key the way the native side builds it", () => {
    for (const place of AUTO_WHITELIST_PLACE_KINDS) {
      if (place.app === "x") expect(place.ruleKey).toBe(`x/${place.kind}`);
      else if (place.app === "email") expect(place.ruleKey).toBe(place.kind);
      else expect(place.ruleKey).toBe(`${place.app}:${place.kind}`);
    }
    // Those three shapes are the ones the Rust file writes.
    for (const app of ["discord", "telegram", "messenger", "instagram"]) {
      expect(RUST).toContain(`format!("${app}:{}", kind.id())`);
    }
    expect(RUST).toContain(`Self::DirectMessage => "signal:direct_message"`);
    expect(RUST).toContain(`Self::DirectMessage => "whatsapp:direct_message"`);
    expect(RUST).toContain(`format!("{app_kind}/{place_kind}")`);
    expect(new Set(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.ruleKey)).size).toBe(38);
  });

  it("offers the four choices the native side offers, in that order", () => {
    const block = implBlock("AutoWhitelistChoice");

    expect(AUTO_WHITELIST_CHOICES.map((choice) => choice.id)).toEqual(strArms(block, "id"));
    expect(AUTO_WHITELIST_CHOICES.map((choice) => choice.label)).toEqual(strArms(block, "label"));
  });

  it("explains every choice in one short line", () => {
    for (const choice of AUTO_WHITELIST_CHOICES) {
      expect(choice.explanation.length).toBeGreaterThan(20);
      expect(choice.explanation.length).toBeLessThan(70);
      expect(choice.explanation.endsWith(".")).toBe(true);
    }
  });

  it("puts every place kind in exactly one drawn group", () => {
    const grouped = AUTO_WHITELIST_PLACE_GROUPS.flatMap((group) => group.kinds).map(key);

    expect(grouped.sort()).toEqual(AUTO_WHITELIST_PLACE_KINDS.map(key).sort());
    expect(new Set(AUTO_WHITELIST_PLACE_GROUPS.map((group) => group.column))).toEqual(
      new Set([1, 2]),
    );
  });
});

describe("TASK 0740 auto-whitelist rules state", () => {
  it("starts every unset place kind at the native default", () => {
    const state = autoWhitelistRulesState();

    expect(Object.keys(state.draft).length).toBe(38);
    expect(new Set(Object.values(state.draft))).toEqual(new Set(["never"]));
    expect(changedRuleKeys(state)).toEqual([]);
    expect(ruleStatusLine(state)).toBe("All 38 place rules saved.");
    expect(RUST).toContain("#[default]\n    Never,");
  });

  it("changes one place kind and leaves the other 37 alone", () => {
    const before = autoWhitelistRulesState(AUTO_WHITELIST_RULES_SCREEN_SAVED);
    const after = setPlaceRule(before, "discord:thread", "always");

    expect(changedRuleKeys(after)).toEqual(["discord:thread"]);
    expect(after.draft["discord:thread"]).toBe("always");
    expect(ruleStatusLine(after)).toBe("1 place rule changed - not saved yet.");
    for (const place of AUTO_WHITELIST_PLACE_KINDS) {
      if (place.ruleKey === "discord:thread") continue;
      expect(after.draft[place.ruleKey]).toBe(before.draft[place.ruleKey]);
    }
  });

  it("refuses a place kind or a choice the native side would reject", () => {
    const state = autoWhitelistRulesState();

    expect(() => setPlaceRule(state, "discord:bulletin_board", "always")).toThrow(
      /Unknown place rule key/u,
    );
    expect(() => setPlaceRule(state, "discord:thread", "sometimes")).toThrow(
      /Unknown rule choice/u,
    );
    expect(() => autoWhitelistRulesState({ "slack:channel": "always" })).toThrow(
      /Unknown place rule key/u,
    );
  });

  it("saves the draft as the kept rules and hands back one rule per place kind", () => {
    const edited = setPlaceRule(
      autoWhitelistRulesState(AUTO_WHITELIST_RULES_SCREEN_SAVED),
      "x/reply",
      "ask_me",
    );
    const result = saveRules(edited);

    expect(changedRuleKeys(result.state)).toEqual([]);
    expect(ruleStatusLine(result.state)).toBe("All 38 place rules saved.");
    expect(result.saved.length).toBe(38);
    expect(result.saved.find((rule) => rule.ruleKey === "x/reply")).toEqual({
      ruleKey: "x/reply",
      app: "x",
      kind: "reply",
      choice: "ask_me",
    });
    expect(result.saved.map((rule) => rule.ruleKey)).toEqual(
      AUTO_WHITELIST_PLACE_KINDS.map((place) => place.ruleKey),
    );
  });

  it("resets every place kind to never without keeping it until saved", () => {
    const state = autoWhitelistRulesState(AUTO_WHITELIST_RULES_SCREEN_SAVED);
    const reset = resetRules(state);

    expect(new Set(Object.values(reset.draft))).toEqual(new Set(["never"]));
    expect(reset.saved).toEqual(state.saved);
    expect(changedRuleKeys(reset).length).toBe(26);
    expect(ruleStatusLine(reset)).toBe("26 place rules changed - not saved yet.");

    const kept = saveRules(reset);
    expect(new Set(Object.values(kept.state.saved))).toEqual(new Set(["never"]));
    expect(new Set(kept.saved.map((rule) => rule.choice))).toEqual(new Set(["never"]));
  });
});

describe("TASK 0740 auto-whitelist rules markup", () => {
  const state = autoWhitelistRulesState(AUTO_WHITELIST_RULES_SCREEN_SAVED);
  const html = renderAutoWhitelistRulesScreen(state);

  it("draws one row per place kind with its selected rule", () => {
    const rows = [...html.matchAll(/<li class="rule-row" data-rule-key="([^"]+)"[^>]*?data-selected="([^"]+)"/gu)];

    expect(rows.length).toBe(38);
    expect(rows.map((row) => row[1])).toEqual(
      AUTO_WHITELIST_PLACE_KINDS.map((place) => place.ruleKey),
    );
    expect(rows.map((row) => row[2])).toEqual(
      AUTO_WHITELIST_PLACE_KINDS.map((place) => AUTO_WHITELIST_RULES_SCREEN_SAVED[place.ruleKey]),
    );
  });

  it("marks exactly one choice on per row, and offers all four", () => {
    expect([...html.matchAll(/data-state="on"/gu)].length).toBe(38);
    expect([...html.matchAll(/class="rule-choice"/gu)].length).toBe(38 * 4);
    expect([...html.matchAll(/checked/gu)].length).toBe(38);
  });

  it("carries the buttons, their explanations and the status line", () => {
    expect(html).toContain(`data-rule-action="save"`);
    expect(html).toContain(`data-rule-action="reset"`);
    expect(html).toContain("Keeps these rules for every new place from now on.");
    expect(html).toContain("Puts every place kind back to never.");
    expect(html).toContain("All 38 place rules saved.");
  });

  it("shows every phrase the screenshot check looks for", () => {
    const text = html.replace(/<[^>]+>/gu, " ").replace(/\s+/gu, " ").toLowerCase();
    for (const phrase of AUTO_WHITELIST_RULES_SCREEN_REQUIRED_TEXT) {
      expect(text, phrase).toContain(phrase.toLowerCase());
    }
    expect(AUTO_WHITELIST_RULES_SCREEN_REQUIRED_TEXT.length).toBeGreaterThan(30);
  });

  it("uses all four choices in the screenshot's saved rules", () => {
    const rows = placeRuleRows(state);

    expect(new Set(rows.map((row) => row.choice))).toEqual(
      new Set(AUTO_WHITELIST_CHOICES.map((choice) => choice.id)),
    );
    expect(autoWhitelistRulePayload(state.draft).length).toBe(38);
  });

  it("escapes what it draws", () => {
    expect(html).not.toContain("<script");
    expect(html.includes("&amp;") || !html.includes("&")).toBe(true);
  });
});
