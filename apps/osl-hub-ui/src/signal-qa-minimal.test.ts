import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ui = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const signalUi = readFileSync(new URL("./signal-qa-main.ts", import.meta.url), "utf8");
const vite = readFileSync(new URL("../vite.config.ts", import.meta.url), "utf8");
const native = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const cargo = readFileSync(new URL("../../osl-hub/Cargo.toml", import.meta.url), "utf8");
const tauriQa = readFileSync(new URL("../../osl-hub/tauri.signal-qa.conf.json", import.meta.url), "utf8");

function signalQaNativeMain(): string {
  const start = native.indexOf("#[cfg(feature = \"signal-qa-shell\")]\nfn main()");
  const end = native.indexOf("#[cfg(not(feature = \"signal-qa-shell\"))]", start);
  expect(start).toBeGreaterThan(-1);
  expect(end).toBeGreaterThan(start);
  return native.slice(start, end);
}

describe("minimal Signal QA flavor", () => {
  it("has an explicit compile-time feature and a narrow IPC allowlist", () => {
    expect(cargo).toContain("signal-qa-shell = []");
    const source = signalQaNativeMain();
    for (const required of [
      "list_native_apps", "host_native_app_window",
      "resize_native_app_window", "focus_native_app_window", "detach_native_app_window",
      "get_signal_protected_send_readiness",
    ]) expect(source).toContain(required);
    for (const excluded of [
      "list_linked_services", "install_native_app", "open_service_host", "scrub_imap_delete",
      "prepare_encrypted_text", "prepare_hub_attachment", "check_hub_for_updates",
      "list_osl_chat_history", "begin_browser_account_import", "get_core_readiness",
      "unlock_hub_password_gate", "create_hub_osl_identity", "setup_hub_main_password",
    ]) expect(source).not.toContain(excluded);
  });

  it("does not initialize unrelated OSL stores or plugins", () => {
    const source = signalQaNativeMain();
    expect(source).not.toContain("tauri_plugin_updater");
    expect(source).not.toContain("ServiceRegistryState::load");
    expect(source).not.toContain("ServiceScopeIndexState::load");
    expect(source).not.toContain("ScrubIndexState::default");
    expect(source).not.toContain("HubUpdaterState::default");
    expect(source).not.toContain("HubNotificationState::default");
    expect(source).not.toContain("HubBrokerState::default");
    expect(source).not.toContain("HubSecurityState::default");
    expect(source).not.toContain("HubIdentityRegistryState::default");
    expect(source).not.toContain("ServiceHostState::default");
    expect(source).not.toContain("ScrubImapState::default");
    expect(source).not.toContain("tauri_plugin_single_instance");
  });

  it("uses an isolated application identity with no production updater", () => {
    expect(native).toContain('tauri::generate_context!("tauri.signal-qa.conf.json")');
    expect(tauriQa).toContain('"identifier": "org.oslprivacy.signalqa"');
    expect(tauriQa).toContain('"endpoints": []');
    expect(tauriQa).not.toContain("discord-privacy-client/releases");
  });

  it("keeps the legacy general renderer branch isolated from the deployed entry", () => {
    const branch = ui.indexOf("if (signalQaShellEnabled) {", ui.indexOf("async function bootstrap"));
    const preferences = ui.indexOf("const preferencesRequest", branch);
    expect(branch).toBeGreaterThan(-1);
    expect(preferences).toBeGreaterThan(branch);
    expect(ui.slice(branch, preferences)).toContain("route = \"signal-qa\"");
    expect(ui.slice(branch, preferences)).toContain("return;");
  });

  it("removes setup, recovery, and password surfaces from the dedicated entry", () => {
    for (const excluded of ["identity-password", "recovery-continue", "Create account", "Enter your password", "setupSignalQaPassword", "unlockSignalQa"]) {
      expect(signalUi).not.toContain(excluded);
    }
  });

  it("replaces the general renderer with a dedicated Signal-only entry", () => {
    expect(vite).toContain('"/src/main.ts": fileURLToPath(new URL("./src/signal-qa-main.ts"');
    expect(signalUi).toContain('id="workspace-render-surface"');
    expect(signalUi).toContain("void claimSignal()");
    expect(signalUi).toContain("signalQaNativeDependencies");
    expect(signalUi).toContain('if (status === "open") return "Claimed"');
    for (const excluded of ["Scrub", "Discord", "Telegram", "OSL Chats", "loadLinkedServices", "home-app"]) {
      expect(signalUi).not.toContain(excluded);
    }
  });
});
