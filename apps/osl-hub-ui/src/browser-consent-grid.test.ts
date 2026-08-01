import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * The browser-consent grid: the screen on which the user grants OSL permission
 * to read a browser profile. Everything asserted here is read out of
 * styles.css, the file the WebView loads. None of it is read out of main.ts:
 * the shipped CSP is `style-src 'self'`, so styling that exists only as a
 * string inside a TypeScript function renders nothing while a source-text test
 * happily passes.
 */
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = new RegExp(`(?:^|\\})\\s*${escaped}\\s*\\{([^}]*)\\}`, "mu").exec(styles);
  expect(match, `${selector} should have a rule in styles.css`).not.toBeNull();
  return match![1];
}

describe("browser consent grid", () => {
  it("lets the row's text column shrink inside its own grid track", () => {
    // A flex item's min-width is `auto`, i.e. its min-content width. Without
    // this the <span> refused to shrink below the longest browser-and-profile
    // name, the row overflowed its minmax(0, 1fr) track, and the
    // `margin-left: auto` checkbox was pushed out of the cell -- out of the
    // fieldset entirely in the last column. Rows overlapped the next row's
    // logo and nothing was legible.
    expect(rule(".browser-detected-item > span")).toMatch(/min-width:\s*0/u);
  });

  it("never lets the consent checkbox be shrunk or pushed out of its row", () => {
    const checkbox = rule(".browser-detected-item input");
    expect(checkbox).toMatch(/flex:\s*0 0 auto/u);
    expect(checkbox).toMatch(/margin-left:\s*auto/u);
  });

  it("gives the profile name enough width to be told apart from its siblings", () => {
    // .onboarding-panel is capped at 620px. At three tracks a row spends most
    // of ~200px on padding, the browser mark, the gaps and the checkbox, and
    // every profile truncated to the same "Google Chrom..." stub -- which makes
    // a per-profile consent choice impossible to make.
    expect(rule(".browser-detected-list")).toMatch(/grid-template-columns:\s*repeat\(2, minmax\(0, 1fr\)\)/u);
    expect(styles).toMatch(/\.onboarding-panel\s*\{[^}]*width:\s*min\(620px/u);
    // Right-hand border only on the second of each pair, matching two tracks.
    expect(styles).toContain(".browser-detected-item:nth-child(2n) { border-right: 0; }");
    expect(styles).not.toContain(".browser-detected-item:nth-child(3n) { border-right: 0; }");
  });

  it("wraps the profile name instead of cutting it off", () => {
    // The name is the one fact on the row that must survive; the boilerplate
    // sentence under it is the one that may ellipsise.
    const name = rule(".browser-detected-item strong");
    expect(name).toMatch(/overflow-wrap:\s*anywhere/u);
    expect(name).not.toMatch(/white-space:\s*nowrap/u);
    expect(name).not.toMatch(/text-overflow/u);

    const detail = rule(".browser-detected-item small");
    expect(detail).toMatch(/text-overflow:\s*ellipsis/u);
    expect(detail).toMatch(/white-space:\s*nowrap/u);
  });

  it("drops to a single column before the row gets narrower than a browser name", () => {
    // styles.css has more than one `max-width: 620px` block, so find the one
    // that actually mentions this grid rather than the first one.
    const blocks = styles.split("@media (max-width: 620px) {").slice(1)
      .map((block) => block.slice(0, block.indexOf("\n}")));
    const narrow = blocks.find((block) => block.includes(".browser-detected-list"));
    expect(narrow, "the 620px breakpoint should restate the consent grid").toBeDefined();
    expect(narrow!).toMatch(/\.browser-detected-list \{ grid-template-columns: minmax\(0, 1fr\); \}/u);
    expect(narrow!).toMatch(/\.browser-detected-item \{ border-right: 0; \}/u);
  });

  it("keeps the section rule unbroken above the fieldset's label", () => {
    // A <legend> that is the first child of a <fieldset> is dropped into a
    // notch cut out of the fieldset's own border. At `width: 100%` that notch
    // was the entire border-top, so the rule never painted and the label sat
    // where it should have been. Only a float (or absolute positioning) opts a
    // legend out of that rendering.
    const legend = rule(".browser-detected-sources > legend");
    expect(legend).toMatch(/float:\s*left/u);
    expect(rule(".browser-detected-sources")).toMatch(/border-top:\s*1px solid var\(--line\)/u);
    // A float is overlapped by the block boxes after it, not pushed past them,
    // so the list has to clear it or the first row draws through the label.
    expect(rule(".browser-detected-list")).toMatch(/clear:\s*both/u);
  });

  it("aligns the section label with the row text underneath it", () => {
    expect(rule(".browser-detected-sources > legend")).toMatch(/padding:\s*0 12px/u);
    expect(rule(".browser-detected-item")).toMatch(/padding:\s*10px 12px/u);
  });

  it("is the grid the consent markup actually builds", () => {
    // Guards the selectors above against a markup rename that would leave every
    // rule in this file live but attached to nothing.
    expect(source).toContain('<fieldset class="browser-detected-sources"');
    expect(source).toContain("<legend>Choose saved browser areas</legend>");
    expect(source).toContain('<div class="browser-detected-list">');
    expect(source).toMatch(/<label class="browser-detected-item">\$\{browserLogo\([^)]*\)\}<span><strong>/u);
    expect(source).toMatch(/<\/span><input type="checkbox" data-browser-profile=/u);
  });
});
