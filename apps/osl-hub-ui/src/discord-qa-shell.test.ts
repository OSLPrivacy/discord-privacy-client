import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = fs.readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const cargo = fs.readFileSync(new URL("../../osl-hub/Cargo.toml", import.meta.url), "utf8");
const qaIdentity = fs.readFileSync(new URL("../../osl-hub/src/discord_qa_identity.rs", import.meta.url), "utf8");
const nativeMain = fs.readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");

describe("compile-time Discord QA shell", () => {
  it("is opt-in and opens only the existing Discord session", () => {
    expect(source).toContain('import.meta.env.VITE_OSL_DISCORD_QA_SHELL === "1"');
    expect(source).toContain('setNativeSessionMode("discord", "existingSession")');
    expect(source).toContain('await openNativeHostedApp(app, service, "discord")');
    expect(source).not.toContain("attempt < 3");
    expect(source).not.toContain("after bounded retries");
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
    expect(source).toContain("await openNativeDiscordProtection(verifiedStablePeers[0].personId)");
    expect(source).toContain('discordQaOverlayState = overlayOpened ? "ready" : "failed"');
  });

  it("keeps the exact native overlay-open failure in the QA semantic status", () => {
    expect(source).toContain("await setNativeDiscordProtectedOverlayOpenForQa(approvedContext.contextToken)");
    expect(source).toContain('discordQaOverlayState === "failed" && nativeProtectFailureNotice');
    expect(source).toContain("nativeProtectFailureNotice = qaOverlayResult?.error");
  });

  it("removes setup navigation without hiding the protection control", () => {
    expect(styles).toContain(".discord-qa-shell .app-launcher-strip");
    expect(styles).toContain(".discord-qa-shell .workspace-settings");
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
