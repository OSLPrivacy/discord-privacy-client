import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

function read(relative: string): string {
  return readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf8");
}

function cfgFunctionBody(source: string, cfg: string, name: string): string {
  const marker = `${cfg}\nfn ${name}()`;
  const start = source.indexOf(marker);
  expect(start, `missing ${name} under ${cfg}`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf("{", start);
  expect(bodyStart, `missing ${name} body`).toBeGreaterThan(start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") depth += 1;
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(bodyStart + 1, index);
    }
  }
  throw new Error(`unterminated ${name} body`);
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

  it("production_binary_observability_check_on_a_non_discord_qa_shell_build", () => {
    const main = read("../../osl-hub/src/main.rs");
    const overlay = read("../../osl-hub/src/native_discord_overlay.rs");

    for (const [source, name] of [
      [main, "active_osl_capture_protection"],
      [overlay, "active_overlay_capture_protection"],
    ] as const) {
      const qaBody = cfgFunctionBody(source, '#[cfg(feature = "discord-qa-shell")]', name);
      const productionBody = cfgFunctionBody(source, '#[cfg(not(feature = "discord-qa-shell"))]', name);

      expect(qaBody).toContain("ScreenshotProtection::Off");
      expect(qaBody).not.toContain("ScreenshotProtection::On");
      expect(productionBody).toContain("ScreenshotProtection::On");
      expect(productionBody).not.toContain("ScreenshotProtection::Off");
    }

    expect(main).toContain("active_osl_capture_protection()");
    expect(overlay).toContain("active_overlay_capture_protection()");
  });
});
