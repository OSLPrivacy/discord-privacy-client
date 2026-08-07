import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { whitelistDropdownMarkup, type WhitelistDropdownPerson } from "./whitelist-dropdown";

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

type RenderedRow = {
  name: string;
  checked: boolean;
};

function renderedRows(markup: string): RenderedRow[] {
  const rows: RenderedRow[] = [];
  const pattern = /<label class="whitelist-dropdown-row"[^>]*>([\s\S]*?)<\/label>/gu;
  for (const match of markup.matchAll(pattern)) {
    const row = match[1] ?? "";
    rows.push({
      name: /<strong>([^<]+)<\/strong>/u.exec(row)?.[1] ?? "",
      checked: /<input\b[^>]*\bchecked\b/u.test(row),
    });
  }
  return rows;
}

describe("TASK0116 whitelist dropdown", () => {
  it("wires the Whitelist button to a dropdown renderer", () => {
    expect(mainSource).toContain('aria-haspopup="menu" aria-controls="whitelist-roster-dropdown"');
    expect(mainSource).toContain('aria-expanded="${whitelistRosterOpen}"');
    expect(mainSource).toContain("whitelistRosterOpen = !whitelistRosterOpen;");
    expect(mainSource).toContain("return whitelistDropdownMarkup({");
  });

  it("renders three named group people with mixed tick states", () => {
    const people: readonly WhitelistDropdownPerson[] = [
      {
        personId: "ada",
        alias: "Ada Lovelace",
        whitelistedScopes: [{ kind: "group", storageKey: "group:math-circle" }],
      },
      {
        personId: "grace",
        alias: "Grace Hopper",
        whitelistedScopes: [],
      },
      {
        personId: "katherine",
        alias: "Katherine Johnson",
        whitelistedScopes: [],
      },
    ];

    const rows = renderedRows(whitelistDropdownMarkup({
      open: true,
      people,
      activePersonId: "katherine",
      activeScopeApproved: true,
      busy: false,
      groupStorageKey: "group:math-circle",
    }));

    const names = rows.map((row) => row.name);
    const tickStates = rows.map((row) => row.checked);
    const checked = tickStates.filter(Boolean).length;
    const unchecked = tickStates.length - checked;

    console.log(`TASK0116 rows=${rows.length}`);
    console.log(`TASK0116 names=${names.join(", ")}`);
    console.log(`TASK0116 tick_states=${tickStates.map(String).join(", ")}`);
    console.log(`TASK0116 checked=${checked} unchecked=${unchecked}`);

    expect(rows).toHaveLength(3);
    expect(names).toEqual(["Ada Lovelace", "Grace Hopper", "Katherine Johnson"]);
    expect(tickStates).toEqual([true, false, true]);
    expect(checked).toBe(2);
    expect(unchecked).toBe(1);
  });
});
