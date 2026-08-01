import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Every destination moves focus to `#route-heading` after a navigation, and
 * announces itself through the landmark that owns it. Two things have to be
 * true for that to say anything: the id must sit on the heading (a landmark
 * with the id on itself has no accessible name and a screen reader announces
 * nothing), and exactly one element in the document may claim
 * `aria-current="page"`.
 */
const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
/** Markup only. A rationale that quotes an attribute is not an attribute. */
const markup = source.replace(/^\s*\/\/.*$/gmu, "").replace(/\/\*[\s\S]*?\*\//gu, "");

function destinationMain(className: string): string {
  const at = source.indexOf(`<main class="content-viewport ${className}"`);
  expect(at, `${className} should render a <main>`).toBeGreaterThanOrEqual(0);
  return source.slice(at, source.indexOf(">", at) + 1);
}

describe("route heading landmarks", () => {
  for (const [className, heading] of [
    ["inbox-destination", "Conversations"],
    ["activity-destination", "Activity"],
    ["connections-destination", "Connections"],
    ["privacy-destination", "Privacy"],
    ["settings-page", "Settings"],
  ] as const) {
    it(`names the ${className} landmark with its own heading`, () => {
      const openingTag = destinationMain(className);
      expect(openingTag).toContain('aria-labelledby="route-heading"');
      // The id belongs to the heading. Inbox had it on the <main>, so the
      // landmark was unnamed, the <h1> was unreachable, and arriving at Inbox
      // announced nothing.
      expect(openingTag).not.toContain('id="route-heading"');
      expect(source).toContain(`<h1 id="route-heading" tabindex="-1">${heading}</h1>`);
    });
  }

  it("focuses the heading after a navigation, which is why the id has to be on it", () => {
    expect(source).toContain('document.querySelector<HTMLElement>("#route-heading")?.focus()');
  });

  it("marks exactly one element per screen as the current page", () => {
    // The primary sidebar owns `aria-current="page"`: the destination, or
    // Settings. Settings' own section buttons choose a section WITHIN that
    // page, so marking them `page` too put two current-page markers on screen
    // at once.
    const pageCurrent = markup.match(/aria-current="page"/gu) ?? [];
    expect(pageCurrent).toHaveLength(2);
    const sidebar = markup.slice(markup.indexOf("function primarySidebarMarkup"), markup.indexOf("function appLauncherStrip"));
    expect(sidebar.match(/aria-current="page"/gu)).toHaveLength(2);

    const settings = markup.slice(markup.indexOf("function settingsContent"), markup.indexOf("function settingsSectionContent"));
    expect(settings).toContain('aria-current="true"');
    expect(settings).not.toContain('aria-current="page"');
  });
});
