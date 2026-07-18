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

  it("grants the preview UI only local, main-window capabilities", () => {
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
      "allow-get-onboarding-preferences",
      "allow-list-hub-app-notifications",
      "allow-set-hub-notifications-enabled",
      "allow-set-hub-screenshot-protection",
      "allow-save-onboarding-preferences",
      "allow-scan-local-privacy",
      "allow-list-linked-services",
      "allow-get-core-readiness",
      "allow-list-core-features",
      "allow-get-hub-license-state",
      "allow-get-mass-cleanup-capabilities",
      "allow-discover-mass-cleanup-targets",
      "allow-execute-mass-cleanup-batch",
      "allow-validate-hub-activation-code",
      "allow-clear-hub-activation-code",
      "allow-unlock-hub-main-password",
      "allow-create-hub-osl-identity",
      "allow-import-hub-osl-identity-phrase",
      "allow-setup-hub-main-password",
      "allow-get-hub-password-role-status",
      "allow-set-hub-stealth-password",
      "allow-remove-hub-stealth-password",
      "allow-set-hub-burn-password",
      "allow-remove-hub-burn-password",
      "allow-check-hub-for-updates",
      "allow-install-hub-update",
      "allow-open-hub-releases-page",
      "allow-list-native-apps",
      "allow-install-native-app",
      "allow-list-browser-imports",
      "allow-open-browser-import",
      "allow-host-native-app-window",
      "allow-resize-native-app-window",
      "allow-focus-native-app-window",
      "allow-detach-native-app-window",
      "allow-create-service-account",
      "allow-open-service-host",
      "allow-close-service-host",
      "allow-set-local-protected-sheet-open",
      "allow-remove-service-account",
      "allow-activate-local-loopback-context",
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
      "allow-prepare-local-protected-text-with-policy",
      "allow-decrypt-local-protected-capsule",
      "allow-prepare-hub-attachment",
      "allow-open-hub-attachment",
      "allow-export-hub-friend-code",
      "allow-add-hub-friend",
      "allow-verify-hub-friend-safety-number",
      "allow-list-hub-people",
      "allow-set-hub-friend-nickname",
      "allow-set-active-hub-friend-permission",
      "allow-get-active-hub-context-security",
      "allow-set-active-hub-context-security",
      "allow-list-hub-identities",
      "allow-create-hub-identity-slot",
      "allow-recover-hub-identity-slot",
      "allow-switch-hub-identity",
      "allow-burn-active-hub-identity",
      "allow-execute-hub-full-cleanup",
      "allow-get-hub-service-burn-readiness",
      "allow-burn-hub-service-account",
      "allow-burn-active-hub-context",
    ]);
    expect(capability.permissions).not.toEqual(
      expect.arrayContaining([
        expect.stringMatching(/shell/i),
        expect.stringMatching(/http/i),
      ]),
    );

    const hubMain = readRelative("../../osl-hub/src/main.rs");
    const handler = hubMain.slice(hubMain.indexOf("tauri::generate_handler!["));
    for (const command of [
      "set_service_host_layout",
      "reset_service_account",
      "activate_hub_context",
      "clear_hub_context",
      "set_hub_friend_scope_permission",
      "set_hub_scope_security",
      "burn_hub_scope",
      "get_hub_password_readiness",
      "get_service_host_status",
      "prepare_local_protected_text",
      "get_hub_scope_security",
      "get_hub_full_cleanup_manifest",
    ]) {
      expect(handler).not.toContain(`${command},`);
    }
  });
});
