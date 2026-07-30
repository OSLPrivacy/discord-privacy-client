import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = fs.readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const cargo = fs.readFileSync(new URL("../../osl-hub/Cargo.toml", import.meta.url), "utf8");
const qaIdentity = fs.readFileSync(new URL("../../osl-hub/src/discord_qa_identity.rs", import.meta.url), "utf8");
const nativeMain = fs.readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const nativeHost = fs.readFileSync(new URL("../../osl-hub/src/native_window_host.rs", import.meta.url), "utf8");
const hubCapability = fs.readFileSync(new URL("../../osl-hub/capabilities/hub.json", import.meta.url), "utf8");

describe("compile-time Discord QA shell", () => {
  it("is opt-in and opens only the existing Discord session", () => {
    expect(source).toContain('import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"');
    expect(source).toContain('setNativeSessionMode("discord", "existingSession")');
    expect(source).toContain('await openNativeHostedApp(app, service, "discord")');
    expect(source).toContain('id="discord-qa-toggle-composer"');
    expect(source).toContain('data-open-burn="account"');
    expect(source).toContain("async function runDiscordQaOneClick()");
    expect(source).toContain("if (discordQaExistingSession) {\n      // The native tether has already verified");
    expect(source).toContain('!(discordQaShell && activeHomeAppId === "discord")');
    expect(source).toContain('"Recover hosted Discord QA window"');
    expect(source).toContain('recovered.mode === "existingNativeCompanion"');
    expect(source).not.toContain("after bounded retries");
  });

  it("automatically opens the verified installed Discord route as soon as QA is ready", () => {
    const bootstrapStart = source.indexOf("async function bootstrap()");
    const bootstrap = source.slice(
      bootstrapStart,
      source.indexOf('window.addEventListener("keydown"', bootstrapStart),
    );
    const startup = source.slice(
      source.indexOf("async function startDiscordQaShell()"),
      source.indexOf("async function openMullvadOnStartup()"),
    );

    expect(bootstrap).toContain("if (discordQaShell) {");
    expect(bootstrap).toContain("void startDiscordQaShell()");
    expect(startup).toContain("!core.readiness.unlocked");
    expect(startup).toContain("discordQaShellStarting");
    expect(startup).toContain("discordQaShellStarted");
    expect(startup).toContain('setNativeSessionMode("discord", "existingSession")');
    expect(startup).toContain('await openNativeHostedApp(app, service, "discord")');
    expect(startup).not.toContain("click()");
    expect(source).toContain("discordQaShellRetryCount < discordQaShellMaxAutomaticRetries");
    expect(source).toContain("const discordQaShellMaxAutomaticRetries = 2");
  });

  it("never routes a disposable QA identity through consumer onboarding", () => {
    const bootstrapStart = source.indexOf("async function bootstrap()");
    const bootstrap = source.slice(
      bootstrapStart,
      source.indexOf('window.addEventListener("keydown"', bootstrapStart),
    );
    const qaRoute = bootstrap.indexOf("if (discordQaShell) {");
    const setupRoute = bootstrap.indexOf('core.readiness.bootstrapStatus === "setupRequired"');

    expect(qaRoute).toBeGreaterThan(-1);
    expect(qaRoute).toBeLessThan(setupRoute);
    expect(bootstrap).toContain('route = "service"');
    expect(bootstrap).toContain("serviceGuideStep = null");
    expect(bootstrap).toContain("void startDiscordQaShell()");
    expect(bootstrap).toContain("if (!discordQaShell) renderNow()");
  });

  it("reuses the bounded native claim/reopen path across exact official Discord channels", () => {
    const claim = nativeHost.slice(
      nativeHost.indexOf("unsafe fn claim_existing_host("),
      nativeHost.indexOf("unsafe fn claim_or_relaunch_existing_host("),
    );
    const reopen = nativeHost.slice(
      nativeHost.indexOf("unsafe fn claim_or_relaunch_existing_host("),
      nativeHost.indexOf("unsafe extern \"system\" fn enum_existing_window"),
    );

    expect(claim).toContain("existing_discord_channel_executables");
    expect(claim).toContain("trust_existing_executable");
    expect(claim).toContain("take_existing_candidate");
    expect(reopen).toContain("claim_existing_host");
    expect(reopen).toContain("should_relaunch_existing_session");
    expect(reopen).toContain("EXISTING_SESSION_DISCOVERY_TIMEOUT");
    expect(reopen).toContain("verify_executable");
    expect(reopen).not.toContain("http");
  });

  it("publishes one stable semantic host state and blocks clicks while starting", () => {
    expect(source).toContain('type DiscordQaHostState = "starting" | "hosted" | "failed"');
    expect(source).toContain('id="discord-qa-host-state"');
    expect(source).toContain('data-host-state="${discordQaHostState}"');
    expect(source).toContain('discordQaHostState === "starting"');
    expect(source).toContain('disabled aria-disabled=\\"true\\"');
    expect(source).toContain("serviceGuideStep = 0");
  });

  it("automatically opens Protect only for exactly one verified stable peer", () => {
    expect(source).toContain('type DiscordQaOverlayState = "starting" | "ready" | "failed"');
    expect(source).toContain('id="discord-qa-overlay-state"');
    expect(source).toContain('data-overlay-state="${discordQaOverlayState}"');
    expect(source).toContain("const verifiedStablePeers = hubPeople.filter((person) => person.safetyNumberVerified && !person.pendingKeyChange)");
    expect(source).toContain("verifiedStablePeers.length !== 1");
    expect(source).toContain("await openSoleVerifiedDiscordQaOverlay()");
    expect(source).toContain("overlayOpened = await openNativeDiscordProtection(solePeer.personId)");
    expect(source).toContain('discordQaOverlayState = overlayOpened ? "ready" : "failed"');
    expect(source).toContain("attempt < 20 && !overlayOpened");
    expect(source).toContain("window.setTimeout(resolve, 500)");
    expect(source).toContain("verified Discord QA overlay did not open within its bounded retry window");
    expect(source).toContain("const qaProbe = await runNativeDiscordHeadlessQa()");
    expect(source).toContain("qaProbe.personToPersonE2ee");
    expect(source).toContain("qaProbe.deliveredToOslInbox");
    expect(nativeMain).toContain('caller.label() != native_discord_overlay::OVERLAY_LABEL && caller.label() != "main"');
    expect(hubCapability).toContain('"allow-send-native-discord-qa-probe"');
    expect(nativeMain).toContain("overlay_state.wait_until_ready(");
    expect(nativeMain.indexOf("overlay_state.wait_until_ready("))
      .toBeLessThan(nativeMain.indexOf("Ok(true)", nativeMain.indexOf("overlay_state.wait_until_ready(")));
    expect(source.indexOf("const qaProbe = await runNativeDiscordHeadlessQa()"))
      .toBeLessThan(source.indexOf("void startDiscordQaVisualOverlayAttempt()"));
    expect(source.indexOf("void startDiscordQaVisualOverlayAttempt()"))
      .toBeLessThan(source.indexOf("await pollNativeDiscordHeadlessQa()"));
  });

  it("preserves a claimed host and permits only bounded automatic recovery", () => {
    expect(source).toContain("const discordQaShellMaxAutomaticRetries = 2");
    expect(source).toContain("discordQaShellComplete");
    expect(source).toContain('discordQaHostState = hostClaimed ? "hosted" : "failed"');
    expect(source).toContain("discordQaShellStarted = hostClaimed");
    expect(source).toContain("discordQaShellRetryCount < discordQaShellMaxAutomaticRetries");
    expect(source).toContain("window.setTimeout(() => void startDiscordQaShell(), 1_000)");
  });

  it("uses modifier-free F11 to reopen only the sole verified Discord overlay in QA", () => {
    expect(source).toContain("async function openSoleVerifiedDiscordQaOverlay()");
    expect(source).toContain('activeHomeAppId !== "discord"');
    expect(source).toContain('activeNativeHostId !== "discord"');
    expect(source).toContain('activeNativeHostMode !== "existingSession"');
    expect(source).toContain("verifiedStablePeers.length !== 1");
    expect(source).toContain("peerProtectedSheet.personId === solePeer.personId");
    expect(source).toContain("await setNativeDiscordProtectedOverlayOpenForQa(activeToken)");
    expect(source).toContain('if (discordQaShell) {\n    void openDiscordQaComposer();');
    expect(source).toContain("void openDiscordQaComposerAfterHostReady()");
    expect(source).toContain("void toggleDesktopFullscreen().catch(() => undefined)");
  });

  it("keeps only the claimed QA Discord carrier aligned and stops outside its route", () => {
    expect(source).toContain("createDiscordQaGeometryKeeper");
    expect(source).toContain('route === "service"');
    expect(source).toContain('activeHomeAppId === "discord"');
    expect(source).toContain('activeNativeHostId === "discord"');
    expect(source).toContain('activeNativeHostMode === "existingSession"');
    expect(source).toContain("discordQaGeometryKeeper.start()");
    expect(source).toContain("discordQaGeometryKeeper.stop()");
    expect(source).toContain('"Keep Discord aligned"');
    expect(source).not.toContain("setInterval(focusNativeAppWindow");
  });

  it("keeps the exact native overlay-open failure in the QA semantic status", () => {
    expect(source).toContain("await setNativeDiscordProtectedOverlayOpenForQa(approvedContext.contextToken)");
    expect(source).toContain('discordQaOverlayState === "failed" && nativeProtectFailureNotice');
    expect(source).toContain("nativeProtectFailureNotice = qaOverlayResult?.error");
  });

  it("removes setup navigation without hiding the QA protection controls", () => {
    expect(styles).toContain(".discord-qa-shell .app-launcher-strip");
    expect(styles).toContain(".discord-qa-shell .workspace-settings");
    expect(styles).toContain(".discord-qa-shell .discord-qa-header-controls");
    expect(styles).not.toContain(".discord-qa-shell .local-protected-toggle");
  });

  it("pairs the renderer shell with a compile-time native device-bound identity", () => {
    expect(cargo).toContain('discord-qa-shell = []');
    expect(nativeMain).toContain('#[cfg(feature = "discord-qa-shell")]');
    expect(nativeMain).toContain("install_device_bound_storage_key");
    expect(nativeMain).toContain("ensure_disposable_identity");
    expect(qaIdentity).toContain("persistent_sealer()");
    expect(qaIdentity).toContain("crypto::random::random_bytes");
    expect(qaIdentity).toContain("password_marker.json");
    expect(qaIdentity).not.toMatch(/password\s*=\s*["']/i);
    expect(qaIdentity).not.toContain("reqwest");
  });
});
