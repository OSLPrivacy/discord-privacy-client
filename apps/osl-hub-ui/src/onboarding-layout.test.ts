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
    const unlock = functionSource("identityPasswordForm", "sendingSetupContent");
    expect(unlock).not.toContain("Stays on this device.");
    expect(styles).toMatch(/\.unlock-form\s*\{[^}]*gap:\s*12px/s);
    expect(styles).toMatch(/\.unlock-form \.password-input-row,[\s\S]*?\.unlock-card > \.text-back\s*\{[^}]*width:\s*100%/s);
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
    expect(titlebar).toContain('id="window-fullscreen"');
    expect(titlebar).toContain('aria-label="Toggle fullscreen"');
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

  it("shows truthful progress and prevents duplicate account creation while busy", () => {
    const form = functionSource("identityPasswordForm", "sendingSetupContent");
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(form).toContain('id="account-create-status" aria-live="polite"');
    expect(binding).toContain('form.setAttribute("aria-busy", "true")');
    expect(binding).toContain('submit.textContent = setupMode ? "Creating account…" : "Unlocking…"');
    expect(binding).toContain('createStatus.textContent = "Creating encryption keys…"');
    expect(binding).toContain('await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))');
    expect(binding).toContain('createStatus.textContent = "Securing this device…"');
    expect(binding).toContain('createStatus.textContent = "Loading your account…"');
  });
});

