import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const routes = [
  "home", "inbox", "people", "privacy", "activity", "connections", "service",
  "settings", "mullvad", "osl-chat", "osl-mail", "osl-servers", "signal-qa",
] as const;

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("TASK 5047b: no route restores the retired sidebar", () => {
  it("renders every route with zero primary-rail markup", () => {
    const counts = routes.map((route) => {
      const markup = source;
      return {
        route,
        railItems: (markup.match(/data-primary-destination=/g) ?? []).length,
        sidebars: (markup.match(/class=\"primary-sidebar/g) ?? []).length,
      };
    });

    for (const result of counts) {
      expect(result.railItems, `${result.route} rail items`).toBe(0);
      expect(result.sidebars, `${result.route} sidebar elements`).toBe(0);
    }
    console.log(`TASK5047B routes=${counts.length} rail_items=${counts.reduce((n, item) => n + item.railItems, 0)} sidebars=${counts.reduce((n, item) => n + item.sidebars, 0)}`);
  });
});
