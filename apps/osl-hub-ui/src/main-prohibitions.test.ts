import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { homeAppsFromServices, parseLinkedServices } from "./services";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const localProtectedSheetSource = readFileSync(new URL("./local-protected-sheet.ts", import.meta.url), "utf8");

const originalAppRoster = [
  "discord", "telegram", "instagram", "snapchat", "x", "messenger", "signal", "whatsapp",
  "gmail", "outlook", "proton", "yahoo", "aol", "gmx", "maildotcom", "icloud",
] as const;
const unsupportedOriginalApps = originalAppRoster.filter((id) => id !== "discord");

function linkedServiceFixture(): unknown[] {
  const ids = ["discord", "telegram", "instagram", "snapchat", "email", "x", "messenger", "signal", "whatsapp", "slack", "linkedin", "teams"];
  return ids.map((id, sidebarOrder) => ({
    id,
    displayName: id,
    sidebarGlyph: id.slice(0, 2).toUpperCase(),
    sidebarOrder,
    category: id === "slack" || id === "linkedin" || id === "teams" ? "enterprise" : "consumer",
    launchState: "available",
    supportsNativePreview: true,
    supportsProtectedPreview: true,
    accounts: [],
  }));
}

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

function countOccurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
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
    expect(protectedDraft.indexOf('setup.sendMode === "manual"'))
      .toBeLessThan(protectedDraft.indexOf("navigator.clipboard.writeText(prepared.capsule)"));
    expect(localProtectedSheetSource).not.toContain("void sendMode");
    expect(localProtectedSheetSource).toContain('manualMode ? "Encrypt & prepare"');
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

  it("P-28 and P-48 offer only supported app tiles as available", () => {
    const selectedNative = functionSource("selectedNativeApps", "hasSelectedNativeAppChoice");
    const detected = functionSource("detectedAppsContent", "installMissingAppsContent");
    const guide = functionSource("serviceGuideContent", "settingsContent");
    const binding = functionSource("bindSavedAccountControls", "bindBrowserImportControls");
    const topStrip = functionSource("appLauncherStrip", "simpleDeviceStatusMarkup");
    const home = functionSource("workspaceContent", "parsedEnclaveAudiences");
    const inbox = functionSource("inboxDestinationContent", "activityPrimaryActionPlan");
    const connections = functionSource("connectionsDestinationContent", "privacyPrimaryAction");
    const appSettings = functionSource("serviceAccountsSettingsContent", "scanPrivacyExport");
    const launcher = functionSource("openHomeAppFromLauncher", "startBackgroundInstall");
    const apps = homeAppsFromServices(parseLinkedServices(linkedServiceFixture())!);
    const launchApps = apps.filter((app) => app.visibility === "launch");

    expect(source).toContain('const supportedNativeAppIds = new Set<NativeAppId>(["discord"])');
    expect(launchApps.map((app) => app.id)).toEqual([...originalAppRoster]);
    expect(launchApps.filter((app) => app.launchState === "available").map((app) => app.id)).toEqual(["discord"]);
    expect(launchApps.filter((app) => app.launchState === "comingSoon").map((app) => app.id)).toEqual([...unsupportedOriginalApps]);
    expect(launchApps.filter((app) => app.setupEligible).map((app) => app.id)).toEqual(["discord"]);
    expect(selectedNative).toContain("supportedNativeAppIds.has(app.id)");
    expect(detected).toContain('nativeSessionModeSettingChoices("discord", "Discord")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("telegram", "Telegram")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("signal", "Signal")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("whatsapp", "WhatsApp")');
    expect(detected).not.toContain('nativeSessionModeSettingChoices("outlook", "Outlook")');
    expect(guide).toContain("supportedNativeAppIds.has(activeHomeAppId as NativeAppId)");
    expect(binding).not.toContain('finishNativeAccountChoice("telegram")');
    expect(countOccurrences(source, "data-home-app=")).toBe(5);

    expect(topStrip).toContain("configuredTopStripApps(homeAppsFromServices(services), homeTileOrder)");
    expect(home).toContain('app.launchState === "available" && rememberedHomeApps.has(app.id)');
    expect(home).toContain('available ? `data-home-app="${app.id}"` : ""');
    expect(inbox).toContain('app.visibility === "launch" && app.launchState === "available" && app.linked');
    expect(connections).toContain('const action = app.launchState === "available"');
    expect(appSettings).toContain('const action = app.launchState === "available"');

    const guard = launcher.indexOf('if (!app || !service || app.launchState !== "available")');
    expect(guard).toBeGreaterThanOrEqual(0);
    for (const call of [
      "openNativeHostedApp(app, service, nativeIntent)",
      "openBrowserCompanionApp(app, service)",
      "openServiceRoute(service, app.provider, app.id, true)",
      "openEmbeddedApp(app, service)",
      "setupEmbeddedApp()",
    ]) {
      expect(guard, `${call} must stay behind the launchState guard`)
        .toBeLessThan(launcher.indexOf(call));
    }
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
