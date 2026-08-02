import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";

describe("T21-T46 Enclaves surface ownership", () => {
  it("ships through the extracted Enclaves route without importing main.ts", () => {
    const source = readFileSync(new URL("./osl-enclaves.ts", import.meta.url), "utf8");
    const serversView = readFileSync(new URL("./osl-servers-view.ts", import.meta.url), "utf8");
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

    expect(source).not.toMatch(/from\s+["']\.\/main["']/u);
    expect(serversView).toContain('from "./osl-enclaves"');
    expect(serversView).toContain("oslEnclavesSurfaceMarkup");
    expect(main).toContain('import { oslServersViewMarkup } from "./osl-servers-view"');
    expect(main).not.toMatch(/from\s+["']\.\/osl-enclaves["']/u);
  });

  it("renders the reachable first-party Enclaves surface with stylesheet classes only", () => {
    const markup = oslEnclavesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>` });

    expect(markup).toContain(">OSL Enclaves</h1>");
    expect(markup).toContain("encrypted communities");
    expect(markup).toContain("<span>Available</span>");
    expect(markup).not.toContain("style=");
  });
});
