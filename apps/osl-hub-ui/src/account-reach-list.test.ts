import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { accountReachListErrors, accountReachListMarkup, type AccountReachChoice } from "./account-reach-list";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = new RegExp(`(?:^|\\})\\s*${escaped}\\s*\\{([^}]*)\\}`, "mu").exec(styles);
  expect(match, `${selector} should have a rule in styles.css`).not.toBeNull();
  return match![1];
}

/** TASK 0244 fixture: four named owned accounts with mixed tick states. */
const FOUR_OWNED_ACCOUNTS: AccountReachChoice[] = [
  { serviceId: "gmail", accountId: "gmail-primary", label: "Gmail (primary)", checked: true },
  { serviceId: "discord", accountId: "discord-alt", label: "Discord (alt)", checked: false },
  { serviceId: "signal", accountId: "signal-personal", label: "Signal (personal)", checked: true },
  { serviceId: "telegram", accountId: "telegram-work", label: "Telegram (work)", checked: false },
];

describe("account reach tick-box list", () => {
  it("draws one row per owned account", () => {
    const markup = accountReachListMarkup(FOUR_OWNED_ACCOUNTS);
    const rows = [...markup.matchAll(/<label class="account-reach-item" data-account-reach-item="([^"]+)">/gu)];
    expect(rows.map((row) => row[1])).toEqual([
      "gmail-primary",
      "discord-alt",
      "signal-personal",
      "telegram-work",
    ]);
  });

  it("shows a fixture of four named accounts with mixed tick states", () => {
    const markup = accountReachListMarkup(FOUR_OWNED_ACCOUNTS);
    const checkboxes = [...markup.matchAll(/<input type="checkbox" data-account-reach="([^"]+)" data-account-reach-service="[^"]+" (checked)?\/>/gu)];

    expect(checkboxes).toHaveLength(4);
    const states = Object.fromEntries(checkboxes.map((box) => [box[1], box[2] === "checked"]));
    expect(states).toEqual({
      "gmail-primary": true,
      "discord-alt": false,
      "signal-personal": true,
      "telegram-work": false,
    });
    // Mixed, not all-on or all-off.
    const checkedCount = Object.values(states).filter(Boolean).length;
    expect(checkedCount).toBeGreaterThan(0);
    expect(checkedCount).toBeLessThan(4);

    for (const account of FOUR_OWNED_ACCOUNTS) {
      expect(markup).toContain(account.label);
    }
  });

  it("labels the list 'What they can see'", () => {
    const markup = accountReachListMarkup(FOUR_OWNED_ACCOUNTS);
    expect(markup).toMatch(/<fieldset class="account-reach-sources"[^>]*>\s*<legend>What they can see<\/legend>/u);
  });

  it("draws a named empty state when there are no owned accounts", () => {
    const markup = accountReachListMarkup([]);
    expect(markup).toContain('data-account-count="0"');
    expect(markup).toContain("No owned accounts yet");
    expect(markup).not.toContain("<input");
  });

  it("refuses to draw a tick state that is neither on nor off", () => {
    const broken = [{ serviceId: "s", accountId: "a", label: "A", checked: "yes" as unknown as boolean }];
    expect(accountReachListErrors(broken)).toEqual(["account reach entry 0 tick state is not on or off: \"yes\""]);
    expect(() => accountReachListMarkup(broken)).toThrow(/tick state is not on or off/u);
  });

  it("refuses to draw a duplicate account id", () => {
    const broken = [
      { serviceId: "one", accountId: "dup", label: "One", checked: true },
      { serviceId: "two", accountId: "dup", label: "Two", checked: false },
    ];
    expect(accountReachListErrors(broken)).toEqual(["account reach entry 1 repeats account id dup"]);
  });

  it("never lets the tick box be shrunk or pushed out of its row", () => {
    const item = rule(".account-reach-item input");
    expect(item).toMatch(/flex:\s*0 0 auto/u);
    expect(item).toMatch(/margin-left:\s*auto/u);
  });

  it("keeps the section rule unbroken above the fieldset's label", () => {
    const legend = rule(".account-reach-sources > legend");
    expect(legend).toMatch(/float:\s*left/u);
    expect(rule(".account-reach-list")).toMatch(/clear:\s*both/u);
  });
});
