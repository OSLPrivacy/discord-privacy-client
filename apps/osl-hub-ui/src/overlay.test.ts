import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { boundedProtectedDraft, utf8Length } from "./overlay-state";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("trusted composer overlay", () => {
  it("bounds drafts by UTF-8 bytes without splitting Unicode scalars", () => {
    expect(utf8Length(boundedProtectedDraft("🙂".repeat(400)))).toBe(1_000);
    expect(boundedProtectedDraft(`safe\0draft\u007f`)).toBe("safedraft");
    expect(boundedProtectedDraft("x".repeat(1_001))).toHaveLength(1_000);
  });

  it("has a separate zero-authority local capability", () => {
    const overlay = JSON.parse(readRelative("../../osl-hub/capabilities/composer-overlay.json")) as {
      local: boolean;
      webviews: string[];
      permissions: unknown[];
      remote?: unknown;
      windows?: unknown;
    };
    const hub = JSON.parse(readRelative("../../osl-hub/capabilities/hub.json")) as {
      local: boolean;
      webviews: string[];
      permissions: string[];
      remote?: unknown;
    };
    expect(overlay.local).toBe(true);
    expect(overlay.webviews).toEqual(["composer-overlay"]);
    expect(overlay.permissions).toEqual([]);
    expect(overlay).not.toHaveProperty("remote");
    expect(overlay).not.toHaveProperty("windows");
    expect(hub.local).toBe(true);
    expect(hub.webviews).toEqual(["main"]);
    expect(hub.permissions).toEqual(expect.arrayContaining([
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
    ]));
    expect(hub).not.toHaveProperty("remote");
    expect(overlay.permissions).not.toEqual(expect.arrayContaining([
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
    ]));
  });

  it("ships as a dedicated local entry with no networking, storage, or IPC", () => {
    const vite = readRelative("../vite.config.ts");
    const source = readRelative("./overlay.ts");
    const native = readRelative("../../osl-hub/src/service_host.rs");
    expect(vite).toContain('overlay: fileURLToPath(new URL("./overlay.html"');
    expect(source).not.toMatch(/\binvoke\s*\(/);
    expect(source).not.toMatch(/\bfetch\s*\(/);
    expect(source).not.toMatch(/localStorage|sessionStorage|indexedDB/);
    expect(native).toContain('const OVERLAY_WEBVIEW_LABEL: &str = "composer-overlay"');
    expect(native).toContain("WebviewUrl::App(PathBuf::from(OVERLAY_ASSET))");
    expect(native).toContain(".transparent(true)");
    expect(native).toContain(".set_bounds(");
    expect(native).toContain("ensure_hidden_overlay(&app, &main_window)");
    expect(native).not.toContain("overlay.show()");
  });

  it("looks like a minimal composer but keeps an unmistakable OSL trust mark", () => {
    const html = readRelative("../overlay.html");
    const css = readRelative("./overlay.css");
    expect(html).toContain('class="trust-mark"');
    expect(html).toContain("This field belongs to OSL");
    expect(html).not.toContain("Manual handoff");
    expect(html).not.toContain("protected draft");
    expect(css).toContain("grid-template-columns: auto minmax(0, 1fr) auto");
    expect(css).toContain("border-radius: 0");
    expect(css).not.toMatch(/box-shadow:\s*0 12px 34px/);
  });
});
