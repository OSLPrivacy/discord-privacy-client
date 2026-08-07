import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  autoRuleChoiceIdFor,
  autoWhitelistRuleChoices,
  configuredAutoWhitelistAppKinds,
  DEFAULT_AUTO_WHITELIST_CHOICE,
  loadSavedAutoWhitelistChoices,
  READ_AUTO_WHITELIST_RULE_COMMAND,
  SAVE_AUTO_WHITELIST_RULE_COMMAND,
  saveAutoWhitelistRuleChoice,
  whitelistingSettingsMarkup,
  type AutoWhitelistInvoke,
  type AutoWhitelistRuleChoiceId,
} from "./auto-whitelist-settings";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function choiceLabel(id: AutoWhitelistRuleChoiceId): string {
  const match = autoWhitelistRuleChoices.find((choice) => choice.id === id);
  if (!match) throw new Error(`no label for choice ${id}`);
  return match.label;
}

// Mirrors cmd_osl_save_auto_whitelist_rule / cmd_osl_read_auto_whitelist_rule in
// crates/ipc/src/commands.rs: save normalizes the key, rejects unknown choices and
// inserts into prefs.auto_whitelist_rules; read answers the saved choice or the
// default, as a label inside an AutoWhitelistRuleDto. `dropWrite` models a save
// that reports success but never persists.
function fixtureRuleBackend(options: { dropWrite?: boolean } = {}) {
  const savedRules = new Map<string, AutoWhitelistRuleChoiceId>();
  const invoke: AutoWhitelistInvoke = (command, args) => {
    const appKind = String(args?.appKind ?? "").trim().toLowerCase().replace(/[\s-]+/gu, "_");
    if (!appKind) return Promise.reject(new Error("OSL: auto-whitelist app kind is empty"));
    if (command === SAVE_AUTO_WHITELIST_RULE_COMMAND) {
      const choice = autoRuleChoiceIdFor(String(args?.choice ?? ""));
      if (!choice) return Promise.reject(new Error(`OSL: unknown auto-whitelist rule choice '${String(args?.choice)}'`));
      if (!options.dropWrite) savedRules.set(appKind, choice);
      return Promise.resolve({ app_kind: appKind, choice: choiceLabel(choice) });
    }
    if (command === READ_AUTO_WHITELIST_RULE_COMMAND) {
      const choice = savedRules.get(appKind) ?? DEFAULT_AUTO_WHITELIST_CHOICE;
      return Promise.resolve({ app_kind: appKind, choice: choiceLabel(choice) });
    }
    return Promise.reject(new Error(`unexpected command ${command}`));
  };
  return { invoke, savedRules };
}

function selectedChoiceIn(markup: string, appKind: string): string {
  const marker = `data-auto-rule-kind="${appKind}"`;
  const start = markup.indexOf(marker);
  expect(start, `${appKind} row should render`).toBeGreaterThanOrEqual(0);
  const row = markup.slice(start, markup.indexOf("</article>", start));
  const checked = [...row.matchAll(/aria-checked="true"[^>]*data-auto-rule-choice="([^"]+)"/gu)].map((m) => m[1]);
  expect(checked, `${appKind} row should show exactly one selected choice`).toHaveLength(1);
  return checked[0];
}

// The done-when check: reopen settings through the saved-rule read command and
// compare what it shows against what was saved.
async function reopenedSettingsReport(invoke: AutoWhitelistInvoke, savedRules: Map<string, AutoWhitelistRuleChoiceId>) {
  const savedChoices = await loadSavedAutoWhitelistChoices(invoke);
  const markup = whitelistingSettingsMarkup(configuredAutoWhitelistAppKinds, savedChoices);
  const mismatches = configuredAutoWhitelistAppKinds.filter((app) => {
    const expected = savedRules.get(app.appKind) ?? DEFAULT_AUTO_WHITELIST_CHOICE;
    return selectedChoiceIn(markup, app.appKind) !== expected;
  }).map((app) => app.appKind);
  return { savedChoices, markup, mismatches };
}