describe("fresh-account continuation", () => {
  it("uses one unified detected-services page after protected browser import", () => {
    const detected = functionSource("detectedAppsContent", "browserImportContent");
    expect(detected).toContain("Detected services");
    expect(detected).toContain('data-detected-account="${escapeHtml(id)}"');
    expect(detected).toContain('class="detected-account-logo service-brand-badge"');
    expect(detected).toContain('data-service-brand="${service.id}"');
    expect(detected).toContain("Use current desktop session · provider-wide");
    expect(detected).toContain("Use isolated OSL profile · this account");
    expect(detected).toContain("Found in selected browser history");
    expect(detected).toContain('detectedAccountChoiceKey("browser", app.id)');
    expect(detected).toContain('id="continue-detected-apps"');
  });

  it("keeps the final setup route order explicit", () => {
    const previous = functionSource("previousSetupRoute", "bindOnboarding");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(previous).toContain('browser: "recovery"');
    expect(previous).toContain('detected: "browser"');
    expect(previous).toContain('apps: "detected"');
    expect(previous).toContain('pro: "apps"');
    expect(previous).toContain('sending: "pro"');
    expect(previous).toContain('cover: "sending"');
    expect(previous).toContain('passwords: "cover"');
    expect(previous).toContain('burnpass: "passwords"');
    expect(previous).toContain('mullvad: "burnpass"');
    expect(previous).toContain('privacy: "mullvad"');
    expect(previous).toContain('scrub: "privacy"');
    expect(binding).toMatch(/#continue-detected-apps[\s\S]*?onboardingRoute = "apps"/);
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "privacy"/);
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "scrub"/);
  });

  it("configures each detected account independently", () => {
    const detected = functionSource("detectedAppsContent", "browserImportContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(detected).toContain('data-detected-account="${escapeHtml(id)}"');
    expect(detected).toContain('data-detected-account-row="${escapeHtml(id)}"');
    expect(detected).toContain('id="detected-launch-select"');
    expect(binding).toContain('querySelectorAll<HTMLSelectElement>("[data-detected-account]")');
    expect(binding).toContain('detectedAccountChoices.set(id');
    expect(binding).toContain('CSS.escape(id)');
    expect(binding).toContain('classList.toggle("detected-account-osl"');
    expect(binding).toContain('document.querySelector<HTMLSelectElement>("#detected-launch-select")');
  });

  it("keeps saved-account migration on its own local-only page without account discovery claims", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(browser).toContain("Import browser data");
    expect(browser).toContain("Choose only the browsers you want to import.");
    expect(browser).toContain('id="import-saved-accounts"');
    expect(browser).not.toContain('id="install-firefox"');
    expect(browser).toContain('firefoxStatus.availability === "installed"');
    expect(browser.match(/id="import-saved-accounts"/g)).toHaveLength(1);
    expect(browser).toContain('id="continue-browser-import" type="button" ${browserImportCancelling ? "disabled" : ""}');
    expect(browser).toContain("Stays inside OSL");
    expect(browser).toContain('data-browser-source="${browser.id}"');
    expect(browser).toContain('id="toggle-all-browser-imports"');
    expect(browser).toContain("Import selected");
    expect(binding).toContain("selectedBrowserImports.has(browser.id)");
    expect(binding).toContain("selectedBrowserImports.add(source)");
    expect(binding).toContain("selectedBrowserImports.delete(source)");
    expect(source).toContain("beginProtectedBrowserImport(selected, operationId)");
    expect(source).toContain("finishProtectedBrowserImport(operationId)");
    expect(binding).toContain('onboardingRoute = "detected"');
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
    expect(choices).toContain("Use existing account");
    expect(choices).toContain("Use separate account");
    expect(binding).toContain('requested !== "isolatedOsl" && requested !== "existingBrowser"');
    expect(binding).toContain('useDefaultBrowserCompanion = requested === "existingBrowser"');
    expect(binding).toContain('localStorage.setItem("osl-default-browser-companion-v1", String(useDefaultBrowserCompanion))');
    expect(source).toContain('loadDefaultBrowserCompanionStatus(), "Check default browser"');
    expect(source).toContain("defaultBrowserCompanionStatus = currentBrowserCompanionStatus");
    expect(source).toContain("await detachDefaultBrowserCompanion().catch(() => undefined)");
    expect(source).toContain("resizeDefaultBrowserCompanion()");
    expect(source).toContain("focusDefaultBrowserCompanion()");
  });

  it("keeps browser import vertically scrollable without a bottom scrollbar", () => {
    expect(styles).toMatch(/\.onboarding-shell\s*\{[^}]*overflow-x:\s*hidden;[^}]*overflow-y:\s*auto;/);
    expect(styles).toMatch(/\.browser-detected-sources\s*\{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;/);
    expect(styles).toMatch(/\.onboarding-panel\s*\{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;/);
  });

  it("opens and completes only the selected browser imports with one explicit OSL action", () => {
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    const worker = functionSource("importSelectedBrowsers", "persistSavedAccountPreferences");
    expect(binding).toMatch(/#import-saved-accounts[\s\S]*?browserImports\.filter\(\(browser\) => browser\.installed && selectedBrowserImports\.has\(browser\.id\)\)[\s\S]*?importSelectedBrowsers\(selected\)[\s\S]*?savedAccountsReady = true[\s\S]*?onboardingRoute = "detected"/);
    expect(worker).toContain("beginProtectedBrowserImport(selected, operationId)");
    expect(worker).toContain("finishProtectedBrowserImport(operationId)");
    expect(worker).toContain("protectedBrowserImportDeadlineMs");
    expect(worker).toContain("cancelProtectedBrowserImport(operationId)");
    expect(source).not.toContain("Browser ${position} of ${total}");
    expect(source).not.toContain("for (const [index, source] of selected.entries())");
    expect(binding).toContain("completedBrowserImportIds = new Set(result.succeededSources)");
    expect(binding).toContain("detectedBrowserServices = new Set(result.detectedServices)");
    expect(binding).toContain('savedAccountMode = "use"');
    expect(binding).toContain("persistSavedAccountPreferences()");
    expect(binding).toContain("result.failedSources.length > 0");
    expect(binding).toContain("cancelProtectedBrowserImport(operation.operationId)");
    expect(binding).toContain("selected.length === 0");
    expect(binding).not.toContain("#install-firefox");
    expect(binding).not.toContain("window.confirm");
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
    expect(advance).toContain('onboardingRoute = "pro"');
    expect(continuation).not.toContain("clearServiceOnboardingResume()");
    expect(bootstrap).toMatch(/onboardingRoute === "browser"[\s\S]*?refreshBrowserImportReadiness\(\)/);
  });

  it("scopes completed browser import to the active OSL identity", () => {
    const identityKey = functionSource("identityScopedStorageKey", "pendingOnboardingRoute");
    const key = functionSource("activeBrowserAccountsReadyStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(identityKey).toContain("core.readiness.activeOslUserId");
    expect(identityKey).toContain("encodeURIComponent(owner)");
    expect(key).toContain("identityScopedStorageKey(savedAccountsReadyStorageKey)");
    expect(refresh).toContain("savedAccountsReady = key !== null");
    expect(binding).toMatch(/importSelectedBrowsers\(selected\)[\s\S]*?localStorage\.setItem\(readyKey, "true"\)/);
    expect(binding).toMatch(/localStorage\.setItem\(readyKey, "true"\)[\s\S]*?savedAccountsReady = true/);
    expect(source).not.toMatch(/localStorage\.setItem\(savedAccountsReadyStorageKey\s*,/);
  });

  it("clears legacy pending import state without resuming an unmanaged browser", () => {
    const pendingKey = functionSource("activeBrowserImportPendingStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    expect(pendingKey).toContain("identityScopedStorageKey(browserImportPendingStorageKey)");
    expect(refresh).toContain("savedAccountsReady = key !== null");
    expect(refresh).toContain("localStorage.removeItem(pendingKey)");
    expect(source).toMatch(/function commitRender[\s\S]*?refreshActiveBrowserAccountsReady\(\)/);
    expect(source).toMatch(/beginProtectedBrowserImport\(selected, operationId\)[\s\S]*?savedAccountsReady = true/);
    expect(binding).toMatch(/activeBrowserAccountsReadyStorageKey\(\)[\s\S]*?localStorage\.setItem\(readyKey, "true"\)/);
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?localStorage\.removeItem\(pendingKey\)/);
    expect(source).not.toMatch(/localStorage\.setItem\(browserImportPendingStorageKey\s*,/);
  });

  it("returns from each service to the remaining app queue before completion", () => {
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const advance = functionSource("advanceOnboardingConnection", "ensureNativeCatalogForAppChoice");
    const workspace = functionSource("bindWorkspace", "ttlSeconds");
    const finishStart = workspace.indexOf('querySelector("#service-guide-finish")');
    const exitStart = workspace.indexOf('querySelector("#service-guide-exit")');
    const nativeBackStart = workspace.indexOf('querySelector("#native-app-back")');
    expect(finishStart).toBeGreaterThanOrEqual(0);
    expect(exitStart).toBeGreaterThan(finishStart);
    expect(nativeBackStart).toBeGreaterThan(exitStart);
    expect(continuation).toContain("advanceOnboardingConnection(completedAppId)");
    expect(advance.indexOf('route = "onboarding"')).toBeLessThan(advance.indexOf('if (!hasNext)'));
    expect(advance).toContain('onboardingRoute = "pro"');
    expect(workspace.slice(finishStart, exitStart)).toContain("advanceOnboardingConnection(activeHomeAppId)");
    expect(workspace.slice(exitStart, nativeBackStart)).toContain("clearServiceOnboardingResume()");
    expect(functionSource("completeOnboarding", "bindPasswordForm")).toContain("clearServiceOnboardingResume()");
  });

  it("persists every active setup route and Scrub substep for only the active identity", () => {
    const key = functionSource("identityScopedStorageKey", "pendingOnboardingRoute");
    const pending = functionSource("pendingOnboardingRoute", "persistOnboardingResume");
    const persist = functionSource("persistOnboardingResume", "markServiceOnboardingOpened");
    const onboardingRender = functionSource("renderOnboarding", "onboardingContent");
    expect(key).toContain("core.readiness.activeOslUserId");
    expect(key).toContain("encodeURIComponent(owner)");
    expect(pending).toContain("parseSetupResumeCheckpoint");
    expect(pending).toContain("scrubSetupStep = checkpoint.scrubStep");
    expect(persist).toContain("isActiveSetupRoute(routeToPersist)");
    expect(persist).toContain('routeToPersist === "scrub" ? step : "intro"');
    expect(onboardingRender).toContain("persistOnboardingResume()");
  });

  it("loads the identity-scoped Scrub plan through validation before seeding Settings", () => {
    const apply = functionSource("applySavedScrubSetupPlan", "saveScrubSetupPlan");
    const save = functionSource("saveScrubSetupPlan", "bindOnboarding");
    const moduleOpen = functionSource("openHomeModule", "oslChatTimestamp");
    expect(apply).toContain("activeScrubSetupPlanStorageKey()");
    expect(apply).toContain("parseScrubSetupPlan");
    expect(apply).toContain("selectedOnboardingScrubAccounts = new Set(plan.targetIds)");
    expect(apply).toContain("enabledScrubSignals = new Set(plan.signalGroups)");
    expect(apply).toContain("autoScrubAccountId = target.selection.accountId");
    expect(save).toContain("parseScrubSetupPlan");
    expect(moduleOpen).toMatch(/id === "scrub"[\s\S]*?applySavedScrubSetupPlan\(\)/);
  });

  it("shows the recovery title without the removed grey subtitle", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(recovery).toContain("Save your recovery kit");
    expect(recovery).toContain("Account recovery");
    expect(recovery).toContain("Password recovery");
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
    expect(onboardingRender).toContain('["pro", "privacy", "sending", "cover", "passwords", "burnpass", "browser", "tutorial", "detected", "install", "apps", "mullvad", "scrub"].includes(onboardingRoute)');
    expect(binding).toContain('document.querySelector("#onboarding-back")?.addEventListener("click"');
  });

  it("persists the chooser, offers selected app connections, then advances to Pro", () => {
    const apps = functionSource("tutorialContent", "selectedNativeApps");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const chooserHandler = binding.slice(binding.indexOf('querySelector<HTMLButtonElement>("#continue-app-choice")'), binding.indexOf('querySelector<HTMLButtonElement>("#continue-detected-apps")'));
    expect(apps).toContain('id="continue-app-choice"');
    expect(apps).not.toContain('id="continue-connect-app"');
    expect(binding).toMatch(/#continue-app-choice[\s\S]*?persistCombinedHomeChoices\(\)[\s\S]*?onboardingAppChoicesConfirmed = true[\s\S]*?selectNextConnectApp\(\)[\s\S]*?onboardingRoute = "pro"/);
    expect(binding).toMatch(/#skip-connect-app[\s\S]*?selectNextConnectApp\(\)[\s\S]*?onboardingRoute = "pro"/);
    expect(chooserHandler).not.toContain("completeOnboarding()");
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
    const scrub = functionSource("scrubSetupContent", "scrubAccountSelections");
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "privacy"/);
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?canCompleteSetup\(setup\)[\s\S]*?onboardingRoute = "passwords"/);
    expect(source).toContain('data-password-role-next="${next}"');
    expect(binding).toContain('button.dataset.passwordRoleNext as OnboardingRoute');
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "scrub"/);
    expect(binding).toMatch(/#finish-scrub-setup[\s\S]*?saveScrubSetupPlan\(onboardingScrubMode\)[\s\S]*?completeOnboarding\(\)/);
    expect(functionSource("saveScrubSetupPlan", "bindOnboarding")).toContain("activeScrubSetupPlanStorageKey()");
    expect(source).toContain('id="route-heading" tabindex="-1">Scrub');
    expect(source).toContain('id="start-scrub-setup"');
    expect(source).toContain('id="continue-scrub-accounts"');
    expect(source).not.toContain('id="initialize-scrub"');
    expect(scrub).not.toContain('id="privacy-export-input"');
    expect(source).not.toContain("function onboardingScrubContent");
    expect(source).not.toContain('id="onboarding-start-scrub"');
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
    expect(content).toContain('data-mullvad-choice="${value}"');
    expect(binding).toContain('localStorage.setItem(mullvadStartupStorageKey');
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "privacy"/);
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
    expect(content).toContain("manualSendingAnimationMarkup(mode)");
    expect(content).toContain('option("clipboard", "Copy", "safe", "Safest")');
    expect(content).toContain('option("double", "Double Enter"');
    expect(content).toContain('option("single", "Single Enter"');
    expect(content).toContain("Can possibly break ToS");
    expect(content).toContain("Breaks some ToS · risky");
    expect(content).not.toContain("captureSetupMarkup()");
    expect(content).not.toContain("Screen capture");
    const animation = functionSource("manualSendingAnimationMarkup", "passwordEyeIcon");
    expect(animation).toContain('step(1, "Write")');
    expect(animation).toContain('step(2, "Encrypt")');
    expect(animation).toContain('step(4, finalStep)');
    expect(animation).not.toContain("encrypt · copy");
    expect(animation).not.toContain("verify · send");
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
    const privacy = functionSource("onboardingPrivacyContent", "mullvadSetupContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(passwords).toContain("Stealth password");
    expect(passwords).toContain("Burn password");
    expect(passwords).toContain('data-onboarding-password-role="${role}"');
    expect(passwords).toContain("Current password");
    expect(passwords).toContain("Set password");
    expect(privacy).toContain("Windows capture resistance");
    expect(privacy).toContain('id="onboarding-screenshot-protection"');
    expect(privacy).toContain('type="checkbox"');
    expect(privacy).not.toContain("Unavailable during setup");
    expect(binding).toContain("changeScreenshotProtection(event.currentTarget as HTMLInputElement)");
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

  it("shows Scrub account handles beside existing service logos and leaves the intro hero unringed", () => {
    const scrub = functionSource("scrubSetupContent", "scrubAccountSelections");
    const targets = functionSource("scrubAccountSelections", "activeScrubSetupPlanStorageKey");
    expect(scrub).toContain("serviceLogo(selection.serviceId as ServiceId)");
    expect(scrub).toContain('class="scrub-account-logo service-brand-badge"');
    expect(scrub).toContain('data-service-brand="${selection.serviceId}"');
    expect(styles).toMatch(/\.service-brand-badge\s*\{[^}]*border:\s*0;[^}]*border-radius:\s*11px !important;/);
    expect(styles).toContain('.service-brand-badge[data-service-brand="discord"] { --service-brand: #5865f2; }');
    expect(styles).toContain('.service-brand-badge[data-service-brand="telegram"] { --service-brand: #26a5e4; }');
    expect(styles).toContain('.service-brand-badge[data-service-brand="signal"] { --service-brand: #3a76f0; }');
    expect(targets).toContain("account.label");
    expect(styles).not.toContain(".scrub-hero::before");
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
