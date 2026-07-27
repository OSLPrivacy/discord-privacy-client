import { readFileSync, readdirSync } from "node:fs";
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

function rustProductionPrefix(source: string): string {
  const testModule = source.search(/#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+tests\s*\{/u);
  const production = testModule < 0 ? source : source.slice(0, testModule);
  return production
    .replace(/\/\*[\s\S]*?\*\//gu, "")
    .replace(/\/\/[^\n]*/gu, "");
}

function classifyMessagingProductionPath(
  main: string,
  broker: string,
  commands: string,
  state: string,
  extraProductionRust = "",
): MessagingProductionFacts {
  const mainProduction = rustProductionPrefix(main);
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
  const handler = mainProduction.slice(mainProduction.indexOf("tauri::generate_handler!["));

  return {
    registeredPrepareCommand: handler.includes("prepare_encrypted_text,"),
    mainCallsBroker: mainProduction.includes("broker::prepare_encrypted_text("),
    brokerCallsIpc: brokerProduction.includes("ipc::commands::cmd_osl_encrypt_message_v2("),
    statelessV3Fallback: dispatcher.includes("crate::wire_v2::encrypt_v3("),
    dmRatchetEnabled: /let\s+v4_dm_enabled\s*=\s*true\s*;/u.test(dispatcher),
    groupSenderKeysEnabled:
      /sender_keys_enabled\s*\.\s*store\s*\(\s*true\b/u.test(allProduction),
    prekeyLifecycleCalled:
      /\.(?:fetch_prekey_bundle|replenish_prekeys|replenish_using_state)\s*\(/u.test(
        allProduction,
      ),
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
    // production gate/caller exists.
    expect(facts.dmRatchetEnabled).toBe(false);
    expect(facts.groupSenderKeysEnabled).toBe(false);
    expect(facts.prekeyLifecycleCalled).toBe(false);

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
        "keystore prekey client is likewise not called by this production",
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
        main.replace("prepare_encrypted_text,", ""),
        broker,
        commands,
        state,
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
        commands.replace("let v4_dm_enabled = false;", "let v4_dm_enabled = true;"),
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

    // Comments and cfg(test)-only fixtures are not production authority.
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
    ).toBe(false);
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

  it("does not describe local burn cleanup as remote wrapped-key destruction", () => {
    const main = readRelative("../../osl-hub/src/main.rs");
    const security = readRelative("../../osl-hub/src/security.rs");
    const commands = readRelative("../../../crates/ipc/src/commands.rs");
    const productionRust = [
      readProductionRustTree("../../osl-hub/src/"),
      readProductionRustTree("../../../crates/ipc/src/"),
    ].join("\n");
    const productionUsesRemoteWrappedKeys = (source: string): boolean =>
      /(?:\.(?:post_wrapped_key|fetch_wrapped_key)\s*\(|\bWrappedKeyUpload\s*\{)/u.test(
        rustProductionPrefix(source),
      );
    const classifyBurnPath = (
      mainSource: string,
      securitySource: string,
      commandSource: string,
    ) => {
      const mainProduction = rustProductionPrefix(mainSource);
      const securityProduction = rustProductionPrefix(securitySource);
      const commandProduction = rustProductionPrefix(commandSource);
      const handler = mainProduction.slice(
        mainProduction.indexOf("tauri::generate_handler!["),
      );
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

    // The keyserver client primitives are implemented elsewhere, but no Hub or
    // IPC production path constructs an upload or calls post/fetch.
    expect(productionUsesRemoteWrappedKeys(productionRust)).toBe(false);

    const assertBurnTruth = (
      securitySource: string,
      commandSource: string,
    ): void => {
      expect(securitySource).toContain(
        "best-effort deletion of each known OSL cipher-store blob",
      );
      expect(securitySource).toContain(
        "Production has no server-held per-message wrapped-key lifecycle",
      );
      expect(securitySource).toContain(
        "does not erase connected-service/provider copies or destroy the",
      );
      expect(commandSource).toContain(
        "That column is",
      );
      expect(commandSource).toContain(
        "local store state, not evidence of the unwired server wrapped-key service",
      );
      expect(commandSource).toContain(
        "it is not remote wrapped-key deletion or cryptographic erasure",
      );
    };
    assertBurnTruth(security, commands);

    for (const syntheticIntegration of [
      "client.post_wrapped_key(sender, &upload)?;",
      "client.fetch_wrapped_key(recipient, content_id)?;",
      "let upload = WrappedKeyUpload { content_id, ..template };",
    ]) {
      expect(productionUsesRemoteWrappedKeys(syntheticIntegration)).toBe(true);
    }

    // One stage-removal mutation per reachable burn stage.
    expect(
      classifyBurnPath(
        main.replaceAll("burn_active_hub_context", "removed_command"),
        security,
        commands,
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
      productionUsesRemoteWrappedKeys(
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
});
