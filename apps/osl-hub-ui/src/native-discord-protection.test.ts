import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const source = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
const nativeMain = readFileSync(fileURLToPath(new URL("../../osl-hub/src/main.rs", import.meta.url)), "utf8");
const nativeAdapter = readFileSync(fileURLToPath(new URL("../../osl-hub/src/native_discord_adapter.rs", import.meta.url)), "utf8");

describe("native Discord protected overlay routing", () => {
  it("offers Protect for native Discord while leaving the in-window sheet embedded-only", () => {
    expect(source).toContain('(activeEmbeddedHost || activeNativeHostId === "discord")');
    expect(source).toContain("localProtectedSheet.open || peerProtectedSheet.open || nativeDiscordProtectionActive");
    expect(source).toMatch(/const protectedSheet = activeEmbeddedHost\s+\? protectedSheetMode/);
    expect(source).toContain("${nativeDiscordProtectPickerMarkup()}");
  });

  it("keeps Discord privacy controls in the trusted OSL header, not the composer overlay", () => {
    const overlay = readFileSync(fileURLToPath(new URL("../overlay.html", import.meta.url)), "utf8");
    const styles = readFileSync(fileURLToPath(new URL("./styles.css", import.meta.url)), "utf8");
    expect(source).toContain('class="native-discord-header-controls"');
    expect(source).toContain('data-open-burn="chat"');
    expect(source).toContain('id="native-discord-covertext"');
    expect(source).toContain('id="native-discord-ai-covertext"');
    expect(source).toContain("Model pack needed");
    expect(source).toContain("no cloud AI is used");
    expect(styles).toContain(".native-discord-header-controls");
    expect(overlay).not.toContain('id="ai-covertext-mode"');
    expect(overlay).toContain('class="overlay-runtime-controls" hidden');
  });

  it("binds only a verified stable friend before opening the separate overlay", () => {
    expect(source).toContain('id="native-protect-verified-peer"');
    expect(source).toContain("candidate.safetyNumberVerified && !candidate.pendingKeyChange");
    expect(source).toContain("await activateNativeManualPeerContext(person.personId)");
    expect(source).toContain("await setActiveHubFriendPermission(context.contextToken, context.personId, true, false)");
    expect(source).toContain("await setNativeDiscordProtectedOverlayOpen(approvedContext.contextToken, true)");
    expect(source).toMatch(/activeNativeHostId !== "discord" \|\| activeNativeHostMode !== expectedMode/g);
  });

  it("focuses Discord and fails closed before requesting the overlay", () => {
    const openProtection = source.slice(
      source.indexOf("async function openNativeDiscordProtection"),
      source.indexOf("function showLocalProtectedChoice"),
    );
    expect(openProtection).toMatch(
      /await focusActiveNativeCompanion\(\)[\s\S]*?Protection stopped: Discord could not be brought forward safely\.[\s\S]*?await setNativeDiscordProtectedOverlayOpen/,
    );
  });

  it("recovers a still-hosted native app only after services are loaded", () => {
    const recovery = source.slice(
      source.indexOf("async function recoverNativeHostAfterRendererLoad"),
      source.indexOf("async function openMullvadOnStartup"),
    );
    expect(recovery).toContain('if (recovered?.status !== "resized") return;');
    expect(recovery).toContain('recovered.mode === "existingNativeCompanion" ? "existingSession" : "dedicated"');
    expect(recovery).toMatch(/activeHomeAppId = app\.id;[\s\S]*?activeService = service;[\s\S]*?route = "service";/);

    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(bootstrap.indexOf("if (linkedServices) services = linkedServices;")).toBeGreaterThanOrEqual(0);
    expect(bootstrap.indexOf("void recoverNativeHostAfterRendererLoad();")).toBeGreaterThan(
      bootstrap.indexOf("if (linkedServices) services = linkedServices;"),
    );
  });

  it("truthfully limits capture resistance to OSL's private Protect layer", () => {
    expect(source).toContain('aria-label="Open Discord"');
    expect(source).toContain('id="discord-existing-session"');
    expect(source).toContain(">Use existing account</button>");
    expect(source).toContain(">Use separate account</button>");
    expect(source).toContain("Discord itself is not capture-resistant; use Protect for OSL's private layer.");
  });

  it("opens a chosen native account in one click from the app page", () => {
    const binding = source.slice(
      source.indexOf("function bindSavedAccountControls"),
      source.indexOf("async function ensureFirefoxForProtectedImport"),
    );
    expect(binding).toContain('route === "service" && activeHomeAppId === appId && activeService');
    expect(binding).toContain("void setupEmbeddedApp()");
    expect(binding).toContain('finishNativeAccountChoice("discord")');
    expect(binding).toContain('finishNativeAccountChoice("telegram")');
    expect(binding).toContain('finishNativeAccountChoice("signal")');
    expect(binding).toContain('finishNativeAccountChoice("whatsapp")');
  });

  it("automatically reopens a closed dedicated app once without changing existing-session behavior", () => {
    const validation = source.slice(
      source.indexOf("async function validateNativeSurfaces"),
      source.indexOf("function scheduleNativeHostRealignment"),
    );
    const existingBranch = validation.slice(
      validation.indexOf('if (activeNativeHostMode === "existingSession")'),
      validation.indexOf("} else {"),
    );
    expect(existingBranch).toContain("Use Bring forward or reopen.");
    expect(existingBranch).not.toContain("reopenActiveNativeCompanion");
    expect(validation.match(/await reopenActiveNativeCompanion\(\);/g)).toHaveLength(1);
    expect(validation).toContain("closed and could not be reopened safely.");
  });

  it("installs PTB on the first dedicated click and keeps cold-start recovery bounded", () => {
    const hostCommand = nativeMain.slice(
      nativeMain.indexOf("async fn host_native_app_window"),
      nativeMain.indexOf("fn resize_native_app_window"),
    );
    expect(hostCommand).toContain("native_apps::install_discord_dedicated_channel().is_ok()");
    expect(hostCommand).toMatch(/ChannelNotOwned[\s\S]*NoChannelAvailable[\s\S]*AppNotInstalled/);
    expect(hostCommand).toContain("std::time::Duration::from_secs(180)");
    expect(hostCommand).toMatch(/WindowNotFound[\s\S]*ProfileInitializationFailed/);
  });

  it("keeps a safe fixed failure stage visible without exposing Discord data", () => {
    expect(source).toContain('nativeProtectFailureNotice = "Protection stopped: chat security settings are unavailable."');
    expect(source).toContain('?? "Protection stopped: bring OSL or Discord forward, clear the Discord composer, then retry."');
    expect(source).toContain("setNativeDiscordProtectedOverlayOpenForQa");
    expect(source).toContain('role="status">${escapeHtml(nativeProtectFailureNotice)}');
    expect(source).not.toContain("nativeProtectFailureNotice = failure");
  });

  it("never opens the WebView geometry sheet for the native branch", () => {
    const nativeBranch = source.slice(source.indexOf('if (activeNativeHostId === "discord")'), source.indexOf("if (!activeEmbeddedHost) return;"));
    expect(nativeBranch).toContain("setNativeDiscordProtectedOverlayOpen");
    expect(nativeBranch).not.toContain("setLocalProtectedSheetOpen");
  });

  it("allows verified OSL relay in the background while keeping Discord placement foreground-gated", () => {
    expect(nativeMain).not.toContain("Bring OSL Privacy or the trusted Discord window forward first");
    expect(nativeMain).toContain("current_discord_service_host(&owner)");
    expect(nativeMain).toContain("target.generation != current.generation");
    expect(nativeMain).toContain("native_discord_scope_binding(&app)?");
    expect(nativeAdapter).toContain("foreground_is_exact_discord(target.window)");
    expect(nativeAdapter).toContain("SetForegroundWindow(target.window as _)");
  });

  it("arms close containment before native cleanup and keeps host locks off the window thread", () => {
    const closePath = nativeMain.slice(
      nativeMain.indexOf("if let tauri::WindowEvent::CloseRequested"),
      nativeMain.indexOf(".setup(|app|"),
    );
    const watchdog = closePath.indexOf("std::thread::spawn(move ||");
    const blockingCleanup = closePath.indexOf("tauri::async_runtime::spawn_blocking(move ||");
    const terminate = closePath.indexOf(".terminate();");
    expect(watchdog).toBeGreaterThanOrEqual(0);
    expect(blockingCleanup).toBeGreaterThan(watchdog);
    expect(terminate).toBeGreaterThan(blockingCleanup);
    expect(closePath.slice(0, blockingCleanup)).not.toContain(".terminate();");
  });
});
