import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("launcher workspace shell", () => {
  it("does not build the retired six-item destination rail", () => {
    const shell = source.slice(
      source.indexOf("function workspaceShellMarkup"),
      source.indexOf("export interface DestinationRouteTarget"),
    );
    expect(shell).toContain('class="hub-layout"');
    expect(shell).toContain("data-shared-launcher-header");
    expect(shell).not.toMatch(/primary-sidebar|primarySidebarMarkup|data-primary-destination|with-primary-sidebar/u);
  });

  it("pins the shared launcher header to 58px", () => {
    expect(styles).toMatch(/:root\s*\{[^}]*--chrome-row-height:\s*58px;/su);
    expect(styles).toMatch(/\.desktop-top-row\.shared-launcher-header-row\s*\{[^}]*height:\s*var\(--chrome-row-height\)/su);
  });
});
