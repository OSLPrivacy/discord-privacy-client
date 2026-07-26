import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

function read(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf8");
}

describe("Discord disposable QA visual capture boundary", () => {
  it("exposes OSL pixels only in the compile-gated QA shell", () => {
    const main = read("../../osl-hub/src/main.rs");
    const overlay = read("../../osl-hub/src/native_discord_overlay.rs");

    expect(main).toContain('#[cfg(feature = "discord-qa-shell")]\nfn active_osl_capture_protection()');
    expect(main).toContain('#[cfg(not(feature = "discord-qa-shell"))]\nfn active_osl_capture_protection()');
    expect(main).toMatch(/cfg\(feature = "discord-qa-shell"\)[\s\S]*?ScreenshotProtection::Off/u);
    expect(main).toMatch(/cfg\(not\(feature = "discord-qa-shell"\)\)[\s\S]*?ScreenshotProtection::On/u);

    expect(overlay).toContain('#[cfg(feature = "discord-qa-shell")]\nfn active_overlay_capture_protection()');
    expect(overlay).toContain('#[cfg(not(feature = "discord-qa-shell"))]\nfn active_overlay_capture_protection()');
    expect(overlay).toMatch(/cfg\(feature = "discord-qa-shell"\)[\s\S]*?ScreenshotProtection::Off/u);
    expect(overlay).toMatch(/cfg\(not\(feature = "discord-qa-shell"\)\)[\s\S]*?ScreenshotProtection::On/u);
  });
});
