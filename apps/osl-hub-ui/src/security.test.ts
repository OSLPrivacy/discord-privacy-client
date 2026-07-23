import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("bundled preview security boundary", () => {
  it("contains no service-network or page-injection primitives", () => {
    const source = readRelative("./main.ts");

    expect(source).not.toMatch(/\bfetch\s*\(/);
    expect(source).not.toMatch(/\bXMLHttpRequest\b/);
    expect(source).not.toMatch(/\bWebSocket\b/);
    expect(source).not.toMatch(/\bsendBeacon\s*\(/);
    expect(source).not.toMatch(/\bwebhook\b/i);
    expect(source).not.toMatch(/<iframe\b/i);

    const serviceHost = readRelative("../../osl-hub/src/service_host.rs");
    expect(serviceHost).not.toMatch(/\.initialization_script/);
    expect(serviceHost).not.toMatch(/on_web_resource_request/);
    expect(serviceHost).not.toMatch(/enable_clipboard_access/);
    expect(serviceHost).not.toMatch(/cookies?_for_url|\.cookies?\s*\(/);
    expect(serviceHost).toContain("NewWindowResponse::Deny");
    expect(serviceHost).toContain(".on_download(|_, _| false)");
  });

  it("packages local assets without a development server", () => {
    const config = JSON.parse(readRelative("../../osl-hub/tauri.conf.json")) as {
      build: Record<string, unknown>;
      app: { security: { csp: string } };
    };

    expect(config.build.frontendDist).toBe("../osl-hub-ui/dist");
    expect(config.build).not.toHaveProperty("devUrl");
    expect(config.app.security.csp).toContain("connect-src ipc: http://ipc.localhost");
    expect(config.app.security.csp).not.toMatch(/connect-src[^;]*(?:https:|wss:|\*)/u);
    expect(config.app.security.csp).toContain("frame-src 'none'");

    const viteConfig = readRelative("../vite.config.ts");
    expect(viteConfig).toContain("modulePreload: false");
  });

  it("grants the WhatsApp QA UI only unlock, exact-host, visual-binding, and owner-window capabilities", () => {
    const capability = JSON.parse(readRelative("../../osl-hub/capabilities/hub.json")) as {
      local: boolean;
      webviews: string[];
      permissions: string[];
      remote?: unknown;
    };

    expect(capability.local).toBe(true);
    expect(capability.webviews).toEqual(["main"]);
    expect(capability).not.toHaveProperty("windows");
    expect(capability).not.toHaveProperty("remote");
    expect(capability.permissions).toEqual([
      "core:window:allow-close",
      "core:window:allow-minimize",
      "core:window:allow-start-dragging",
      "core:window:allow-toggle-maximize",
      "allow-get-core-readiness",
      "allow-unlock-hub-password-gate",
      "allow-claim-whatsapp-qa-window",
      "allow-resize-whatsapp-qa-window",
      "allow-get-whatsapp-qa-protection-status",
      "allow-begin-whatsapp-visual-binding",
      "allow-confirm-whatsapp-visual-binding",
    ]);
    expect(capability.permissions).not.toEqual(
      expect.arrayContaining([
        expect.stringMatching(/shell/i),
        expect.stringMatching(/http/i),
      ]),
    );

  });
});
