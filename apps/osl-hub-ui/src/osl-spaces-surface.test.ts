import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { oslSpacesSurfaceMarkup } from "./osl-spaces";

describe("T21-T46 Spaces surface ownership", () => {
  it("ships through the extracted Enclaves route without importing main.ts", () => {
    const source = readFileSync(new URL("./osl-spaces.ts", import.meta.url), "utf8");
    const serversView = readFileSync(new URL("./osl-servers-view.ts", import.meta.url), "utf8");
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

    expect(source).not.toMatch(/from\s+["']\.\/main["']/u);
    expect(serversView).toContain('from "./osl-spaces"');
    expect(serversView).toContain("oslSpacesSurfaceMarkup");
    expect(main).toContain('import { oslServersViewMarkup } from "./osl-servers-view"');
    expect(main).not.toMatch(/from\s+["']\.\/osl-spaces["']/u);
  });

  it("renders the reachable first-party Spaces surface with stylesheet classes only", () => {
    const markup = oslSpacesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>` });

    expect(markup).toContain(">OSL Enclaves</h1>");
    expect(markup).toContain("encrypted shared spaces");
    expect(markup).toContain("<span>Available</span>");
    expect(markup).not.toContain("style=");
  });
});
