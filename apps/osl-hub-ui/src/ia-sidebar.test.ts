import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { oslPrimaryDestinationValues, oslPrimaryDestinations, oslSettingsDestination } from "./state";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/**
 * Declaration text only, so a rule quoted inside a `/* ... *\/` rationale can
 * never stand in for the rule itself.
 */
const styleDeclarations = styles.replace(/\/\*[\s\S]*?\*\//gu, "");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

/**
 * The declarations of one top-level rule in styles.css. Anchored on a newline
 * so `.primary-sidebar` cannot be satisfied by `.primary-sidebar-item`, and so
 * the top-level rule is never confused with its indented `@media` override.
 */
function ruleBody(selector: string): string {
  const start = styleDeclarations.indexOf(`\n${selector} {`);
  expect(start, `${selector} should be a top-level rule in styles.css`).toBeGreaterThanOrEqual(0);
  const open = styleDeclarations.indexOf("{", start);
  const close = styleDeclarations.indexOf("}", open);
  expect(close).toBeGreaterThan(open);
  return styleDeclarations.slice(open + 1, close);
}

describe("fixed desktop IA sidebar", () => {
  const sidebar = functionSource("primarySidebarMarkup", "appLauncherStrip");
  /** The emitted markup only: a rationale in a comment must never satisfy a check. */
  const sidebarCode = sidebar.replace(/^\s*\/\/.*$/gmu, "");

  it("is rendered as the first column of the workspace shell", () => {
    const renderWorkspace = functionSource("renderWorkspace", "primarySidebarMarkup");
    expect(renderWorkspace).toContain('class="hub-layout with-primary-sidebar"');
    expect(renderWorkspace).toContain("${primarySidebarMarkup()}<section class=\"hub-workspace\"");
    // The 232px first column is asserted in styles.css, where it is the only
    // copy and the only one that can apply. The shipped CSP (tauri.conf.json,
    // app.security.csp) is `style-src 'self'` with no `'unsafe-inline'`, no
    // nonce and no hash, so the WebView drops runtime style elements and inline
    // style attributes outright; the stylesheet is a `'self'` asset and is not
    // dropped. This assertion used to read primarySidebarMarkup()'s own source
    // text, which meant it passed against a string the WebView never honoured --
    // green for the whole time the navigation was rendering as unstyled native
    // buttons wrapping across the top of the window.
    expect(ruleBody(".hub-layout.with-primary-sidebar")).toContain("grid-template-columns: 232px minmax(0, 1fr)");
    expect(ruleBody(".primary-sidebar")).toContain("width: 232px");
    expect(ruleBody(".primary-sidebar")).toContain("grid-template-rows: auto minmax(0, 1fr) auto");
    // And the CSP-dead copy may not come back: the emitted markup carries no
    // styling of its own for the WebView to drop.
    expect(sidebarCode).not.toContain("<style");
    expect(sidebarCode).not.toContain('style="');
  });

  it("uses the fixed six primary destinations in model order", () => {
    expect(sidebar).toContain("oslPrimaryDestinations.map");
    expect(sidebar).toContain('aria-label="Primary destinations"');
    for (const destination of oslPrimaryDestinationValues) {
      expect(sidebar).toContain(`id === "${destination}"`);
    }
    expect(oslPrimaryDestinations.map((destination) => destination.id)).toEqual(oslPrimaryDestinationValues);
  });

  it("keeps Settings fixed outside the primary destinations", () => {
    expect(oslPrimaryDestinationValues).not.toContain(oslSettingsDestination);
    expect(sidebar).toContain('class="primary-sidebar-settings');
    expect(sidebar).toContain('data-route="${oslSettingsDestination}"');
    expect(sidebar.indexOf('aria-label="Primary destinations"')).toBeLessThan(sidebar.indexOf('class="primary-sidebar-settings'));
  });

  it("does not expose implementation concepts as navigation copy", () => {
    const visibleCopy = [
      ...oslPrimaryDestinations.flatMap((destination) => [destination.label, destination.userQuestion]),
      "Settings",
      "OSL",
    ].join("\n");
    expect(visibleCopy).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/i);
    expect(sidebar).not.toMatch(/data-sidebar-move|data-sidebar-toggle|Move or hide apps/i);
  });
});
