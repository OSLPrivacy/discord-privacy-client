import { existsSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

const orphanModules = [
  "osl_notes.rs",
  "osl_assets.rs",
  "osl_lan.rs",
  "osl_formats.rs",
  "osl_collab.rs",
  "osl_plugins.rs",
] as const;

describe("Notes/Creative orphan-feature record", () => {
  it("records every present but undeclared module as untracked", () => {
    const document = readFileSync(
      new URL("../../../docs/design/untracked-orphan-modules.md", import.meta.url),
      "utf8",
    );
    const rustLib = readFileSync(new URL("../../osl-hub/src/lib.rs", import.meta.url), "utf8");

    expect(document).toContain("# Notes/Creative orphan cluster — track assignment required");
    expect(document).toContain("**untracked feature candidate**");
    expect(document).toContain("Owner decision D40");

    for (const module of orphanModules) {
      const modulePath = new URL(`../../osl-hub/src/${module}`, import.meta.url);
      expect(existsSync(modulePath)).toBe(true);
      expect(statSync(modulePath).size).toBeGreaterThan(0);
      expect(rustLib).not.toMatch(new RegExp(`\\bpub mod ${module.replace(".rs", "")}\\s*;`, "u"));
      expect(document).toContain(`\`${module}\``);
    }
  });
});
