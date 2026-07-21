import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("clean onboarding sign in", () => {
  it("removes the redundant account-access strip", () => {
    expect(source).not.toContain("ACCOUNT ACCESS");
    expect(source).not.toContain('class="onboarding-brand"');
  });

  it("uses account state to make create, finish, or unlock the one clear primary action", () => {
    expect(source).toContain('const partialIdentity = core.readiness.identityLoaded && core.readiness.bootstrapStatus === "setupRequired"');
    expect(source).toContain('const primaryRoute: OnboardingRoute = partialIdentity ? "create" : returning ? "unlock" : "create"');
    expect(source).toContain('data-onboarding="${primaryRoute}"');
    expect(source).toContain('partialIdentity ? "Finish setup"');
    expect(source).toContain('class="signin-link" data-onboarding="import"');
    expect(source).toContain("Unlock first to add another identity in Settings.");
    expect(source).not.toContain('class="button signin-create" data-onboarding="create"');
    expect(source).not.toContain("Sign in to OSL");
    expect(source).not.toContain("Welcome back");
    expect(source).not.toContain("Open your private OSL workspace on this device.");
    expect(source).not.toContain("Your service passwords stay on each service's own sign-in page.");
  });

  it("keeps password unlock to one field and one action", () => {
    expect(source).toContain('class="password-form unlock-form"');
    expect(source).toContain('class="unlock-logo-stage"');
    expect(source).toMatch(/class="unlock-logo-stage"[\s\S]*?src="\$\{oslVectorLogoUrl\}"[\s\S]*?>Enter your password<\/h1>/);
    expect(source).toContain(">Enter your password</h1>");
    expect(source).toContain('id="identity-password-submit" type="submit" disabled>Unlock</button>');
    expect(styles).toMatch(/\.unlock-form\s*\{\s*gap:\s*12px;/);
    expect(styles).toMatch(/\.unlock-form \.unlock-error:empty\s*\{\s*display:\s*none;/);
    expect(styles).toMatch(/\.onboarding-unlock \.unlock-card > \.text-back\s*\{\s*margin-top:\s*0;/);
  });

  it("uses a crisp accessible password visibility control", () => {
    const iconStart = source.indexOf("function passwordEyeIcon");
    const icon = source.slice(iconStart, source.indexOf("let services", iconStart));
    expect(icon).toContain('viewBox="0 0 20 20"');
    expect(icon).toContain('<circle cx="10" cy="10" r="2.25"/>');
    expect(styles).toMatch(/\.password-input-row \.password-eye\s*\{[\s\S]*?width:\s*44px;[\s\S]*?min-height:\s*44px;/);
    expect(styles).toMatch(/\.password-eye svg\s*\{[^}]*stroke-linecap:\s*round;/s);
  });

  it("keeps the welcome surface compact and centered", () => {
    expect(styles).toMatch(/\.onboarding-welcome\s*\{[^}]*width:\s*min\(440px/s);
    expect(styles).toMatch(/\.signin-card\s*\{[^}]*text-align:\s*center/s);
    expect(source).toContain('icons/icon-cyan.png');
  });

  it("keeps the custom titlebar unbranded and fully draggable beside accessible controls", () => {
    const titlebar = functionSource("desktopTitlebar", "bindDesktopTitlebar");
    expect(titlebar).toContain('class="desktop-drag-region" data-tauri-drag-region');
    expect(titlebar).not.toMatch(/>OSL<|<img|class="desktop-title"/);
    expect(titlebar).toContain('aria-label="Minimize"');
    expect(titlebar).toContain('aria-label="Maximize or restore"');
    expect(titlebar).toContain('aria-label="Close"');
    expect(titlebar).toContain("activeNativeHostId");
    expect(titlebar).toContain('disabled title="Unavailable while a companion window is open"');
    expect(styles).toMatch(/\.desktop-drag-region\s*\{[^}]*flex:\s*1 1 auto/s);
    expect(styles).toMatch(/\.window-controls button:disabled\s*\{[^}]*pointer-events:\s*none;/s);
  });

  it("does not stack window-control listeners during no-op refreshes", () => {
    const binding = functionSource("bindDesktopTitlebar", "renderOnboarding");
    expect(binding).toContain('button.dataset.windowControlBound === "true"');
    expect(binding).toContain('button.dataset.windowControlBound = "true"');
  });
});

describe("fresh-account continuation", () => {
  it("persists and resumes every current post-account setup step without accepting legacy app routes", () => {
    const pending = functionSource("pendingOnboardingRoute", "beginServiceOnboarding");
    const renderOnboarding = functionSource("renderOnboarding", "onboardingContent");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    for (const route of ["pro", "privacy", "sending", "cover", "passwords", "burnpass", "mullvad", "browser", "tutorial"]) {
      expect(pending).toContain(`pending === "${route}"`);
    }
    expect(pending).not.toContain('pending === "apps"');
    expect(pending).not.toContain('pending === "detected"');
    expect(pending).not.toContain('pending === "install"');
    expect(pending).toContain("localStorage.removeItem(onboardingResumeStorageKey)");
    expect(renderOnboarding).toContain("persistCurrentOnboardingRoute()");
    expect(source).not.toContain('pendingOnboardingRoute() ?? "mullvad"');
    expect(bootstrap).toContain('pendingOnboardingRoute() ?? "pro"');
  });

  it("combines detected and remaining apps in one chooser", () => {
    const choice = functionSource("tutorialContent", "selectedNativeApps");
    expect(choice).toContain("Choose apps");
    expect(choice).toContain("Detected");
    expect(choice).toContain("Other apps");
    expect(choice).toContain("app.linked");
    expect(choice).toContain('native?.availability === "installed"');
    expect(choice).toContain("savedAccountsReady && importedFirefoxHomeAppIds.has(app.id)");
    expect(choice).toContain('data-onboarding-app-choice="${app.id}"');
    expect(choice).toContain("Nothing opens during setup");
    expect(choice).toContain('nativeCatalogBusy ? "Checking Windows…" : "Continue"');
  });

  it("routes browser import directly through the combined chooser to Home", () => {
    const previous = functionSource("previousSetupRoute", "bindOnboarding");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const browserBinding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(browserBinding).toMatch(/Browser import finished[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(browserBinding).toMatch(/#continue-browser-import[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(browserBinding).not.toContain("retry the same source once");
    expect(browserBinding).toMatch(/catch \(failure\)[\s\S]*?browserImportQueue = \[\][\s\S]*?persistBrowserImportQueue\(\)/);
    expect(browserBinding).toMatch(/activeOperation[\s\S]*?finishProtectedBrowserImport[\s\S]*?await activeOperation[\s\S]*?finishProtectedBrowserImport/);
    expect(browserBinding).toContain("selected browser queue could not be completed safely");
    expect(browserBinding).not.toContain("manually in Firefox");
    expect(previous).toContain('tutorial: "browser"');
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?await completeOnboarding\(\)/);
  });

  it("persists Home choices without installing, opening, or adopting native sessions", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const intent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    expect(intent).toContain("selectedOnboardingAppsStorageKey");
    expect(intent).toContain("selectedOnboardingApps");
    expect(intent).not.toContain("savedNativeApps");
    expect(intent).not.toContain("persistSavedAccountPreferences");
    expect(intent).not.toContain("installNativeApp");
    expect(intent).not.toContain("openNativeHostedApp");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?completeOnboarding\(\)/);
  });

  it("never infers native-app routing from an unknown catalog", () => {
    const completeness = functionSource("isCompleteNativeCatalog", "hasSelectedInstalledNativeApps");
    const chooser = functionSource("ensureNativeCatalogForAppChoice", "selectedNativeAppIntent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(completeness).toContain("catalog.length === supportedNativeAppIds.size");
    expect(completeness).toContain("ids.size === supportedNativeAppIds.size");
    expect(completeness).toContain("every((appId) => ids.has(appId))");
    expect(chooser).toContain("hasSelectedNativeAppChoice()");
    expect(chooser).not.toContain("nativeAppsReady");
    expect(chooser).toContain('withNativeDeadline(loadNativeApps(), "Check Windows apps", nativeCatalogDecisionDeadlineMs)');
    expect(chooser).toContain("if (!isCompleteNativeCatalog(catalog))");
    expect(chooser).toContain("Couldn’t check Windows apps. Try again.");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?await ensureNativeCatalogForAppChoice\(\)[\s\S]*?persistCombinedHomeChoices\(\)/);
  });

  it("shows every installed native app while requiring isolation support only for separate profiles", () => {
    const installedChoice = functionSource("hasSelectedInstalledNativeApps", "hasSelectedMissingNativeApps");
    const nativeSelection = functionSource("selectedNativeAppIntent", "detectedAppsContent");
    const detected = functionSource("detectedAppsContent", "installMissingAppsContent");
    const discordChoices = functionSource("discordSessionModeChoices", "detectedAppsContent");
    const telegramChoices = functionSource("telegramSessionModeChoices", "detectedAppsContent");
    expect(installedChoice).toContain('app.availability === "installed" && app.isolatedProfileAvailable');
    expect(nativeSelection).toContain('if (nativeId === "discord" || existingNativeSessionRequested(appId)) return nativeId;');
    expect(nativeSelection).toContain('savedAccountMode === "use" && savedNativeApps.has(nativeId) && catalogApp?.availability === "installed" && catalogApp.isolatedProfileAvailable');
    expect(nativeSelection).toContain("onboardingServiceSetup");
    expect(nativeSelection).toContain("selectedOnboardingApps.has(appId)");
    expect(nativeSelection).toContain('savedAccountMode !== "clean"');
    expect(nativeSelection).toContain('nativeSessionModeForApp(nativeId) === "dedicated"');
    expect(detected).toContain('selectedNativeApps().filter((app) => app.availability === "installed")');
    expect(discordChoices).toContain('data-discord-session-mode="dedicated"');
    expect(discordChoices).toContain('data-discord-session-mode="existingSession"');
    expect(discordChoices).toContain(">Current account</strong>");
    expect(discordChoices).toContain(">Separate account</strong>");
    expect(detected).toContain("discordSessionModeChoices()");
    expect(telegramChoices).toContain('data-telegram-session-mode="existingSession"');
    expect(telegramChoices).toContain('data-telegram-session-mode="dedicated"');
    expect(telegramChoices).toContain(">Current account</strong>");
    expect(telegramChoices).toContain(">Separate account</strong>");
    expect(detected).toContain("telegramSessionModeChoices()");
    expect(detected).toContain("signalSessionModeChoices()");
    expect(detected).toContain("whatsappSessionModeChoices()");
    expect(detected).toContain("outlookSessionModeChoices()");
    expect(source).toContain('data-signal-session-mode="existingSession"');
    expect(source).toContain('data-signal-session-mode="dedicated"');
    expect(source).toContain('aria-label="Signal account"');
    expect(source).toContain('data-whatsapp-session-mode="existingSession"');
    expect(source).toContain('data-whatsapp-session-mode="dedicated"');
    expect(source).toContain('if (supportedNativeAppIds.has(app.id as NativeAppId))');
    expect(source).toContain("A separate ${app.displayName} app account is unavailable");
    expect(detected).toContain("whatsappSessionModeChoices()");
    expect(source).toContain('appId === "whatsapp"');
    expect(source).toMatch(/serviceGuideContent[\s\S]*?activeHomeAppId === "telegram"[\s\S]*?telegramSessionModeChoices\(\)/);
    expect(source).toMatch(/const sessionChoices = onboardingServiceSetup[\s\S]*?\? ""/);
    expect(source).not.toContain("Uses your signed-in ${name} window without copying its session.");
    expect(source).not.toContain("${name} stays outside OSL capture protection.");
  });

  it("turns one app choice into a persisted Home tile without opening it", () => {
    const defaultIntent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(binding).toMatch(/data-onboarding-app-choice[\s\S]*?selectedOnboardingApps\.add\(appId\)/);
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?ensureNativeCatalogForAppChoice\(\)[\s\S]*?persistCombinedHomeChoices\(\)/);
    expect(defaultIntent).toContain("selectedOnboardingAppsStorageKey");
    expect(defaultIntent).not.toContain("savedNativeApps");
    expect(binding).not.toMatch(/#continue-app-choice[\s\S]*?openNativeHostedApp/);
  });

  it("records an explicit empty app choice instead of treating it as legacy no-preference state", () => {
    const persistence = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    const workspace = functionSource("workspaceContent", "peopleListMarkup");
    expect(persistence).toContain("hasExplicitOnboardingAppSelection = true");
    expect(workspace).toContain("hasExplicitOnboardingAppSelection || rememberedHomeApps.size");
    expect(workspace).toMatch(/hasExplicitOnboardingAppSelection \|\| rememberedHomeApps\.size[\s\S]*?launchableHomeApps\.filter/);
  });

  it("does not open each selected service during fresh setup", () => {
    const apps = functionSource("tutorialContent", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(apps).not.toContain("Connect your apps");
    expect(apps).not.toContain("Open selected app");
    expect(apps).not.toContain("data-connect-app-choice");
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?await completeOnboarding\(\)/);
  });

  it("offers multi-source selection behind one protected importer contract", () => {
    const tutorial = functionSource("tutorialContent", "selectedNativeApps");
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(tutorial).not.toContain("data-browser-import");
    expect(browser).toContain("Bring your logins");
    expect(browser).toContain("Optional.");
    expect(browser).toContain("browserLogo(browser.id)");
    expect(browser).toContain('<fieldset class="browser-detected-sources"');
    expect(browser).toContain("Choose browsers");
    expect(browser).toContain("Import all detected browsers");
    expect(browser).toContain("data-browser-select-all");
    expect(browser).toContain("Select everything, then press Import once");
    expect(browser).toContain('data-browser-source="${browser.id}"');
    expect(browser).toContain("selectedBrowserImportIds.has(browser.id)");
    expect(browser).toContain("Import from this browser");
    expect(browser).toContain('id="import-saved-accounts"');
    expect(browser).not.toContain('id="install-firefox"');
    expect(browser.match(/id="import-saved-accounts"/g)).toHaveLength(1);
    expect(browser).toContain('id="continue-browser-import" type="button" ${browserImportCancelling ? "disabled" : ""}>${secondaryLabel}');
    expect(browser).toContain('browserImportCancelling ? "Closing Firefox…" : queueActive ? "Cancel import" : "Not now"');
    expect(browser).not.toContain("Manual export");
    expect(browser).not.toContain("Prepare export in");
    expect(browser).not.toContain("How it works");
    expect(browser).toContain("Choose browsers");
    expect(browser).toContain('selectionReady ? "Import selected" : "Choose browsers"');
    expect(browser).not.toContain("Import selected · pending");
    expect(browser).toContain("Choose sources, press Import once, and stay in OSL.");
    expect(browser).toContain("Each selected source is handled automatically.");
    expect(browser).not.toContain("Done with");
    expect(browser).not.toMatch(/Finish \$\{escapeHtml\(currentName\)\}/);
    expect(browser).toContain('class="button primary" id="import-saved-accounts"');
    expect(browser.match(/class="button primary"/g)).toHaveLength(1);
    expect(binding).toContain("!protectedBrowserImportReady || selectedBrowserImportIds.size === 0");
    expect(binding).toContain("browserImportQueue = [...selectedBrowserImportIds]");
    expect(binding).toContain('querySelector<HTMLInputElement>("[data-browser-select-all]")');
    expect(binding).toContain("browserImports.filter((browser) => browser.installed)");
    expect(binding).not.toMatch(/data-browser-select-all[\s\S]*?render\(\)[\s\S]*?startProtectedBrowserImport\(\)/);
    expect(binding).toContain("beginProtectedBrowserImport(browserImportQueue)");
    expect(binding.match(/beginProtectedBrowserImport\(browserImportQueue\)/g)).toHaveLength(1);
    expect(binding).not.toContain("for (let index = 0; index < browserImportQueue.length; index += 1)");
    expect(binding).not.toContain("beginProtectedBrowserImport([currentSource])");
    expect(binding).toContain("await finishProtectedBrowserImport()");
    expect(binding).toContain("if (runEpoch !== browserImportRunEpoch) return");
    expect(binding).toMatch(/beginProtectedBrowserImport\(browserImportQueue\)[\s\S]*?finishProtectedBrowserImport\(\)[\s\S]*?savedAccountsReady = true[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?browserImportRunEpoch \+= 1[\s\S]*?finishProtectedBrowserImport\(\)/);
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).not.toContain("openBrowserImport(");
    expect(binding).not.toContain("window.confirm");
    expect(source).not.toContain("browserPasswordImportOptIn");
    expect(source).not.toContain("data-browser-password-import");
    expect(source).not.toContain("browser-password-import-opt-in");
  });

  it("keeps the normal-profile default browser behind explicit truthful consent", () => {
    const choices = functionSource("browserSessionModeChoices", "detectedAppsContent");
    const binding = functionSource("bindSavedAccountControls", "bindBrowserImportControls");
    expect(source).toContain('let useDefaultBrowserCompanion = localStorage.getItem("osl-default-browser-companion-v1") === "true"');
    expect(choices).toContain('data-browser-session-mode="isolatedOsl"');
    expect(choices).toContain('data-browser-session-mode="existingBrowser"');
    expect(choices).toContain("Browser account");
    expect(choices).toContain("New account");
    expect(binding).toContain('requested !== "isolatedOsl" && requested !== "existingBrowser"');
    expect(binding).toContain('useDefaultBrowserCompanion = requested === "existingBrowser"');
    expect(binding).toContain('localStorage.setItem("osl-default-browser-companion-v1", String(useDefaultBrowserCompanion))');
    expect(source).toContain('loadDefaultBrowserCompanionStatus(), "Check default browser"');
    expect(source).toContain("defaultBrowserCompanionStatus = currentBrowserCompanionStatus");
    expect(source).toContain("await detachDefaultBrowserCompanion().catch(() => undefined)");
    expect(source).toContain("resizeDefaultBrowserCompanion()");
    expect(source).toContain("focusDefaultBrowserCompanion()");
  });

  it("places the combined app choice immediately after browser import", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    const entry = functionSource("enterCombinedAppChoice", "persistCombinedHomeChoices");
    expect(browser).not.toContain("unsupported");
    expect(browser).not.toContain("unavailable in this build");
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?await enterCombinedAppChoice\(\)/);
    expect(binding).toContain("ensureFirefoxForProtectedImport()");
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).not.toContain("#install-firefox");
    expect(binding).not.toContain("window.confirm");
    expect(entry).not.toContain("selectedOnboardingApps.add");
    expect(entry).not.toContain("selectedOnboardingAppsStorageKey");
  });

  it("turns Import selected into one bounded action even when Firefox still needs installation", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    const readiness = functionSource("ensureFirefoxForProtectedImport", "refreshBrowserImportReadiness");
    expect(browser).toContain("browserImportBusy");
    expect(browser).toContain("Preparing protected import…");
    expect(browser).toContain("browserImportFailureNotice");
    expect(browser).toContain('role="alert"');
    expect(binding).toContain("await ensureFirefoxForProtectedImport()");
    expect(binding).toContain("const operation = beginProtectedBrowserImport(browserImportQueue)");
    expect(binding).toContain("const result = await operation.finally");
    expect(binding).toContain('browserImportFailureNotice = localActionError(failure, "Browser import did not start")');
    expect(binding.indexOf("await ensureFirefoxForProtectedImport()")).toBeLessThan(binding.indexOf("beginProtectedBrowserImport(browserImportQueue)"));
    expect(readiness).toContain("await installFirefox()");
    expect(readiness).toContain("firefoxInstallDecisionDeadlineMs");
    expect(readiness).toContain('status.availability === "installed"');
  });

  it("refreshes browser and Firefox readiness when the import page is entered or resumed", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const refresh = functionSource("refreshBrowserImportReadiness", "importIdentityForm");
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const advance = functionSource("advanceOnboardingConnection", "ensureNativeCatalogForAppChoice");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(browser).toContain("browserReadinessBusy");
    expect(refresh).toContain('loadBrowserImports(), "Refresh browsers"');
    expect(refresh).toContain('loadFirefoxStatus(), "Refresh Firefox"');
    expect(refresh).toContain("browserImports = catalog");
    expect(refresh).toContain("firefoxStatus = currentFirefoxStatus");
    expect(continuation).toContain("advanceOnboardingConnection(completedAppId)");
    expect(advance).toContain("void completeOnboarding()");
    expect(continuation).not.toContain("clearServiceOnboardingResume()");
    expect(bootstrap).toMatch(/onboardingRoute === "browser"[\s\S]*?refreshBrowserImportReadiness\(\)/);
  });

  it("keeps prior completed-import state scoped to the active OSL identity", () => {
    const key = functionSource("activeBrowserAccountsReadyStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(key).toContain("core.readiness.activeOslUserId");
    expect(key).toContain("encodeURIComponent(owner)");
    expect(key).toContain("return owner ?");
    expect(refresh).toContain("savedAccountsReady = key !== null");
    expect(binding).toMatch(/beginProtectedBrowserImport\(browserImportQueue\)[\s\S]*?localStorage\.setItem\(readyKey, "true"\)/);
    expect(binding).toMatch(/localStorage\.setItem\(readyKey, "true"\)[\s\S]*?savedAccountsReady = true/);
    expect(source).not.toMatch(/localStorage\.setItem\(savedAccountsReadyStorageKey\s*,/);
  });

  it("clears legacy pending import state without resuming an unmanaged browser", () => {
    const pendingKey = functionSource("activeBrowserImportPendingStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    expect(pendingKey).toContain("core.readiness.activeOslUserId");
    expect(pendingKey).toContain("encodeURIComponent(owner)");
    expect(pendingKey).toContain("browserImportPendingStorageKey");
    expect(refresh).not.toContain("browserMigrationAwaitingConfirmation");
    expect(source).toMatch(/function commitRender[\s\S]*?refreshActiveBrowserAccountsReady\(\)/);
    expect(binding).not.toContain("beginBrowserAccountImport()");
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?localStorage\.removeItem\(pendingKey\)/);
    expect(source).not.toMatch(/localStorage\.setItem\(browserImportPendingStorageKey\s*,/);
  });

  it("returns from each service to the remaining app queue before completion", () => {
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const workspace = functionSource("bindWorkspace", "ttlSeconds");
    const finishStart = workspace.indexOf('querySelector("#service-guide-finish")');
    const exitStart = workspace.indexOf('querySelector("#service-guide-exit")');
    const nativeBackStart = workspace.indexOf('querySelector("#native-app-back")');
    expect(finishStart).toBeGreaterThanOrEqual(0);
    expect(exitStart).toBeGreaterThan(finishStart);
    expect(nativeBackStart).toBeGreaterThan(exitStart);
    expect(continuation).toContain("advanceOnboardingConnection(completedAppId)");
    expect(workspace.slice(finishStart, exitStart)).toContain("advanceOnboardingConnection(activeHomeAppId)");
    expect(workspace.slice(exitStart, nativeBackStart)).toContain("clearServiceOnboardingResume()");
    expect(functionSource("completeOnboarding", "bindPasswordForm")).toContain("clearServiceOnboardingResume()");
  });

  it("shows the recovery title without the removed grey subtitle", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(recovery).toContain("Save your recovery kit");
    expect(recovery).not.toContain("OSL cannot retrieve these later");
    expect(recovery).not.toContain('class="compact-lead"');
  });

  it("continues from saved recovery material into optional Pro setup", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(recovery).toContain('id="copy-recovery-kit"');
    expect(recovery).toContain('recoverySavedAcknowledged ? "checked" : ""');
    expect(recovery).toContain('recoverySavedAcknowledged ? "" : "disabled"');
    expect(binding).toMatch(/#copy-recovery-kit[\s\S]*?navigator\.clipboard\.writeText\(kit\)[\s\S]*?Recovery kit copied — save it, then confirm below/);
    expect(binding).not.toMatch(/#copy-recovery-kit[\s\S]*?recoverySavedAcknowledged = true/);
    expect(binding).toMatch(/recoverySaved\?\.addEventListener\("change"[\s\S]*?recoverySavedAcknowledged = recoverySaved\.checked[\s\S]*?recoveryContinue\.disabled = !recoverySavedAcknowledged/);
    expect(binding).toMatch(/#recovery-continue[\s\S]*?recoveryBundle = null;[\s\S]*?recoverySavedAcknowledged = false;[\s\S]*?onboardingRoute = "pro"/);
  });

  it("starts every recovery screen unacknowledged and clears recovery state on full cleanup", () => {
    const password = functionSource("bindPasswordForm", "bindImportForm");
    const imported = functionSource("bindImportForm", "continueOnboardingFromService");
    const burn = functionSource("executeBurn", "ttlSeconds");
    expect(password).toMatch(/recoveryBundle = \{[\s\S]*?recoverySavedAcknowledged = false;[\s\S]*?onboardingRoute = "recovery"/);
    expect(imported).toMatch(/recoveryBundle = \{[\s\S]*?recoverySavedAcknowledged = false;[\s\S]*?onboardingRoute = "recovery"/);
    expect(burn).toMatch(/localStorage\.clear\(\);[\s\S]*?recoveryBundle = null;[\s\S]*?recoverySavedAcknowledged = false;/);
  });

  it("keeps setup sequential without a global completion shortcut", () => {
    const onboardingRender = functionSource("renderOnboarding", "onboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingRender).toContain('id="onboarding-back"');
    expect(onboardingRender).not.toContain('id="skip-onboarding"');
    expect(onboardingRender).not.toContain("Skip · manual setup");
    expect(onboardingRender).toContain('["pro", "privacy", "sending", "cover", "passwords", "burnpass", "browser", "tutorial", "detected", "install", "apps", "mullvad"]');
    expect(onboardingRender).not.toContain('"scrub"].includes(onboardingRoute)');
    expect(binding).not.toContain('document.querySelector("#skip-onboarding")');
    expect(binding).toContain('document.querySelector("#onboarding-back")?.addEventListener("click"');
  });

  it("completes after persisting the single chooser without opening apps", () => {
    const apps = functionSource("tutorialContent", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(apps).toContain('id="continue-app-choice"');
    expect(apps).not.toContain('id="continue-connect-app"');
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?await completeOnboarding\(\)/);
  });

  it("persists only non-sensitive chooser state before Home", () => {
    const intent = functionSource("persistCombinedHomeChoices", "selectedNativeApps");
    expect(intent).toContain("selectedOnboardingAppsStorageKey");
    expect(intent).not.toContain("savedNativeApps");
    expect(intent).not.toContain("persistSavedAccountPreferences");
    expect(intent).not.toContain("account");
    expect(intent).not.toContain("password");
  });

  it("uses the approved order and defers Scrub until after onboarding", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const completion = functionSource("completeOnboarding", "bindPasswordForm");
    const previous = functionSource("previousSetupRoute", "bindOnboarding");
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "sending"/);
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?canCompleteSetup\(setup\)[\s\S]*?onboardingRoute = "cover"/);
    expect(binding).toMatch(/#continue-cover-draft[\s\S]*?onboardingRoute = "passwords"/);
    expect(source).toContain('data-onboarding-password-next="${next}"');
    expect(functionSource("onboardingPasswordRoleContent", "mullvadSetupContent")).toContain('stealth ? "burnpass" : "mullvad"');
    expect(binding).toContain('button.dataset.passwordRoleNext as OnboardingRoute');
    expect(functionSource("bindBrowserImportControls", "importIdentityForm")).toMatch(/#continue-browser-import[\s\S]*?enterCombinedAppChoice\(\)/);
    expect(previous).toContain('pro: "recovery"');
    expect(previous).toContain('privacy: "pro"');
    expect(previous).toContain('sending: "pro"');
    expect(previous).toContain('cover: "sending"');
    expect(previous).toContain('passwords: "cover"');
    expect(previous).toContain('burnpass: "passwords"');
    expect(previous).toContain('mullvad: "burnpass"');
    expect(previous).toContain('browser: "mullvad"');
    expect(previous).toContain('tutorial: "browser"');
    expect(functionSource("onboardingContent", "tutorialContent")).not.toContain('onboardingRoute === "scrub"');
    expect(binding).not.toContain("initializeOnboardingScrub");
    expect(completion.indexOf("await loadNativeApps()")).toBeGreaterThan(completion.indexOf("await saveOnboardingPreferences"));
    expect(completion).toContain('route = "home"');
  });

  it("offers Pro activation after fresh account creation without storing the code in the renderer", () => {
    const content = functionSource("proSetupContent", "tutorialContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const activation = functionSource("activatePro", "requestClearProActivation");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(content).toContain("Enter Pro code");
    expect(content).toContain('id="activation-form"');
    expect(content).toContain('data-onboarding="sending"');
    expect(binding).toContain('"#activation-form"');
    expect(activation).toContain("validateHubActivationCode(activationCode)");
    expect(activation).toContain('onboardingRoute === "pro"');
    expect(content).not.toMatch(/localStorage|sessionStorage/);
    expect(bootstrap).toContain('onboardingRoute = "welcome"');
  });

  it("offers an optional fixed Mullvad handoff without claiming tunnel access", () => {
    const content = functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(content).toContain("Optional network privacy.");
    expect(content).toContain('id="install-mullvad"');
    expect(content).toContain('id="open-mullvad"');
    expect(content).toContain("Use my Mullvad session");
    expect(content).toContain('id="continue-mullvad"');
    expect(content).toContain('id="skip-mullvad"');
    expect(content).not.toMatch(/mullvad-connected|mullvad-autostart|refresh-mullvad|Mullvad pixels|does not copy or read/);
    expect(binding).toContain('runMullvadSetupAction("install")');
    expect(binding).toContain('runMullvadSetupAction("open")');
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "browser"[\s\S]*?refreshBrowserImportReadiness\(\)/);
  });

  it("keeps Mullvad installation and hosting behind one setup action", () => {
    const action = functionSource("runMullvadSetupAction", "bindPasswordForm");
    expect(action).toMatch(/installMullvad\(\)[\s\S]*?Date\.now\(\) \+ 180_000/);
    expect(action).toMatch(/loadMullvadStatus\(\)[\s\S]*?hostMullvadUntilReady\(/);
    expect(action).toContain('hostMullvadUntilReady("Open Mullvad inside OSL")');
    const guardedHost = functionSource("hostMullvadWithDeadline", "hostMullvadUntilReady");
    expect(guardedHost).toContain("label, 30_000");
    expect(guardedHost).toMatch(/hostAttempt\.then[\s\S]*?restoreMullvadWindow\(\)/);
    const readinessRetry = functionSource("hostMullvadUntilReady", "runMullvadSetupAction");
    expect(readinessRetry).toContain('["appNotInstalled", "existingSessionUnavailable", "windowOperationRejected"]');
    expect(readinessRetry).toContain("Date.now() < deadline");
    expect(source).toMatch(/async function validateNativeSurfaces[\s\S]*?hostMullvadWithDeadline\("Reopen Mullvad"\)[\s\S]*?mullvadWindowHosted = true/);
    expect(action).not.toContain("check again when it finishes");
    expect(functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup")).toContain('class="mullvad-setup-notice" role="status"');
  });

  it("refreshes Mullvad after an unfinished setup is unlocked", () => {
    const passwordBinding = functionSource("bindPasswordForm", "bindImportForm");
    expect(passwordBinding.match(/onboardingRoute === "mullvad"\) void refreshMullvadSetup\(\)/g)).toHaveLength(2);
  });

  it("offers guarded sending choices without overstating placement support", () => {
    const content = functionSource("sendingSetupContent", "coverDraftSetupContent");
    expect(content).toContain("Choose how to send");
    expect(content).toContain("manualSendingAnimationMarkup(selectedMode)");
    expect(source).toContain('step(1, "Write")');
    expect(source).toContain('step(2, "Encrypt")');
    expect(source).toContain('step(4, finalStep)');
    expect(content).toContain("Never presses Send");
    expect(content).toContain('option("double", "Double Enter"');
    expect(content).toContain('option("single", "Single Enter"');
    expect(content).toContain("If OSL cannot prove the destination");
  });

  it("shows a restrained animated atomic-versus-typing comparison", () => {
    const content = functionSource("coverDraftSetupContent", "onboardingPasswordRoleContent");
    expect(content).toContain("Choose cover insertion");
    expect(content).toContain("AI writes the cover one character at a time");
    expect(content).toContain("Insert on send");
    expect(content).toContain("LOOKS GOOD");
    expect(content).toContain("Pro");
    expect(content).toContain('class="cover-atomic-preview"');
    expect(content).toContain('class="cover-composer cover-typing-preview"');
    expect(content).not.toContain('style="--cover-delay:');
    expect(styles).toContain(".cover-typing-preview i:nth-child(10) { --cover-delay: 2.98s; }");
    expect(content).toContain("OSL stops if it cannot verify the exact destination");
  });

  it("collects only wired password roles and exposes only real capture resistance", () => {
    const passwords = functionSource("onboardingPasswordRoleContent", "onboardingPrivacyContent");
    const privacy = functionSource("captureSetupMarkup", "coverDraftSetupContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(passwords).toContain("Stealth password");
    expect(passwords).toContain("Burn password");
    expect(passwords).toContain('data-onboarding-password-role="${role}"');
    expect(passwords).toContain("Current password");
    expect(passwords).toContain("Set password");
    expect(privacy).toContain("Protected messages appear only after OSL enables this protection");
    expect(privacy).toContain('id="window-capture-enabled"');
    expect(privacy).toContain('type="checkbox"');
    expect(privacy).not.toContain("Decrypt display");
    expect(privacy).not.toContain("Unavailable during setup");
    expect(privacy).not.toContain('id="decrypt-display"');
    expect(binding).toContain("setScreenshotProtection(windowCaptureEnabled)");
  });

  it("advances password-role setup only after an explicit valid form submission", () => {
    const binding = functionSource("bindOnboardingPasswordRole", "bindPasswordVisibility");
    expect(binding).toContain('form.addEventListener("submit"');
    expect(binding).toContain("if (!submit || submit.disabled || !error) return");
    expect(binding).toMatch(/current\.addEventListener\("input", validate\)[\s\S]*?alternate\.addEventListener\("input", validate\)[\s\S]*?confirm\.addEventListener\("input", validate\)/);
    expect(binding).not.toMatch(/current\.addEventListener\("(?:click|focus)"/);
    for (const eventName of ["click", "focus", "pointerdown", "input"]) {
      const listener = new RegExp(`(?:current|alternate|confirm)\\.addEventListener\\("${eventName}"[\\s\\S]{0,180}?onboardingRoute`);
      expect(binding).not.toMatch(listener);
    }
    expect(source).toContain('data-skip-onboarding-password-role="${next}"');
    const onboardingBinding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingBinding).toContain('querySelectorAll<HTMLButtonElement>("button[data-password-role-next]")');
    expect(onboardingBinding).not.toContain('querySelectorAll<HTMLButtonElement>("[data-password-role-next]")');
    expect(onboardingBinding).toContain('querySelectorAll<HTMLButtonElement>("button[data-skip-onboarding-password-role]")');
    expect(onboardingBinding).toMatch(/button\[data-skip-onboarding-password-role\][\s\S]*?onboardingRoute = next/);
    expect(binding).toContain("form.dataset.onboardingPasswordNext as OnboardingRoute");
    expect(binding).not.toContain("form.dataset.passwordRoleNext");
  });

  it("re-reads readiness when password setup reports a failure", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding).toContain('submit.textContent = setupMode ? "Creating account…" : "Unlocking…"');
    expect(binding).toContain('form.setAttribute("aria-busy", "true")');
    expect(binding).toMatch(/catch \(failure\)[\s\S]*?withNativeDeadline\(loadCoreIntegration\(\), "Check OSL account"/);
    expect(binding).toContain('readiness.bootstrapStatus === "ready" && readiness.unlocked');
    expect(binding).toContain('readiness.bootstrapStatus === "passwordRequired"');
    expect(binding).toMatch(/readiness\.bootstrapStatus === "passwordRequired"[\s\S]*?unlockHubPasswordGate\(secret\)/);
    expect(binding).toContain('readiness.bootstrapStatus === "setupRequired" && readiness.identityLoaded');
    expect(binding).not.toContain("password setup did not finish");
  });

  it("does not replace a focused onboarding control during an unchanged background refresh", () => {
    const rendering = functionSource("renderOnboarding", "onboardingContent");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(rendering).toContain("lastOnboardingMarkup === markup");
    expect(rendering).toContain('root.querySelector(".onboarding-shell")');
    expect(rendering).toContain("sensitiveEditInProgress");
    expect(rendering).toContain('input[type="password"]');
    expect(rendering.indexOf("return;")).toBeLessThan(rendering.indexOf("root.innerHTML = markup"));
    expect(bootstrap).toContain("renderWhenIdle();");
    expect(bootstrap).not.toContain('route === "onboarding" ? render() : renderWhenIdle()');
  });

  it("reloads the encrypted service registry after first password setup and recovery", () => {
    const createBinding = functionSource("bindPasswordForm", "bindImportForm");
    const importBinding = functionSource("bindImportForm", "renderWorkspace");
    expect(createBinding).toMatch(/setupHubMainPassword\(secret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
    expect(importBinding).toMatch(/setupHubMainPassword\(passwordSecret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
  });

  it("reloads password roles immediately after setup and unlock", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding.match(/loadHubPasswordRoleStatus\(\)/g)?.length).toBeGreaterThanOrEqual(2);
  });
});
