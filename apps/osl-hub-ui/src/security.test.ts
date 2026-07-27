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
      "core:window:allow-is-fullscreen",
      "core:window:allow-minimize",
      "core:window:allow-set-fullscreen",
      "core:window:allow-start-dragging",
      "core:window:allow-toggle-maximize",
      // The main window now emits window-alignment/visibility events to (and
      // listens for events from) its own first-party local overlay window
      // (native-discord-overlay, see capabilities/native-discord-overlay.json).
      // That target window is itself capability-scoped and local-only, so this
      // does not add remote or cross-app reach — it stays inside the same
      // local, main-window boundary this test protects.
      "core:event:allow-emit-to",
      "core:event:allow-listen",
      "allow-get-onboarding-preferences",
      "allow-list-hub-app-notifications",
      "allow-set-hub-notifications-enabled",
      "allow-set-hub-screenshot-protection",
      "allow-copy-hub-friend-invite",
      "allow-save-onboarding-preferences",
      "allow-scan-local-privacy",
      "allow-initialize-scrub-index",
      "allow-append-scrub-index-chunk",
      "allow-get-scrub-index-status",
      "allow-pause-scrub-index",
      "allow-resume-scrub-index",
      "allow-cancel-scrub-index",
      "allow-list-linked-services",
      "allow-get-core-readiness",
      "allow-list-core-features",
      "allow-get-hub-license-state",
      "allow-get-mass-cleanup-capabilities",
      "allow-discover-mass-cleanup-targets",
      "allow-execute-mass-cleanup-batch",
      "allow-validate-hub-activation-code",
      "allow-clear-hub-activation-code",
      "allow-unlock-hub-password-gate",
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
      // Read-only consent/marker probes. Both return booleans from local native
      // state and perform no takeover or cross-app write.
      "allow-native-app-takeover-requires-consent",
      "allow-discord-marker-available",
      "allow-install-native-app",
      "allow-get-mullvad-status",
      "allow-install-mullvad",
      "allow-open-mullvad",
      "allow-host-mullvad-window",
      "allow-resize-mullvad-window",
      "allow-focus-mullvad-window",
      "allow-restore-mullvad-window",
      "allow-list-browser-imports",
      "allow-open-browser-import",
      "allow-get-firefox-status",
      "allow-install-firefox",
      "allow-begin-browser-account-import",
      "allow-begin-protected-browser-import",
      "allow-finish-protected-browser-import",
      "allow-launch-firefox-service",
      "allow-get-default-browser-companion-status",
      "allow-host-default-browser-companion",
      "allow-resize-default-browser-companion",
      "allow-focus-default-browser-companion",
      "allow-detach-default-browser-companion",
      "allow-host-native-app-window",
      "allow-resize-native-app-window",
      "allow-focus-native-app-window",
      "allow-detach-native-app-window",
      "allow-activate-native-manual-peer-context",
      "allow-activate-osl-chat-context",
      "allow-close-osl-chat-context",
      // Discord QA-shell headless-testing hooks. Their Rust command handlers
      // are compiled only under the `discord-qa-shell` Cargo feature, which is
      // NOT in the default feature set (see apps/osl-hub/Cargo.toml) — the
      // commands do not exist in production binaries, so granting the
      // permission here is inert outside QA builds. They stay local/main-
      // window scoped either way.
      "allow-send-native-discord-qa-probe",
      "allow-run-native-discord-headless-qa",
      "allow-poll-native-discord-headless-qa",
      "allow-prepare-osl-chat-text",
      "allow-open-osl-chat-text",
      "allow-list-osl-chat-history",
      "allow-select-osl-chat-attachment",
      "allow-list-osl-chat-attachments",
      "allow-open-osl-chat-attachment",
      "allow-set-native-discord-protected-overlay-open",
      "allow-set-native-discord-covertext-enabled",
      "allow-create-service-account",
      "allow-open-service-host",
      "allow-close-service-host",
      "allow-set-local-protected-sheet-open",
      "allow-remove-service-account",
      "allow-activate-local-loopback-context",
      "allow-activate-manual-peer-context",
      "allow-prepare-peer-prose-text",
      "allow-open-peer-prose-text",
      "allow-prepare-encrypted-text",
      "allow-decrypt-hub-capsule",
      "allow-prepare-local-protected-text-with-policy",
      "allow-decrypt-local-protected-capsule",
      "allow-prepare-hub-attachment",
      "allow-open-hub-attachment",
      "allow-export-hub-friend-code",
      "allow-add-hub-friend",
      "allow-verify-hub-friend-safety-number",
      // Friend removal is hub-local only: the Rust `remove_hub_friend` command
      // withdraws local approvals/policy and queues revocation notices. It adds
      // no remote or cross-app reach, so it stays inside the local, main-window
      // boundary this test protects.
      "allow-remove-hub-friend",
      "allow-list-hub-people",
      "allow-set-hub-friend-nickname",
      "allow-set-active-hub-friend-permission",
      "allow-set-active-hub-friend-reach",
      "allow-revoke-active-hub-friend-scope",
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
    const permissions = readRelative("../../osl-hub/permissions/hub.toml");
    for (const [permission, command] of [
      ["allow-get-firefox-status", "get_firefox_status"],
      ["allow-install-firefox", "install_firefox"],
      ["allow-launch-firefox-service", "launch_firefox_service"],
      ["allow-get-default-browser-companion-status", "get_default_browser_companion_status"],
      ["allow-host-default-browser-companion", "host_default_browser_companion"],
      ["allow-resize-default-browser-companion", "resize_default_browser_companion"],
      ["allow-focus-default-browser-companion", "focus_default_browser_companion"],
      ["allow-detach-default-browser-companion", "detach_default_browser_companion"],
      ["allow-copy-hub-friend-invite", "copy_hub_friend_invite"],
      ["allow-discord-marker-available", "discord_marker_available"],
    ] as const) {
      expect(handler).toContain(`${command},`);
      expect(permissions).toContain(`identifier = "${permission}"`);
      expect(permissions).toContain(`commands.allow = ["${command}"]`);
      expect(capability.permissions).toContain(permission);
    }
    // Negative control: the status DTO field with the same name is not a Tauri
    // command. The exact annotated function must exist as well as handler wiring.
    expect(hubMain).toContain("#[tauri::command]\nfn discord_marker_available(");
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

  it("does not turn a stuck transcript read into an automatic retry storm", () => {
    const source = readRelative("./overlay.ts");
    const run = source.slice(
      source.indexOf("async function runTranscriptRehydrate"),
      source.indexOf("function transcriptTimestamp"),
    );
    const watchdog = run.slice(
      run.indexOf("const watchdog = window.setTimeout"),
      run.indexOf("const result = await"),
    );
    const schedule = source.slice(
      source.indexOf("function scheduleTranscriptRehydrate"),
      source.indexOf("async function runTranscriptRehydrate"),
    );

    // Timing out releases the one-read guard and may replace the read once.
    // The latch is spent before scheduling, so a second never-settling invoke
    // cannot replace itself and create the old 6.8-second accumulation loop.
    expect(watchdog).toContain("rehydrateBusy = false;");
    expect(watchdog).toContain("const externalRetryPending = rehydratePending;");
    expect(watchdog).toContain(
      "&& (externalRetryPending || !rehydrateWatchdogReplacementUsed)",
    );
    expect(watchdog.indexOf("rehydrateWatchdogReplacementUsed = true;")).toBeLessThan(
      watchdog.indexOf("scheduleTranscriptRehydrate();"),
    );
    expect(watchdog.match(/scheduleTranscriptRehydrate\(\);/gu)).toHaveLength(1);
    expect(schedule).not.toContain("rehydrateWatchdogReplacementUsed = false;");

    // A later real UI edge can still retry through the ordinary coalescer.
    expect(schedule).toContain("void runTranscriptRehydrate();");
    expect(source).toContain(
      'window.addEventListener("wheel", () => scheduleTranscriptRehydrate(),',
    );
  });
});
