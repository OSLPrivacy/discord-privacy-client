import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function readProductionRustTree(relativeRoot: string): string {
  const excludedDirectories = new Set(["fixtures", "testdata", "tests"]);
  const sources: string[] = [];
  const visit = (directory: string): void => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (!excludedDirectories.has(entry.name)) {
          visit(join(directory, entry.name));
        }
      } else if (entry.isFile() && entry.name.endsWith(".rs")) {
        sources.push(
          rustProductionPrefix(readFileSync(join(directory, entry.name), "utf8")),
        );
      }
    }
  };
  visit(fileURLToPath(new URL(relativeRoot, import.meta.url)));
  return sources.join("\n");
}

type MessagingProductionFacts = {
  registeredPrepareCommand: boolean;
  mainCallsBroker: boolean;
  brokerCallsIpc: boolean;
  statelessV3Fallback: boolean;
  dmRatchetEnabled: boolean;
  groupSenderKeysEnabled: boolean;
  prekeyLifecycleCalled: boolean;
};

function rustWithoutComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//gu, "")
    .replace(/\/\/[^\n]*/gu, "");
}

function rustProductionPrefix(source: string): string {
  const testModule = source.search(/#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+tests\s*\{/u);
  const production = testModule < 0 ? source : source.slice(0, testModule);
  return rustWithoutComments(production);
}

function tauriCommandSurface(source: string): string {
  const macroStart = source.indexOf("macro_rules! hub_tauri_commands");
  if (macroStart >= 0) {
    // The authoritative command list ends where the next macro definition
    // begins. Two spellings exist because the surface was split across files:
    // main.rs follows the list with `hub_tauri_generate_handler` (the callback
    // that expands it into a real `tauri::generate_handler!`), while
    // hub_command_surface.rs follows it with the test-only
    // `hub_tauri_command_names` counterpart.
    const macroEnd = [
      "macro_rules! hub_tauri_generate_handler",
      "macro_rules! hub_tauri_command_names",
    ]
      .map((terminator) => source.indexOf(terminator, macroStart + 1))
      .filter((index) => index >= 0)
      .sort((a, b) => a - b)[0];
    if (macroEnd !== undefined) return source.slice(macroStart, macroEnd);
  }
  return source.slice(source.indexOf("tauri::generate_handler!["));
}

// Every `invoke_handler(tauri::generate_handler![...])` list written out
// literally in a source file. main.rs still registers one such list directly
// (the signal-qa shell binary); the product binary registers through the
// `hub_tauri_commands!` macro instead.
function literalInvokeHandlerLists(source: string): string {
  const marker = "invoke_handler(tauri::generate_handler![";
  const lists: string[] = [];
  for (
    let cursor = source.indexOf(marker);
    cursor >= 0;
    cursor = source.indexOf(marker, cursor + 1)
  ) {
    const end = source.indexOf("]);", cursor + marker.length);
    if (end < 0) continue;
    lists.push(source.slice(cursor + marker.length, end));
  }
  return lists.join("\n");
}

// The registered Tauri command surface of the Hub binary, read from BOTH files
// that own a piece of it.
//
// A refactor moved the single authoritative command list out of
// apps/osl-hub/src/main.rs into the library module
// apps/osl-hub/src/hub_command_surface.rs, which now defines
// `macro_rules! hub_tauri_commands`. The reason is stated in that module's
// header: main.rs is a `[[bin]]` with `required-features = ["desktop"]` that CI
// cannot compile at all, so nothing inside it is ever proven; the library is.
// main.rs kept only the `#[tauri::command]` wrappers plus the
// `hub_tauri_commands!(hub_tauri_generate_handler)` invocation that turns the
// list into the real `invoke_handler`, and one literal handler list for the
// signal-qa shell build.
//
// So any assertion about whether a command is *registered* (and therefore about
// which ACL entry must exist for it) has to search the combination: reading
// main.rs alone now yields the macro's definition text, not command names, which
// would silently make both the positive and the negative registration proofs
// vacuous. Assertions about code that genuinely lives in main.rs (the
// `#[tauri::command]` wrappers, the gate/burn call sites) still read main.rs.
function hubCommandSurface(): string {
  const surfaceModule = readRelative("../../osl-hub/src/hub_command_surface.rs");
  const main = readRelative("../../osl-hub/src/main.rs");
  return [
    tauriCommandSurface(rustWithoutComments(surfaceModule)),
    literalInvokeHandlerLists(rustWithoutComments(main)),
  ].join("\n");
}

function classifyMessagingProductionPath(
  main: string,
  broker: string,
  commands: string,
  state: string,
  extraProductionRust = "",
  // Injectable so the stage-removal proofs below can starve registration:
  // the command list no longer lives in main.rs, so mutating `main` can no
  // longer make `registeredPrepareCommand` false. Mutate this instead.
  commandSurface: string = hubCommandSurface(),
): MessagingProductionFacts {
  const mainProduction = rustWithoutComments(main);
  const brokerProduction = rustProductionPrefix(broker);
  const commandsProduction = rustProductionPrefix(commands);
  const stateProduction = rustProductionPrefix(state);
  const dispatcherStart = commandsProduction.indexOf(
    "pub fn cmd_osl_encrypt_message_v2_wire(",
  );
  const dispatcherEnd = commandsProduction.indexOf(
    "\nfn scope_is_group_or_server(",
    dispatcherStart,
  );
  const dispatcher =
    dispatcherStart < 0 || dispatcherEnd < 0
      ? ""
      : commandsProduction.slice(dispatcherStart, dispatcherEnd);
  const allProduction = [
    mainProduction,
    brokerProduction,
    commandsProduction,
    stateProduction,
    rustProductionPrefix(extraProductionRust),
  ].join("\n");
  const messagingPath = [
    mainProduction,
    brokerProduction,
    dispatcher,
    stateProduction,
  ].join("\n");
  const extraProduction = rustProductionPrefix(extraProductionRust);
  const syntheticExtra =
    extraProduction.length > 0
    && !extraProduction.includes("pub fn cmd_osl_encrypt_message_v2_wire(")
    && !extraProduction.includes("pub fn run_prekey_replenishment_tick(");
  const handler = commandSurface;

  return {
    registeredPrepareCommand: handler.includes("prepare_encrypted_text,"),
    mainCallsBroker: mainProduction.includes("broker::prepare_encrypted_text("),
    brokerCallsIpc: brokerProduction.includes("ipc::commands::cmd_osl_encrypt_message_v2("),
    statelessV3Fallback: dispatcher.includes("crate::wire_v2::encrypt_v3("),
    dmRatchetEnabled:
      /let\s+v4_dm_enabled\s*=\s*true\s*;/u.test(dispatcher) ||
      /\bencrypt_v4_send\s*\(/u.test(dispatcher),
    groupSenderKeysEnabled:
      /sender_keys_enabled\s*\.\s*store\s*\(\s*true\b/u.test(allProduction),
    prekeyLifecycleCalled:
      /\.(?:fetch_prekey_bundle|replenish_prekeys|replenish_using_state)\s*\(/u.test(
        messagingPath,
      )
      || (syntheticExtra
        && /\.(?:fetch_prekey_bundle|replenish_prekeys|replenish_using_state)\s*\(/u.test(
          extraProduction,
        )),
  };
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
    const expectedPermissions = [
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
      "allow-set-scrub-index-manifest",
      "allow-get-scrub-index-manifest",
      "allow-get-scrub-index-scan",
      "allow-append-scrub-index-chunk",
      "allow-get-scrub-index-status",
      "allow-pause-scrub-index",
      "allow-resume-scrub-index",
      "allow-cancel-scrub-index",
      "allow-get-autoscrub-run-fl",
      "allow-start-autoscrub-reviewed-run",
      "allow-request-autoscrub-global-stop",
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
      "allow-open-hub-source-repository",
      "allow-list-native-apps",
      "allow-native-app-takeover-requires-consent",
      "allow-discord-marker-available",
      "allow-install-native-app",
      "allow-claim-whatsapp-qa-window",
      "allow-resize-whatsapp-qa-window",
      "allow-get-whatsapp-qa-protection-status",
      "allow-begin-whatsapp-visual-binding",
      "allow-confirm-whatsapp-visual-binding",
      "allow-open-whatsapp-qa-protected-text",
      "allow-get-mullvad-status",
      "allow-install-mullvad",
      "allow-open-mullvad",
      "allow-host-mullvad-window",
      "allow-resize-mullvad-window",
      "allow-focus-mullvad-window",
      "allow-restore-mullvad-window",
      "allow-list-browser-imports",
      "allow-open-browser-import",
      "allow-list-browser-profiles-for-consent",
      "allow-grant-browser-profile-consent",
      "allow-scan-consented-browser-profile",
      "allow-load-detected-browser-footprint",
      "allow-revoke-detected-browser-footprint",
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
      "allow-get-signal-protected-send-readiness",
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
      "allow-request-native-discord-visible-row-qa-receipt",
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
      "allow-open-hosted-session-scan",
      "allow-request-hosted-session-scan",
      "allow-request-hosted-session-scan-command",
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
      // Local, read-only username lookup used by the add-friend-by-username
      // flow (adapters.ts `get_hub_username_status`). The Rust handler
      // (apps/osl-hub/src/main.rs) requires an unlocked active identity,
      // validates the username, and resolves it through the in-process
      // `lookup_username` helper — there is no network, keyserver, or
      // cross-app reach, so it stays inside the local main-window boundary
      // this test protects. The write-side siblings (`claim_hub_username`,
      // `add_hub_friend_by_username`) are deliberately NOT granted here.
      "allow-get-hub-username-status",
      "allow-verify-hub-friend-safety-number",
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
    ];
    expect(new Set(capability.permissions)).toEqual(new Set(expectedPermissions));
    expect(capability.permissions).toHaveLength(expectedPermissions.length);
    expect(capability.permissions).toHaveLength(new Set(capability.permissions).size);
    expect(capability.permissions).not.toEqual(
      expect.arrayContaining([
        expect.stringMatching(/shell/i),
        expect.stringMatching(/http/i),
      ]),
    );

    const hubMain = readRelative("../../osl-hub/src/main.rs");
    const handler = hubCommandSurface();
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
      ["allow-native-app-takeover-requires-consent", "native_app_takeover_requires_consent"],
      ["allow-discord-marker-available", "discord_marker_available"],
      ["allow-claim-whatsapp-qa-window", "claim_whatsapp_qa_window"],
      ["allow-resize-whatsapp-qa-window", "resize_whatsapp_qa_window"],
      ["allow-get-whatsapp-qa-protection-status", "get_whatsapp_qa_protection_status"],
      ["allow-begin-whatsapp-visual-binding", "begin_whatsapp_visual_binding"],
      ["allow-confirm-whatsapp-visual-binding", "confirm_whatsapp_visual_binding"],
      ["allow-open-whatsapp-qa-protected-text", "open_whatsapp_qa_protected_text"],
      ["allow-remove-hub-friend", "remove_hub_friend"],
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

  it("keeps every QA browser-profile verb explicitly fail-closed", () => {
    const source = readRelative("../../osl-hub/src/main.rs");
    const criteria = source.slice(
      source.indexOf("fn insert_verb_criteria("),
      source.indexOf("fn write_verdict("),
    );
    const driver = source.slice(
      source.indexOf("fn run_receive_side_verb("),
      source.indexOf("fn refused_verdict("),
    );
    const arm = (body: string, verb: string): string => {
      const start = body.indexOf(`Verb::${verb} => {`);
      expect(start).toBeGreaterThanOrEqual(0);
      const rest = body.slice(start);
      const next = rest.indexOf("\n            Verb::", 1);
      return next < 0 ? rest : rest.slice(0, next);
    };

    for (const [verb, criterion, refusal] of [
      [
        "ListBrowserProfiles",
        "browser_profile_list_driven",
        "LIST_BROWSER_PROFILES_UNAVAILABLE",
      ],
      [
        "GrantBrowserProfile",
        "browser_profile_grant_driven",
        "GRANT_BROWSER_PROFILE_UNAVAILABLE",
      ],
      [
        "RevokeBrowserProfile",
        "browser_profile_revoke_driven",
        "REVOKE_BROWSER_PROFILE_UNAVAILABLE",
      ],
      [
        "RunBrowserImport",
        "browser_profile_import_driven",
        "RUN_BROWSER_IMPORT_UNAVAILABLE",
      ],
    ] as const) {
      const criterionArm = arm(criteria, verb);
      expect(criterionArm).toContain(`"${criterion}"`);
      expect(criterionArm).toMatch(/\n\s+false,/);
      expect(criterionArm).toContain(`Some(${refusal}.to_owned())`);

      const driverArm = arm(driver, verb);
      expect(driverArm).toContain('outcome = "refused";');
      expect(driverArm).toContain(`outcome_detail.refusal = Some(${refusal});`);
    }

    // The compiler guards enum exhaustiveness. These negative controls guard
    // against weakening that property locally while keeping the build green.
    expect(criteria).not.toMatch(/\n\s*_\s*=>/);
    expect(driver).not.toMatch(/\n\s*_\s*=>/);
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

  it("keeps ratchet, sender-key, and prekey claims bound to the shipping path", () => {
    const main = readRelative("../../osl-hub/src/main.rs");
    const broker = readRelative("../../osl-hub/src/broker.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const wire = readRelative("../../../crates/ipc/src/wire_v2.rs");
    const prekeyClient = readRelative("../../../crates/keystore/src/client.rs");
    const ipcPublicDocs = readRelative("../../../crates/ipc/src/lib.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
      readProductionRustTree("../../../src-tauri/src/"),
    ].join("\n");
    const facts = classifyMessagingProductionPath(
      main,
      broker,
      commands,
      state,
      productionRust,
    );

    // Positive production controls keep an empty/deleted command surface from
    // making the negative reachability assertions pass vacuously.
    expect(facts.registeredPrepareCommand).toBe(true);
    expect(facts.mainCallsBroker).toBe(true);
    expect(facts.brokerCallsIpc).toBe(true);
    expect(facts.statelessV3Fallback).toBe(true);
    expect(wire).toContain("recipient_ik plays both ik and spk");
    expect(wire).toContain("None,\n            &recip.mlkem_pub,");
    expect(prekeyClient).toContain("pub fn fetch_prekey_bundle(");
    expect(prekeyClient).toContain("pub fn replenish_prekeys(");

    // Implemented prototype code is not a shipping guarantee until each
    // production gate/caller exists. Initial prekey publish is live, while
    // peer-bundle fetch and receive-side consumption remain outside messaging.
    expect(facts.dmRatchetEnabled).toBe(false);
    expect(facts.groupSenderKeysEnabled).toBe(false);
    expect(facts.prekeyLifecycleCalled).toBe(true);

    // The owned crate's public documentation must retain the reachable v3 and
    // implemented-but-disabled distinction. Root README truth is separately
    // owned and tracked by the crypto-lane report.
    const assertOwnedPublicTruth = (source: string): void => {
      expect(source).toContain("Current production messaging posture:");
      expect(source).toContain("Stateless wire-v3 recipient wrapping");
      expect(source).toContain(
        "production disables the v4 DM branch and defaults the",
      );
      expect(source).toContain(
        "implementation inventory, not current",
      );
      expect(source).toContain(
        "Identity registration publishes the initial prekey batch",
      );
      expect(source).toContain(
        "messaging\n//!   send path still does not fetch peer prekey bundles",
      );
    };
    assertOwnedPublicTruth(ipcPublicDocs);
    expect(() =>
      assertOwnedPublicTruth(
        ipcPublicDocs.replace(
          "production disables the v4 DM branch and defaults the",
          "production enables the v4 DM branch and defaults the",
        ),
      ),
    ).toThrow();

    // One failure-capable mutation per reachability stage, plus positive
    // mutations proving all three dormant-subsystem detectors can turn on.
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands,
        state,
        "",
        // Registration now lives in hub_command_surface.rs, so that is the text
        // this stage-removal mutation has to starve.
        hubCommandSurface().replace("prepare_encrypted_text,", ""),
      ).registeredPrepareCommand,
    ).toBe(false);
    expect(
      classifyMessagingProductionPath(
        main.replace("broker::prepare_encrypted_text(", "broker::prepare_encrypted_text_removed("),
        broker,
        commands,
        state,
      ).mainCallsBroker,
    ).toBe(false);
    expect(
      classifyMessagingProductionPath(
        main,
        broker.replace(
          "ipc::commands::cmd_osl_encrypt_message_v2(",
          "ipc::commands::cmd_osl_encrypt_message_v2_removed(",
        ),
        commands,
        state,
      ).brokerCallsIpc,
    ).toBe(false);
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands.replaceAll(
          "crate::wire_v2::encrypt_v3(",
          "crate::wire_v2::encrypt_v3_removed(",
        ),
        state,
      ).statelessV3Fallback,
    ).toBe(false);
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands.replace(
          "RatchetPolicyDecision::LegacyV3 | RatchetPolicyDecision::LegacyV4Dm => {}",
          "RatchetPolicyDecision::LegacyV4Dm => { encrypt_v4_send(); }\n        RatchetPolicyDecision::LegacyV3 => {}",
        ),
        state,
      ).dmRatchetEnabled,
    ).toBe(true);
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands,
        state,
        "state.sender_keys_enabled.store(true, Ordering::Release);",
      ).groupSenderKeysEnabled,
    ).toBe(true);
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands,
        state,
        "client.replenish_prekeys(&state).await?;",
      ).prekeyLifecycleCalled,
    ).toBe(true);

    // Comments and cfg(test)-only fixtures do not add production authority; the
    // live initial publish/replenish path is the baseline either way.
    expect(
      classifyMessagingProductionPath(
        main,
        broker,
        commands,
        state,
        [
          "// client.fetch_prekey_bundle(\"peer\").await?;",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() { client.replenish_using_state(&state); }",
          "}",
        ].join("\n"),
      ).prekeyLifecycleCalled,
    ).toBe(true);
  });

  it("keeps signed burn alerts classified as implemented-unwired", () => {
    const burnAlert = readRelative("../../../crates/keystore/src/burn_alert.rs");
    const keystoreLib = readRelative("../../../crates/keystore/src/lib.rs");
    const keystoreClient = readRelative("../../../crates/keystore/src/client.rs");
    const broker = readRelative("../../osl-hub/src/broker.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
      keystoreClient,
    ].join("\n");
    const productionReferencesBurnAlert = (source: string): boolean =>
      /\b(?:BurnAlertPayload|sign_burn_alert|verify_burn_alert)\b/u.test(
        rustProductionPrefix(source),
      );

    // Positive implementation controls: the prototype and public re-export
    // must exist, so deleting the subsystem cannot make zero reachability pass.
    expect(burnAlert).toContain("pub struct BurnAlertPayload");
    expect(burnAlert).toContain("pub fn sign_burn_alert(");
    expect(burnAlert).toContain("pub fn verify_burn_alert(");
    expect(keystoreLib).toContain(
      "pub use burn_alert::{sign_burn_alert, verify_burn_alert, BurnAlertPayload",
    );

    // Positive control for the distinct reachable revocation implementation:
    // its presence cannot be used to imply this signature prototype is wired.
    expect(broker).toContain("RevocationRowOutcome::EnforcementUnavailable");
    expect(productionReferencesBurnAlert(productionRust)).toBe(false);

    // The source contract must preserve both the implemented fact and the
    // absent sender/upload/fetch/verify/render integration.
    const assertUnwiredTruth = (source: string): void => {
      expect(source).toContain("implements only the canonical payload bytes");
      expect(source).toContain(
        "Current production code does not construct this",
      );
      expect(source).toContain(
        "The separate bilateral `0x0A`",
      );
      expect(source).toContain(
        "implemented-unwired and are not a working peer action",
      );
      expect(source).toContain(
        "That sender/recipient integration does not currently exist",
      );
    };
    assertUnwiredTruth(burnAlert);

    // Each public prototype type/function can independently become reachable;
    // these positives keep the zero-caller detector failure-capable.
    for (const syntheticCaller of [
      "let payload = BurnAlertPayload::from_scope(sender, peer, scope, text, now);",
      "let signature = sign_burn_alert(sender, &payload);",
      "let accepted = verify_burn_alert(sender_public, &payload, &signature);",
    ]) {
      expect(productionReferencesBurnAlert(syntheticCaller)).toBe(true);
    }

    // Removing the status qualifier or restoring either former present-tense
    // integration claim must fail.
    expect(() =>
      assertUnwiredTruth(
        burnAlert.replace(
          "implemented-unwired and are not a working peer action",
          "available as a working peer action",
        ),
      ),
    ).toThrow();
    expect(burnAlert).not.toContain("the client\n//! uploads a regular wrapped-keys row");
    expect(burnAlert).not.toContain("Recipients call this after decrypting");

    // Comments and the conventional cfg(test) module are not production
    // authority.
    expect(
      productionReferencesBurnAlert(
        [
          "// sign_burn_alert(sender, &payload);",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() { verify_burn_alert(pk, &payload, &sig); }",
          "}",
        ].join("\n"),
      ),
      ).toBe(false);
  });

  it("public 'independently reviewed' claim gate unlocked", () => {
    const gateScript = fileURLToPath(
      new URL("../../../scripts/check-app-claims.mjs", import.meta.url),
    );
    const output = execFileSync(process.execPath, [gateScript, "--self-test"], {
      encoding: "utf8",
      maxBuffer: 1024 * 1024,
    });

    expect(output).not.toContain("FAIL");
    expect(output).toContain("PASS passes narrow independently reviewed remediation claim");
    expect(output).toContain("PASS catches narrow independently reviewed claim without limitation");
    expect(output).toContain("PASS catches broad independently reviewed encryption claim");
    expect(output).toContain("PASS catches independently reviewed claim that implies outside firm");
  });

  it("does not describe local burn cleanup as remote wrapped-key destruction", () => {
    const main = readRelative("../../osl-hub/src/main.rs");
    const security = readRelative("../../osl-hub/src/security.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
    ].join("\n");
    const productionUsesRemoteWrappedKeyUpload = (source: string): boolean =>
      /(?:\.post_wrapped_key\s*\(|\bWrappedKeyUpload\s*\{)/u.test(
        rustProductionPrefix(source),
      );
    const commandsProduction = rustProductionPrefix(commands);
    const applyBurnBody = commandsProduction
      .split("pub fn cmd_osl_apply_burn(")[1]
      ?.split("\n}\n\npub fn ")[0] ?? "";
    const classifyBurnPath = (
      mainSource: string,
      securitySource: string,
      commandSource: string,
      // Injectable for the same reason as above: `burn_active_hub_context` is
      // registered in hub_command_surface.rs, not in main.rs, so the
      // stage-removal proof has to mutate the surface text.
      commandSurface: string = hubCommandSurface(),
    ) => {
      const mainProduction = rustWithoutComments(mainSource);
      const securityProduction = rustProductionPrefix(securitySource);
      const commandProduction = rustProductionPrefix(commandSource);
      const handler = commandSurface;
      return {
        registered: handler.includes("burn_active_hub_context"),
        mainCallsSecurity: mainProduction.includes("security::burn_scope("),
        securityCallsLocalBurn: securityProduction.includes(
          "ipc::commands::cmd_osl_apply_burn(",
        ),
        localRowsShredded: commandProduction.includes(
          "store.wipe_wrapped_keys_in_scope(",
        ),
        remoteBlobDeleteAttempted: securityProduction.includes(
          "ipc::prose_token::prose_token_burn_id(",
        ),
      };
    };
    const facts = classifyBurnPath(main, security, commands);

    // Positive controls bind the claims to the actual registered burn route:
    // Tauri -> security orchestration -> local row shred plus best-effort blob
    // deletion.
    expect(facts).toEqual({
      registered: true,
      mainCallsSecurity: true,
      securityCallsLocalBurn: true,
      localRowsShredded: true,
      remoteBlobDeleteAttempted: true,
    });

    // Wrapped-key upload is live for native-overlay relay, and fetch is live
    // for attachment open. Burn must still not be described as a server-held
    // wrapped-key destruction path.
    expect(productionUsesRemoteWrappedKeyUpload(productionRust)).toBe(true);
    expect(productionUsesRemoteWrappedKeyUpload(security)).toBe(false);
    expect(productionUsesRemoteWrappedKeyUpload(commands)).toBe(true);
    expect(commandsProduction).toContain("post_wrapped_key_before_producing_link");
    expect(productionUsesRemoteWrappedKeyUpload(applyBurnBody)).toBe(false);
    expect(rustProductionPrefix(productionRust)).toMatch(/\.fetch_wrapped_key\s*\(/u);

    const assertBurnTruth = (
      securitySource: string,
      commandSource: string,
    ): void => {
      expect(securitySource).toContain(
        "best-effort deletion of each known OSL cipher-store blob",
      );
      expect(securitySource).toContain(
        "Burn does not spend or delete native-overlay server-held wrapped-key",
      );
      expect(securitySource).toContain(
        "does not erase connected-service/provider copies or",
      );
      expect(securitySource).toContain(
        "destroy the recipient's long-term decryption authority",
      );
      expect(commandSource).toContain(
        "That column is",
      );
      expect(commandSource).toContain(
        "local store state, not a native-overlay server wrapped-key row",
      );
      expect(commandSource).toContain(
        "it is not remote wrapped-key deletion or cryptographic erasure",
      );
    };
    assertBurnTruth(security, commands);

    for (const syntheticIntegration of [
      "client.post_wrapped_key(sender, &upload)?;",
      "let upload = WrappedKeyUpload { content_id, ..template };",
    ]) {
      expect(productionUsesRemoteWrappedKeyUpload(syntheticIntegration)).toBe(true);
    }
    expect(productionUsesRemoteWrappedKeyUpload(
      "client.fetch_wrapped_key(recipient, content_id)?;",
    )).toBe(false);

    // One stage-removal mutation per reachable burn stage.
    expect(
      classifyBurnPath(
        main,
        security,
        commands,
        hubCommandSurface().replaceAll("burn_active_hub_context", "removed_command"),
      ).registered,
    ).toBe(false);
    expect(
      classifyBurnPath(
        main.replaceAll("security::burn_scope(", "security::burn_scope_removed("),
        security,
        commands,
      ).mainCallsSecurity,
    ).toBe(false);
    expect(
      classifyBurnPath(
        main,
        security.replace(
          "ipc::commands::cmd_osl_apply_burn(",
          "ipc::commands::cmd_osl_apply_burn_removed(",
        ),
        commands,
      ).securityCallsLocalBurn,
    ).toBe(false);
    expect(
      classifyBurnPath(
        main,
        security,
        commands.replace(
          "store.wipe_wrapped_keys_in_scope(",
          "store.wipe_wrapped_keys_in_scope_removed(",
        ),
      ).localRowsShredded,
    ).toBe(false);
    expect(
      classifyBurnPath(
        main,
        security.replaceAll(
          "ipc::prose_token::prose_token_burn_id(",
          "ipc::prose_token::prose_token_burn_id_removed(",
        ),
        commands,
      ).remoteBlobDeleteAttempted,
    ).toBe(false);

    // Reinstating the former remote/key-destruction semantics fails the claim
    // contract even while the implementation stays unchanged.
    expect(() =>
      assertBurnTruth(
        security.replace(
          "best-effort deletion of each known OSL cipher-store blob",
          "wrapped keys gone, remote cipher-store blobs gone",
        ),
        commands,
      ),
    ).toThrow();
    for (const forbidden of [
      "wrapped keys gone",
      "can now never fetch it",
      "destroys OUR ability to re-decrypt",
      "Local decrypt is gated solely by wrapped-key presence",
      "wipes its decrypt capability",
    ]) {
      expect(`${security}\n${commands}`).not.toContain(forbidden);
    }

    // Comment/test-only decoys are not remote wrapped-key production use.
    expect(
      productionUsesRemoteWrappedKeyUpload(
        [
          "// client.post_wrapped_key(sender, &upload)?;",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() { client.fetch_wrapped_key(recipient, id); }",
          "}",
        ].join("\n"),
      ),
      ).toBe(false);
  });

  it("wires keyserver burn only through verified unwhitelist burn", () => {
    const burnPrimitive = readRelative("../../../crates/keystore/src/burn.rs");
    const keystoreClient = readRelative("../../../crates/keystore/src/client.rs");
    const keystoreLib = readRelative("../../../crates/keystore/src/lib.rs");
    const main = readRelative("../../osl-hub/src/main.rs");
    const security = readRelative("../../osl-hub/src/security.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
    ].join("\n");
    const productionUsesKeyserverBurn = (source: string): boolean =>
      /(?:\bKeyServerClient::burn\b|\bBurnScope::(?:Single|ToUser|All)\b|\.\s*burn\s*\()/u.test(
        rustProductionPrefix(source),
      );

    // Positive implementation controls prevent deletion of the primitive
    // subsystem from satisfying the reachability check.
    expect(burnPrimitive).toContain("pub enum BurnScope");
    expect(burnPrimitive).toContain("pub fn canonical_burn_bytes(");
    expect(burnPrimitive).toContain("pub fn sign_burn(");
    expect(keystoreClient).toContain("pub fn burn(");
    expect(keystoreLib).toContain(
      "pub use burn::{canonical_burn_bytes, sign_burn, BurnScope, BURN_DOMAIN}",
    );

    // Positive controls for the separate reachable product burn.
    const mainProduction = rustWithoutComments(main);
    const handler = hubCommandSurface();
    expect(handler).toContain("burn_active_hub_context");
    expect(mainProduction).toContain("security::burn_scope(");
    expect(rustProductionPrefix(security)).toContain(
      "ipc::commands::cmd_osl_apply_burn(",
    );
    expect(rustProductionPrefix(commands)).toContain(
      "store.wipe_wrapped_keys_in_scope(",
    );

    expect(productionUsesKeyserverBurn(productionRust)).toBe(true);
    expect(rustProductionPrefix(commands)).toContain(
      "fn burn_wrapped_keys_for_peer(",
    );
    expect(rustProductionPrefix(commands)).toContain(
      "post_burn_control_and_delete_wrapped_keys",
    );

    const assertBurnPrimitiveTruth = (source: string): void => {
      expect(source).toContain(
        "Wrapped-key deletion request primitives",
      );
      expect(source).toContain(
        "IPC production code uses it only from the in-Discord",
      );
      expect(source).toContain(
        "refuses Discord-only\n//! identifiers",
      );
      expect(source).toContain(
        "The separate active-context product burn still performs local-row cleanup",
      );
    };
    assertBurnPrimitiveTruth(burnPrimitive);

    for (const syntheticCaller of [
      "let scope = BurnScope::All;",
      "let scope = BurnScope::Single { content_id };",
      "KeyServerClient::burn(client, identity, &scope)?;",
      "client.burn(identity, &scope)?;",
    ]) {
      expect(productionUsesKeyserverBurn(syntheticCaller)).toBe(true);
    }

    expect(() =>
      assertBurnPrimitiveTruth(
        burnPrimitive.replace(
          "IPC production code uses it only from the in-Discord",
          "All product burn paths use it for",
        ),
      ),
    ).toThrow();
    expect(burnPrimitive).not.toContain(
      'the user-facing Rust API\n//! for "delete my wrapped-key blobs from the server',
    );

    expect(
      productionUsesKeyserverBurn(
        [
          "// client.burn(identity, &scope)?;",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() { let scope = BurnScope::All; }",
          "}",
        ].join("\n"),
      ),
    ).toBe(false);
  });

  it("keeps the prekey lifecycle limited to publish and replenish paths", () => {
    const prekeys = readRelative("../../../crates/keystore/src/prekeys.rs");
    const keystoreClient = readRelative("../../../crates/keystore/src/client.rs");
    const keystoreLib = readRelative("../../../crates/keystore/src/lib.rs");
    const main = readRelative("../../osl-hub/src/main.rs");
    const broker = readRelative("../../osl-hub/src/broker.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const wire = readRelative("../../../crates/ipc/src/wire_v2.rs");
    const wireRn = readRelative("../../../crates/ipc/src/wire_rn.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
    ].join("\n");
    const stripDormantPrekeyScheduler = (source: string): string =>
      source
        .replace(
          /pub const PREKEY_REPLENISH_INTERVAL_SECONDS:[\s\S]*?(?=\npub fn cmd_aead_seal\()/u,
          "",
        )
        .replace(
          /\n#\[cfg\(test\)\]\nmod production_identity_bundle_pipeline_tests \{[\s\S]*$/u,
          "",
        );
    const productionReferencesPrekeyLifecycle = (source: string): boolean =>
      /\b(?:PrekeyState|PrekeyConfig|OpkEntry|SpkEntry|ReplenishOpk|ReplenishSpk|REPLENISH_DOMAIN|SPK_ROTATION_INTERVAL_SECONDS|fetch_prekey_bundle|replenish_prekeys|replenish_using_state|load_prekey_state|save_prekey_state|sign_replenish_batch|canonical_replenish_bytes|should_rotate_spk|rotate_spk|should_replenish|add_opk_batch|replenish_count_to_target|consume_opk)\b/u.test(
        stripDormantPrekeyScheduler(rustProductionPrefix(source)),
      );

    // Positive implementation controls prevent deletion of the dormant
    // subsystem from satisfying the absence check.
    for (const implementationSymbol of [
      "pub struct PrekeyState",
      "pub fn new(",
      "pub fn should_rotate_spk(",
      "pub fn rotate_spk(",
      "pub fn should_replenish(",
      "pub fn save_prekey_state(",
      "pub fn load_prekey_state(",
      "pub fn sign_replenish_batch(",
    ]) {
      expect(prekeys).toContain(implementationSymbol);
    }
    for (const clientMethod of [
      "pub fn fetch_prekey_bundle(",
      "pub fn replenish_prekeys(",
      "pub fn replenish_using_state(",
    ]) {
      expect(keystoreClient).toContain(clientMethod);
    }
    expect(keystoreLib).toContain(
      "sign_replenish_batch, OpkEntry, PrekeyConfig, PrekeyState",
    );

    // The separate production messaging path must remain present and
    // explicitly stateless, so an empty product tree cannot satisfy the gate.
    const messagingFacts = classifyMessagingProductionPath(
      main,
      broker,
      commands,
      state,
      productionRust,
    );
    expect(messagingFacts.registeredPrepareCommand).toBe(true);
    expect(messagingFacts.mainCallsBroker).toBe(true);
    expect(messagingFacts.brokerCallsIpc).toBe(true);
    expect(messagingFacts.statelessV3Fallback).toBe(true);
    expect(wire).toContain("recipient_ik plays both ik and spk");
    expect(wire).toContain("None,\n            &recip.mlkem_pub,");

    expect(productionReferencesPrekeyLifecycle(productionRust)).toBe(true);
    expect(commands).toContain("fn publish_initial_prekeys_after_register(");
    expect(commands).toContain("fn publish_initial_prekeys_at<");
    expect(commands).toContain(
      "client.replenish_prekeys(identity, Some(&state.current_spk), &state.opk_pool)",
    );
    expect(commands).toContain("publish_initial_prekeys_after_register(&client, id);");
    expect(commands).toContain("pub fn run_prekey_replenishment_tick(");
    expect(commands).toContain("client\n                .replenish_using_state(");
    expect(commands).toContain("crate::wire_rn::RN_WIRE_IN_ENABLED");
    const wireRnProduction = rustProductionPrefix(wireRn);
    expect(rustProductionPrefix(productionRust).replace(wireRnProduction, "")).not.toMatch(
      /\bconsume_opk\s*\(/u,
    );
    expect(wireRnProduction).toMatch(/\bconsume_opk\s*\(/u);
    expect(wireRnProduction).toContain("prekeys.consume_opk(opk_id)");
    expect(wireRnProduction).toContain(
      "accept_b5_prekey_state_and_persist_with_sealer",
    );
    expect(wireRnProduction).toContain("pub const RN_WIRE_IN_ENABLED: bool = false;");
    expect(rustProductionPrefix(productionRust)).toMatch(/\bconsume_opk\s*\(/u);

    const assertWiredPrekeyTruth = (source: string): void => {
      expect(source).toContain(
        "Client-side prekey primitives.",
      );
      expect(source).toContain(
        "IPC production registration now constructs",
      );
      expect(source).toContain(
        "keyserver initial replenish path after successful identity registration",
      );
      expect(source).toContain(
        "IPC also owns server-count-driven\n//! replenish scheduling",
      );
      expect(source).toContain(
        "matching the server protocol's atomic-pop design",
      );
      expect(source).toContain(
        "broader receive-side product lifecycle work remains\n//! future integration work",
      );
      expect(source).toMatch(
        /reserved\s*\n\s*\/\/\/ for a future integrated receive-side PQXDH handshake/u,
      );
      expect(source).toContain(
        "Current production code has no such caller",
      );
    };
    assertWiredPrekeyTruth(prekeys);

    for (const syntheticProductionReference of [
      "use keystore::PrekeyState;",
      "let state = PrekeyState::new(identity, config, now);",
      "client.fetch_prekey_bundle(identity, peer)?;",
      "client.replenish_prekeys(identity, spk, opks)?;",
      "client.replenish_using_state(identity, state, remaining, now)?;",
      "save_prekey_state(path, state, sealer)?;",
      "state.consume_opk(opk_id);",
      "let rotate = PrekeyState::rotate_spk;",
      "use keystore::ReplenishOpk as UploadKey;",
      "let replenish = KeyServerClient::replenish_prekeys;",
      "use keystore::PrekeyState as SessionBootstrap;",
    ]) {
      expect(
        productionReferencesPrekeyLifecycle(syntheticProductionReference),
      ).toBe(true);
    }

    expect(() =>
      assertWiredPrekeyTruth(
        prekeys.replace(
          "IPC production registration now constructs",
          "Current Hub/IPC production code neither constructs",
        ),
      ),
    ).toThrow();
    expect(
      productionReferencesPrekeyLifecycle(
        [
          "// client.fetch_prekey_bundle(identity, peer)?;",
          "/* use keystore::PrekeyState; */",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() {",
          "    let state = PrekeyState::new(identity, config, now);",
          "  }",
          "}",
        ].join("\n"),
      ),
    ).toBe(false);
  });

  it("keeps sequence burn-floor enforcement classified as implemented-unwired", () => {
    const security = readRelative("../../osl-hub/src/security.rs");
    const broker = readRelative("../../osl-hub/src/broker.rs");
    const main = readRelative("../../osl-hub/src/main.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const wire = readRelative("../../../crates/ipc/src/wire_v2.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
    ].join("\n");
    const sequenceSymbols = [
      "next_peer_send_seq",
      "peer_scope_commitment",
      "admit_peer_content_seq",
    ] as const;
    const productionSequenceReferenceCount = (
      source: string,
      symbol: (typeof sequenceSymbols)[number],
    ): number =>
      rustProductionPrefix(source).match(
        new RegExp(`\\b${symbol}\\b`, "gu"),
      )?.length ?? 0;

    // Positive implementation controls: all three prototype functions must
    // remain, so removing the subsystem cannot satisfy zero integration.
    for (const symbol of sequenceSymbols) {
      expect(security).toContain(`pub fn ${symbol}(`);
      expect(productionSequenceReferenceCount(productionRust, symbol)).toBe(1);
    }
    expect(security).toContain("counters\n        .next_send_seq(&commitment)");
    expect(security).toContain(
      "ipc::revocation::accept_content(&ledger, &commitment, send_seq)",
    );
    expect(security).toContain(
      "ipc::revocation::record_content_accepted(&mut ledger, &commitment, send_seq)",
    );

    // Positive controls for the separate reachable product path: registered
    // prepare -> broker -> IPC -> stateless v3, with no OPK.
    const messagingFacts = classifyMessagingProductionPath(
      main,
      broker,
      commands,
      state,
      productionRust,
    );
    expect(messagingFacts.registeredPrepareCommand).toBe(true);
    expect(messagingFacts.mainCallsBroker).toBe(true);
    expect(messagingFacts.brokerCallsIpc).toBe(true);
    expect(messagingFacts.statelessV3Fallback).toBe(true);
    expect(wire).toContain("recipient_ik plays both ik and spk");
    expect(wire).toContain("None,\n            &recip.mlkem_pub,");

    // The reachable inbound notice path must retain and refuse rather than
    // claim that an unenforced floor was applied.
    expect(broker).toContain(
      "(RevocationRowOutcome::EnforcementUnavailable, None)",
    );
    expect(broker).toContain(
      "production content envelope and decrypt path call none of them",
    );

    const assertUnwiredSequenceTruth = (source: string): void => {
      expect(source).toContain(
        "Implemented-unwired allocator for an authenticated per-peer",
      );
      expect(source).toContain(
        "Current production send paths do neither",
      );
      expect(source).toContain(
        "Implemented-unwired opaque commitment helper",
      );
      expect(source).toContain(
        "current production envelopes do not carry either value",
      );
      expect(source).toContain(
        "Implemented-unwired admission helper for a peer burn floor",
      );
      expect(source).toContain(
        "Current production decrypt paths do not call it",
      );
    };
    assertUnwiredSequenceTruth(security);

    for (const [symbol, syntheticReference] of [
      [
        "next_peer_send_seq",
        "security::next_peer_send_seq(core, security, peer, scope)?;",
      ],
      [
        "peer_scope_commitment",
        "use crate::security::peer_scope_commitment as scope_for_wire;",
      ],
      [
        "admit_peer_content_seq",
        "let admit = security::admit_peer_content_seq;",
      ],
    ] as const) {
      expect(
        productionSequenceReferenceCount(
          `${productionRust}\n${syntheticReference}`,
          symbol,
        ),
      ).toBe(2);
    }

    expect(() =>
      assertUnwiredSequenceTruth(
        security.replace(
          "Implemented-unwired allocator for an authenticated per-peer",
          "Allocator for each authenticated per-peer",
        ),
      ),
    ).toThrow();
    expect(security).not.toContain(
      "which the broker puts on\n/// the wire next to `send_seq`",
    );

    const commentAndTestOnly = [
      "// security::next_peer_send_seq(core, security, peer, scope)?;",
      "/* peer_scope_commitment(core, peer, scope)?; */",
      "#[cfg(test)]",
      "mod tests {",
      "  fn fixture() { admit_peer_content_seq(core, security, peer, scope, 1); }",
      "}",
    ].join("\n");
    for (const symbol of sequenceSymbols) {
      expect(
        productionSequenceReferenceCount(commentAndTestOnly, symbol),
      ).toBe(0);
    }
  });

  it("keeps wrapped-key upload scoped to native overlay relay", () => {
    const wrappedKey = readRelative("../../../crates/keystore/src/wrapped_key.rs");
    const signedGet = readRelative("../../../crates/keystore/src/signed_get.rs");
    const keystoreClient = readRelative("../../../crates/keystore/src/client.rs");
    const keystoreLib = readRelative("../../../crates/keystore/src/lib.rs");
    const main = readRelative("../../osl-hub/src/main.rs");
    const broker = readRelative("../../osl-hub/src/broker.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const state = readRelative("../../../crates/ipc/src/state.rs");
    const wire = readRelative("../../../crates/ipc/src/wire_v2.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
      readProductionRustTree("../../../src-tauri/src/"),
    ].join("\n");
    const productionReferencesWrappedKeyLifecycle = (source: string): boolean =>
      /\b(?:WrappedKeyUpload|post_wrapped_key|fetch_wrapped_key|sign_wrapped_key_post|sign_wrapped_key_get|canonical_wrapped_key_post_bytes|canonical_wrapped_key_get_bytes|WRAPPED_KEY_POST_DOMAIN|WRAPPED_KEY_GET_DOMAIN)\b/u.test(
        rustProductionPrefix(source),
      );

    // Positive implementation controls keep removal of the signing subsystem
    // from satisfying the production-scope assertions.
    for (const implementationSymbol of [
      "pub struct WrappedKeyUpload",
      "pub fn canonical_wrapped_key_post_bytes(",
      "pub fn sign_wrapped_key_post(",
    ]) {
      expect(wrappedKey).toContain(implementationSymbol);
    }
    for (const implementationSymbol of [
      "pub fn canonical_wrapped_key_get_bytes(",
      "pub fn sign_wrapped_key_get(",
    ]) {
      expect(signedGet).toContain(implementationSymbol);
    }
    expect(keystoreClient).toContain("pub fn fetch_wrapped_key(");
    expect(keystoreClient).toContain("pub fn post_wrapped_key(");
    expect(keystoreLib).toContain(
      "canonical_wrapped_key_post_bytes, sign_wrapped_key_post, WrappedKeyUpload",
    );
    expect(keystoreLib).toContain(
      "canonical_wrapped_key_get_bytes, sign_prekey_bundle_get",
    );

    // Positive controls for the real production content path prevent an empty
    // or deleted product tree from making absence look like proof.
    const messagingFacts = classifyMessagingProductionPath(
      main,
      broker,
      commands,
      state,
      productionRust,
    );
    expect(messagingFacts.registeredPrepareCommand).toBe(true);
    expect(messagingFacts.mainCallsBroker).toBe(true);
    expect(messagingFacts.brokerCallsIpc).toBe(true);
    expect(messagingFacts.statelessV3Fallback).toBe(true);
    expect(wire).toContain("recipient_ik plays both ik and spk");
    expect(wire).toContain("None,\n            &recip.mlkem_pub,");

    expect(productionReferencesWrappedKeyLifecycle(productionRust)).toBe(true);
    expect(rustProductionPrefix(productionRust)).toMatch(
      /(?:\.post_wrapped_key\s*\(|\bWrappedKeyUpload\s*\{)/u,
    );

    const brokerProduction = rustProductionPrefix(broker);
    const producer = brokerProduction
      .split("fn prepare_peer_inbox_text(")[1]
      ?.split("fn split_native_overlay_text(")[0] ?? "";
    const helper = brokerProduction
      .split("fn post_native_overlay_wrapped_key(")[1]
      ?.split("fn prepare_direct_manual_v3(")[0] ?? "";
    const wrappedPost = producer.indexOf("post_native_overlay_wrapped_key(");
    const relayPost = producer.indexOf(
      ".post_control_inbox(&identity, &manual.peer_osl_user_id, &scope_id, &bundle)",
    );
    expect(wrappedPost).toBeGreaterThanOrEqual(0);
    expect(relayPost).toBeGreaterThanOrEqual(0);
    expect(wrappedPost).toBeLessThan(relayPost);
    expect(helper).toContain(".post_wrapped_key(identity, &upload)");

    const assertScopedWrappedKeyTruth = (
      postSource: string,
      getSource: string,
    ): void => {
      expect(postSource).toContain(
        "Wrapped-key upload signing primitives",
      );
      expect(postSource).toContain(
        "constructs [`WrappedKeyUpload`] only for the native",
      );
      expect(postSource).toContain(
        "not remote\n//! burn key destruction or a product-wide attachment lifecycle",
      );
      expect(getSource).toContain(
        "Canonical keyserver GET signing primitives",
      );
      expect(getSource).toContain(
        "uses wrapped-key GET for bounded attachment open",
      );
      expect(getSource).toContain(
        "not by themselves establish a live destructive-read flow",
      );
    };
    assertScopedWrappedKeyTruth(wrappedKey, signedGet);

    for (const syntheticProductionReference of [
      "let upload = WrappedKeyUpload { content_id, ..template };",
      "client.post_wrapped_key(sender, &upload)?;",
      "client.fetch_wrapped_key(recipient, content_id)?;",
      "use keystore::WrappedKeyUpload as ServerShare;",
      "let post = KeyServerClient::post_wrapped_key;",
      "let fetch = KeyServerClient::fetch_wrapped_key;",
      "sign_wrapped_key_post(identity, upload, now);",
      "sign_wrapped_key_get(identity, content_id, now);",
    ]) {
      expect(
        productionReferencesWrappedKeyLifecycle(syntheticProductionReference),
      ).toBe(true);
    }

    expect(() =>
      assertScopedWrappedKeyTruth(
        wrappedKey.replace(
          "constructs [`WrappedKeyUpload`] only for the native",
          "constructs [`WrappedKeyUpload`] for every",
        ),
        signedGet,
      ),
    ).toThrow();
    expect(wrappedKey).not.toContain("Every\n//! persisted field");
    expect(signedGet).not.toContain(
      "authorization for keyserver GETs that consume server state",
    );

    expect(
      productionReferencesWrappedKeyLifecycle(
        [
          "// client.post_wrapped_key(sender, &upload)?;",
          "/* let fetch = KeyServerClient::fetch_wrapped_key; */",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() {",
          "    let upload = WrappedKeyUpload { content_id, ..template };",
          "  }",
          "}",
        ].join("\n"),
      ),
    ).toBe(false);
  });

  it("binds the duress engine to a distinct reachable gate outcome", () => {
    const duress = readRelative("../../../crates/keystore/src/duress.rs");
    const password = readRelative("../../../crates/keystore/src/password.rs");
    const keystoreLib = readRelative("../../../crates/keystore/src/lib.rs");
    const main = readRelative("../../osl-hub/src/main.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
      readProductionRustTree("../../../src-tauri/src/"),
    ].join("\n");
    const legacyDuressSymbols = [
      "DuressEngine",
      "DuressHandlers",
      "DuressPaths",
      "DuressJournal",
      "DuressReport",
      "WipeStep",
      "verify_against_record",
      "VerifyOutcome",
      "InactivityTimer",
    ] as const;
    const productionReferencesLegacyDuress = (source: string): boolean =>
      new RegExp(`\\b(?:${legacyDuressSymbols.join("|")})\\b`, "u").test(
        rustProductionPrefix(source),
      );

    // Positive implementation/re-export controls prevent removing the
    // primitive subsystem from satisfying the reachability assertions below.
    for (const implementationSymbol of [
      "pub enum WipeStep",
      "pub struct DuressHandlers",
      "pub struct DuressPaths",
      "pub struct DuressJournal",
      "pub struct DuressReport",
      "pub struct DuressEngine",
      "pub fn execute(&self)",
      "pub fn resume_if_pending(&self)",
    ]) {
      expect(duress).toContain(implementationSymbol);
    }
    for (const implementationSymbol of [
      "pub enum VerifyOutcome",
      "pub fn verify_against_record(",
      "pub struct InactivityTimer",
      "pub fn should_reprompt(&self)",
    ]) {
      expect(password).toContain(implementationSymbol);
    }
    for (const exportedSymbol of [
      "DuressEngine",
      "DuressError",
      "DuressHandlers",
      "DuressJournal",
      "DuressPaths",
      "DuressReport",
    ]) {
      expect(keystoreLib).toContain(exportedSymbol);
    }
    expect(keystoreLib).toContain(
      "verify_against_record, Argon2Params, InactivityTimer",
    );

    expect(productionReferencesLegacyDuress(productionRust)).toBe(true);
    expect(rustProductionPrefix(productionRust)).toContain(
      "production_duress_engine",
    );
    expect(rustProductionPrefix(productionRust)).toContain(
      "execute_production_duress",
    );

    // Positive controls for the separate, reachable Hub password gate and the
    // now-live production duress engine.
    expect(productionRust).toContain(
      "pub duress_engine: Mutex<keystore::DuressEngine>",
    );
    expect(productionRust).toContain("record_wrong_password_attempt_or_duress(");
    expect(productionRust).toContain("WrongPasswordAttemptAction::DuressTriggered");
    expect(productionRust).toContain("DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT");

    const mainProduction = rustWithoutComments(main);
    const handler = hubCommandSurface();
    expect(handler).toContain("unlock_hub_password_gate,");
    expect(mainProduction).toContain(
      "startup_gate::verify_password_role(&verify_app.state::<HubCoreState>(), password)",
    );
    expect(mainProduction).toContain("startup_gate::verify_duress_pin");
    expect(mainProduction).toContain("VerifiedGateRole::Duress => {");
    expect(mainProduction).toContain("HubGateUnlockResult::duress(verification)");
    expect(mainProduction).toContain("VerifiedGateRole::Burn => {");
    expect(mainProduction).toContain("cleanup::execute_verified_gate_burn(");

    const assertDuressTruth = (
      duressSource: string,
      passwordSource: string,
    ): void => {
      expect(duressSource).toContain(
        "Duress-engine primitives.",
      );
      expect(duressSource).toContain(
        "IPC constructs a production [`DuressEngine`] in application state",
      );
      expect(duressSource).toContain(
        "Hub password gate invokes it for duress outcomes",
      );
      expect(duressSource).toContain(
        "The separate burn-password\n//! path still uses `startup_gate` and `cleanup`",
      );
      expect(duressSource).toContain(
        "With no callback, this step is reported as",
      );
      expect(duressSource).toContain(
        "After each step attempt, the engine records its",
      );
      expect(passwordSource).toContain(
        "Legacy unlock/duress record primitives plus the shared inactivity timer",
      );
      expect(passwordSource).toContain(
        "Current Hub/IPC production code uses [`InactivityTimer`] for auto-lock",
      );
      expect(passwordSource).toContain(
        "routes duress outcomes to [`crate::duress::DuressEngine`]",
      );
      expect(passwordSource).toContain(
        "When invoked, this module's storage helpers serialize",
      );
    };
    assertDuressTruth(duress, password);

    for (const syntheticProductionReference of [
      "let engine = DuressEngine::new(journal, paths, handlers);",
      "let resume = DuressEngine::resume_if_pending;",
      "use keystore::DuressEngine as WipeEngine;",
      "let verify = keystore::verify_against_record;",
      "match outcome { VerifyOutcome::Duress => burn(), _ => {} }",
      "let timer = InactivityTimer::from_seconds(900);",
    ]) {
      expect(
        productionReferencesLegacyDuress(syntheticProductionReference),
      ).toBe(true);
    }

    expect(() =>
      assertDuressTruth(
        duress.replace(
          "IPC constructs a production [`DuressEngine`] in application state",
          "The UI animation alone handles duress",
        ),
        password,
      ),
    ).toThrow();
    expect(duress).not.toContain(
      "The caller (Tauri shell) plays the normal unlock animation",
    );
    expect(password).not.toContain(
      "then triggers\n//!   the duress flow",
    );

    expect(
      productionReferencesLegacyDuress(
        [
          "// let engine = DuressEngine::new(journal, paths, handlers);",
          "/* verify_against_record(record, password)?; */",
          "#[cfg(test)]",
          "mod tests {",
          "  fn fixture() { let timer = InactivityTimer::from_seconds(900); }",
          "}",
        ].join("\n"),
      ),
    ).toBe(false);
  });
});
