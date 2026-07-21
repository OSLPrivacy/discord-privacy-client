import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const uiRoot = fileURLToPath(new URL("..", import.meta.url));
const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const shieldHtml = readFileSync(`${uiRoot}/shield.html`, "utf8");
const viteSource = readFileSync(`${uiRoot}/vite.config.ts`, "utf8");
const nativeSource = readFileSync(`${repoRoot}/apps/osl-hub/src/native_discord_overlay.rs`, "utf8");
const capability = JSON.parse(
  readFileSync(`${repoRoot}/apps/osl-hub/capabilities/native-discord-shield.json`, "utf8"),
) as { local?: boolean; webviews?: string[]; permissions?: string[] };

describe("native Discord capture shield", () => {
  it("is a bundled opaque black document with no script or remote content", () => {
    expect(shieldHtml).toContain("background: #000");
    expect(shieldHtml).not.toMatch(/<script\b/i);
    expect(shieldHtml).not.toMatch(/https?:\/\//i);
    expect(viteSource).toContain('shield: fileURLToPath(new URL("./shield.html", import.meta.url))');
  });

  it("has no IPC permissions and is scoped to only the shield webview", () => {
    expect(capability.local).toBe(true);
    expect(capability.webviews).toEqual(["native-discord-shield"]);
    expect(capability.permissions).toEqual([]);
  });

  it("cannot focus, open content, download, or outrank the protected overlay", () => {
    expect(nativeSource).toContain(".focusable(false)");
    expect(nativeSource).toContain(".skip_taskbar(true)");
    expect(nativeSource).toContain(".devtools(false)");
    expect(nativeSource).toContain("NewWindowResponse::Deny");
    expect(nativeSource).toContain(".on_download(|_, _| false)");
    expect(nativeSource).toContain("ensure_shield_stack(&window, &shield)");
    expect(nativeSource).toContain("BeginDeferWindowPos(2)");
    expect(nativeSource).toContain("clear_and_hide(&app)");
  });
});
