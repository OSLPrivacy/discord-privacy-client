import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const packageJson = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
) as { scripts?: Record<string, string> };
const viteConfig = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");

describe("Discord QA UI build entrypoint", () => {
  it("pins the QA renderer flag without changing the production build", () => {
    expect(packageJson.scripts?.build).toBe("tsc --noEmit && vite build");
    expect(packageJson.scripts?.["build:discord-qa"])
      .toBe("tsc --noEmit && vite build --mode discord-qa");
    expect(viteConfig).toContain('mode === "discord-qa"');
    expect(viteConfig).toContain(
      '"import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("1")',
    );
  });
});