async function changeRuleThroughControl(
  invoke: AutoWhitelistInvoke,
  appKind: string,
  choiceId: AutoWhitelistRuleChoiceId,
): Promise<AutoWhitelistRuleChoiceId> {
  // Drive the change with the exact data attributes the rendered control carries,
  // the same values the [data-auto-rule-choice] click handler in main.ts reads.
  const markup = whitelistingSettingsMarkup();
  const button = new RegExp(`<button[^>]*data-auto-rule-app-kind="${appKind}"[^>]*data-auto-rule-choice="${choiceId}"[^>]*>`, "u").exec(markup);
  expect(button, `control for ${appKind}/${choiceId} should render`).not.toBeNull();
  const controlKind = /data-auto-rule-app-kind="([^"]+)"/u.exec(button![0])![1];
  const controlChoice = /data-auto-rule-choice="([^"]+)"/u.exec(button![0])![1];
  return saveAutoWhitelistRuleChoice(invoke, controlKind, controlChoice);
}

describe("TASK 0142 connect auto-rule controls", () => {
  it("saves a changed fixture rule and reopening settings shows exactly that one saved choice", async () => {
    const { invoke, savedRules } = fixtureRuleBackend();
    const accepted = await changeRuleThroughControl(invoke, "discord", "only_if_a_friend");
    expect(accepted).toBe("only_if_a_friend");

    const { savedChoices, mismatches } = await reopenedSettingsReport(invoke, savedRules);
    expect(Object.entries(savedChoices)).toEqual([["discord", "only_if_a_friend"]]);
    expect(mismatches).toHaveLength(0);

    console.log(
      `TASK0142 saved_choice_count=${Object.keys(savedChoices).length} ` +
      `saved_choice=${Object.entries(savedChoices).map(([kind, choice]) => `${kind}:${choice}`).join("|")} ` +
      `reopened_mismatch_count=${mismatches.length}`,
    );
  });

  it("fails the reopen check when the save write is deliberately dropped", async () => {
    const { invoke, savedRules } = fixtureRuleBackend({ dropWrite: true });
    const accepted = await changeRuleThroughControl(invoke, "discord", "only_if_a_friend");
    expect(accepted).toBe("only_if_a_friend");
    expect(savedRules.size).toBe(0);

    const { savedChoices } = await reopenedSettingsReport(invoke, savedRules);
    const check = () => {
      expect(Object.entries(savedChoices)).toEqual([["discord", "only_if_a_friend"]]);
    };
    expect(check).toThrowError();
    console.log(`TASK0142 dropped_write_saved_choice_count=${Object.keys(savedChoices).length} check=fails`);
  });

  it("wires the controls to the saved-rule commands in main.ts", () => {
    const bindStart = mainSource.indexOf("function bindSavedAccountControls()");
    expect(bindStart).toBeGreaterThanOrEqual(0);
    const handler = mainSource.slice(bindStart, mainSource.indexOf("\nfunction ", bindStart + 1));
    expect(handler).toContain('[data-auto-rule-choice]');
    expect(handler).toContain("saveAutoWhitelistRuleChoice((command, args) => invoke(command, args), appKind, choice)");
    expect(handler).toContain("refreshAutoWhitelistSavedChoices()");

    const refreshStart = mainSource.indexOf("async function refreshAutoWhitelistSavedChoices()");
    expect(refreshStart).toBeGreaterThanOrEqual(0);
    const refresh = mainSource.slice(refreshStart, mainSource.indexOf("\nfunction ", refreshStart));
    expect(refresh).toContain("loadSavedAutoWhitelistChoices((command, args) => invoke(command, args))");

    expect(mainSource).toContain('if (next === "apps") void refreshAutoWhitelistSavedChoices();');
    expect(mainSource).toContain("whitelistingSettingsMarkup(configuredAutoWhitelistAppKinds, autoWhitelistSavedChoices)");
  });
});
