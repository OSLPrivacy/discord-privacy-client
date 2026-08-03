import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("B0-07b main.ts prohibitions", () => {
  it("P-13 preserves Manual mode and exposes it to send-mode consumers", () => {
    const onboarding = functionSource("bindOnboarding", "completeOnboarding");
    const firstRun = functionSource("balancedFirstRunSetup", "completeSixStepOnboarding");
    const settings = functionSource("sendingSettingsContent", "privacySettingsContent");
    const protectedDraft = functionSource("prepareLocalProtectedDraft", "openLocalProtectedCapsule");

    expect(onboarding).toContain('["manual", "clipboard", "double"].includes(mode)');
    expect(onboarding).not.toContain('setup.sendMode === "manual") setup.sendMode = "clipboard"');
    expect(firstRun).toContain("const sendMode = state.sendMode");
    expect(firstRun).not.toContain('state.sendMode === "manual" ? "clipboard" : state.sendMode');
    expect(settings).toContain('["manual", "Manual", "Prepare only; you place and send"]');
    expect(settings).not.toContain('setup.sendMode === "manual" ? "clipboard" : setup.sendMode');
    expect(protectedDraft).toContain('setup.sendMode === "manual"');
    expect(source).toContain("localProtectedSheetMarkup(localProtectedSheet, setup.sendMode)");
  });

  it("P-05 renders screenshot protection from the effective platform state", () => {
    const onboarding = functionSource("bindOnboarding", "completeOnboarding");
    const startup = functionSource("startReadyWorkspaceLoads", "bootstrap");

    expect(onboarding).toContain("await setScreenshotProtection(windowCaptureEnabled).catch(() => false)");
    expect(onboarding).toContain("screenshotProtectionEnabled = windowCaptureEnabled && captureProtectionEnforced()");
    expect(onboarding).not.toContain("screenshotProtectionEnabled = await setScreenshotProtection");
    expect(startup).toContain("screenshotProtectionEnabled = windowCaptureEnabled && captureProtectionEnforced()");
    expect(startup).not.toContain("screenshotProtectionEnabled = windowCaptureEnabled ? applied : false");
  });

  it("P-28 and P-48 offer only supported native app tiles", () => {
    const selectedNative = functionSource("selectedNativeApps", "hasSelectedNativeAppChoice");
    const detected = functionSource("detectedAppsContent", "installMissingAppsContent");
    const guide = functionSource("serviceGuideContent", "settingsContent");
    const binding = functionSource("bindSavedAccountControls", "bindBrowserImportControls");

    expect(source).toContain('const supportedNativeAppIds = new Set<NativeAppId>(["discord"])');
    expect(selectedNative).toContain("supportedNativeAppIds.has(app.id)");
    expect(detected).toContain('nativeSessionModeSettingChoices("discord", "Discord")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("telegram", "Telegram")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("signal", "Signal")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("whatsapp", "WhatsApp")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("outlook", "Outlook")');
    expect(guide).toContain("supportedNativeAppIds.has(activeHomeAppId as NativeAppId)");
    expect(binding).not.toContain('finishNativeAccountChoice("telegram")');
  });

  it("P-34 states independent warning defaults", () => {
    const review = functionSource("reviewDefaultsOnboardingContent", "coverDraftSetupContent");
    const presets = functionSource("protectionPresetOnboardingContent", "mullvadSetupContent");

    expect(review).toContain('"Warn before unprotected sends"');
    expect(review).toContain('"On", true');
    expect(review).toContain('"Warn before protected sends"');
    expect(review).toContain('"Warn before protected sends", "Extra warning before already-protected handoff.", "Off"');
    expect(presets).toContain("<strong>Warn before unprotected sends</strong>");
    expect(presets).toContain('${statusTag("On", "active")}');
    expect(presets).toContain("<strong>Warn before protected sends</strong>");
    expect(presets).toContain('${statusTag("Off")}');
    expect(`${review}${presets}`).not.toContain("Warn before risky sends");
  });

  it("P-18 does not claim unsupported OSL Mail acknowledgement confirms deletion", () => {
    const binding = functionSource("bindWorkspace", "openHomeAppFromLauncher");

    expect(binding).toContain("Retrieval acknowledged locally; server deletion was not requested or confirmed by this build");
    expect(binding).not.toContain("Server deletion was not confirmed");
  });
});
