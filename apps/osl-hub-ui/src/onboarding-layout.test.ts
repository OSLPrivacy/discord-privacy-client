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
    expect(source).toContain('class="button signin-create" data-onboarding="create"');
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
    expect(styles).toMatch(/\.desktop-drag-region\s*\{[^}]*flex:\s*1 1 auto/s);
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
    expect(binding).toContain('submit.textContent = "Creating account…"');
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
    expect(detected).toContain('id="continue-detected-apps"');
  });

  it("keeps the final setup route order explicit", () => {
    const previous = functionSource("previousSetupRoute", "bindOnboarding");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(previous).toContain('browser: "recovery"');
    expect(previous).toContain('detected: "browser"');
    expect(previous).toContain('sending: "mullvad"');
    expect(previous).toContain('passwords: "sending"');
    expect(previous).toContain('privacy: "burnpass"');
    expect(previous).toContain('scrub: "privacy"');
    expect(binding).toMatch(/#continue-detected-apps[\s\S]*?onboardingRoute = "mullvad"/);
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "sending"/);
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
    expect(browser).toContain("Move supported data from every detected browser.");
    expect(browser).toContain('id="import-saved-accounts"');
    expect(browser).not.toContain('id="install-firefox"');
    expect(browser).toContain('firefoxStatus.availability === "installed"');
    expect(browser.match(/id="import-saved-accounts"/g)).toHaveLength(1);
    expect(browser).toContain('id="continue-browser-import" type="button" ${browserImportCancelling ? "disabled" : ""}');
    expect(browser).toContain("Stays inside OSL");
    expect(browser).not.toContain('data-browser-source="${browser.id}"');
    expect(browser).toContain('"Import all"');
    expect(source).toContain("beginProtectedBrowserImport([source], operationId)");
    expect(source).toContain("finishProtectedBrowserImport(operationId)");
    expect(binding).toContain('onboardingRoute = "detected"');
    expect(binding).not.toContain("window.confirm");
    expect(source).not.toContain("browserPasswordImportOptIn");
    expect(source).not.toContain("data-browser-password-import");
    expect(source).not.toContain("browser-password-import-opt-in");
  });

  it("keeps browser import vertically scrollable without a bottom scrollbar", () => {
    expect(styles).toMatch(/\.onboarding-shell\s*\{[^}]*overflow-x:\s*hidden;[^}]*overflow-y:\s*auto;/);
    expect(styles).toMatch(/\.browser-detected-sources\s*\{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;/);
    expect(styles).toMatch(/\.onboarding-panel\s*\{[^}]*min-width:\s*0;[^}]*max-width:\s*100%;/);
  });

  it("opens and completes every detected browser import with one explicit OSL action", () => {
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    const worker = functionSource("importOneBrowser", "persistSavedAccountPreferences");
    expect(binding).toMatch(/#import-saved-accounts[\s\S]*?browserImports\.filter\(\(browser\) => browser\.installed\)[\s\S]*?importOneBrowser\(source, index \+ 1, selected\.length\)[\s\S]*?savedAccountsReady = true[\s\S]*?onboardingRoute = "detected"/);
    expect(worker).toContain("beginProtectedBrowserImport([source], operationId)");
    expect(worker).toContain("finishProtectedBrowserImport(operationId)");
    expect(worker).toContain("protectedBrowserImportSourceDeadlineMs");
    expect(worker).toContain("cancelProtectedBrowserImport(operationId)");
    expect(binding).toContain("cancelProtectedBrowserImport(operation.operationId)");
    expect(binding).toContain("selected.length === 0");
    expect(binding).not.toContain("#install-firefox");
    expect(binding).not.toContain("window.confirm");
  });

  it("refreshes browser and Firefox readiness when the import page is entered or resumed", () => {
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const refresh = functionSource("refreshBrowserImportReadiness", "importIdentityForm");
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const bootstrap = source.slice(source.indexOf("async function bootstrap"));
    expect(browser).toContain("browserReadinessBusy");
    expect(refresh).toContain('loadBrowserImports(), "Refresh browsers"');
    expect(refresh).toContain('loadFirefoxStatus(), "Refresh Firefox"');
    expect(refresh).toContain("browserImports = catalog");
    expect(refresh).toContain("firefoxStatus = currentFirefoxStatus");
    expect(continuation).toContain("void refreshBrowserImportReadiness()");
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
    expect(binding).toContain("activeBrowserAccountsReadyStorageKey()");
    expect(binding).toContain('localStorage.setItem(readyKey, "true")');
    expect(source).not.toMatch(/localStorage\.setItem\(savedAccountsReadyStorageKey\s*,/);
  });

  it("restores an unfinished browser import only for the active OSL identity", () => {
    const pendingKey = functionSource("activeBrowserImportPendingStorageKey", "refreshActiveBrowserAccountsReady");
    const refresh = functionSource("refreshActiveBrowserAccountsReady", "saveHomeTilePreferences");
    const binding = functionSource("bindBrowserImportControls", "refreshBrowserImportReadiness");
    expect(pendingKey).toContain("identityScopedStorageKey(browserImportPendingStorageKey)");
    expect(refresh).toContain("savedAccountsReady = key !== null");
    expect(refresh).toContain("localStorage.removeItem(pendingKey)");
    expect(source).toMatch(/function commitRender[\s\S]*?refreshActiveBrowserAccountsReady\(\)/);
    expect(source).toMatch(/beginProtectedBrowserImport\(\[source\], operationId\)[\s\S]*?savedAccountsReady = true/);
    expect(binding).toMatch(/activeBrowserAccountsReadyStorageKey\(\)[\s\S]*?localStorage\.setItem\(readyKey, "true"\)/);
    expect(binding).toMatch(/#continue-browser-import[\s\S]*?localStorage\.removeItem\(pendingKey\)/);
    expect(source).not.toMatch(/localStorage\.setItem\(browserImportPendingStorageKey\s*,/);
  });

  it("keeps the browser resume checkpoint until explicit setup exit or completion", () => {
    const continuation = functionSource("continueOnboardingFromService", "currentHomeTileIds");
    const workspace = functionSource("bindWorkspace", "ttlSeconds");
    const finishStart = workspace.indexOf('querySelector("#service-guide-finish")');
    const exitStart = workspace.indexOf('querySelector("#service-guide-exit")');
    const nativeBackStart = workspace.indexOf('querySelector("#native-app-back")');
    expect(finishStart).toBeGreaterThanOrEqual(0);
    expect(exitStart).toBeGreaterThan(finishStart);
    expect(nativeBackStart).toBeGreaterThan(exitStart);
    expect(continuation).toContain('onboardingRoute = "browser"');
    expect(continuation).not.toContain("clearServiceOnboardingResume()");
    expect(workspace.slice(finishStart, exitStart)).not.toContain("clearServiceOnboardingResume()");
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
    const save = functionSource("saveScrubSetupPlan", "previousSetupRoute");
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
    expect(recovery).toContain("Save your recovery phrases");
    expect(recovery).toContain("Account recovery phrase");
    expect(recovery).toContain("Password recovery phrase");
    expect(recovery).not.toContain("Account details");
    expect(recovery).not.toContain("OSL cannot retrieve these later");
    expect(recovery).not.toContain('class="compact-lead"');
  });

  it("continues from saved recovery material into protected browser import", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(functionSource("recoveryContent", "identityPasswordForm")).not.toContain('id="copy-recovery-kit"');
    expect(binding).toMatch(/#recovery-continue[\s\S]*?recoveryBundle = null;[\s\S]*?onboardingRoute = "browser";[\s\S]*?render\(\)[\s\S]*?refreshBrowserImportReadiness\(\)/);
  });

  it("offers an explicit manual-setup escape throughout setup", () => {
    const onboardingRender = functionSource("renderOnboarding", "onboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingRender).toContain('id="skip-onboarding"');
    expect(onboardingRender).toContain('id="onboarding-back"');
    expect(onboardingRender).toContain("Skip · manual setup");
    expect(onboardingRender).toContain("activeSetupRoutes.includes");
    expect(source).not.toMatch(/type OnboardingRoute[^\n]*"install"/);
    expect(binding).toContain('document.querySelector("#skip-onboarding")?.addEventListener("click"');
    expect(binding).toContain('document.querySelector("#onboarding-back")?.addEventListener("click"');
  });

  it("removes the forced real-service connection step", () => {
    expect(source).not.toContain("function onboardingAppsContent");
    expect(source).not.toContain("Connect one app");
    expect(source).not.toContain('id="continue-connect-app"');
    expect(source).not.toContain('data-connect-app-choice=');
  });

  it("places featured local Scrub last before Home", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const completion = functionSource("completeOnboarding", "bindPasswordForm");
    const scrub = functionSource("scrubSetupContent", "scrubAccountSelections");
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "sending"/);
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?canCompleteSetup\(setup\)[\s\S]*?onboardingRoute = "passwords"/);
    expect(source).toContain('data-password-role-next="${next}"');
    expect(binding).toContain('button.dataset.passwordRoleNext as OnboardingRoute');
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "scrub"/);
    expect(binding).toMatch(/#finish-scrub-setup[\s\S]*?saveScrubSetupPlan\(onboardingScrubMode\)[\s\S]*?completeOnboarding\(\)/);
    expect(functionSource("saveScrubSetupPlan", "previousSetupRoute")).toContain("activeScrubSetupPlanStorageKey()");
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

  it("offers a simple optional Mullvad startup choice", () => {
    const content = functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(content).toContain("Mullvad Recommended");
    expect(content).toContain("Configure on startup");
    expect(content).toContain("Open Mullvad when OSL starts");
    expect(content).toContain("Don't do that");
    expect(content).toContain('data-mullvad-choice=');
    expect(content).toContain('id="continue-mullvad"');
    expect(source).toContain('import mullvadLogoUrl from "./mullvad-logo.svg?url"');
    expect(content).toContain('<img src="${mullvadLogoUrl}" alt=""/>');
    expect(binding).toContain('button.dataset.mullvadChoice === "auto"');
    expect(binding).toContain('localStorage.setItem(mullvadStartupStorageKey');
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "sending"/);
  });

  it("offers guarded sending choices without overstating placement support", () => {
    const content = functionSource("sendingSetupContent", "onboardingPasswordRoleContent");
    expect(content).toContain("Choose how to send");
    expect(content).toContain("manualSendingAnimationMarkup(mode)");
    expect(content).toContain('option("clipboard", "Copy", "safe", "Safest")');
    expect(content).toContain('option("double", "Double Enter"');
    expect(content).toContain('option("single", "Single Enter"');
    expect(content).toContain("Can possibly break ToS");
    expect(content).toContain("Breaks some ToS · risky");
    const animation = functionSource("manualSendingAnimationMarkup", "passwordEyeIcon");
    expect(animation).toContain('["Enter", "Ctrl+V", "Enter"]');
    expect(animation).toContain('["Enter", "Enter"]');
    expect(animation).not.toContain("encrypt · copy");
    expect(animation).not.toContain("verify · send");
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
    expect(privacy).toContain('id="onboarding-screenshot-protection"');
    expect(privacy).toContain("Decrypt display");
    expect(privacy).toContain("Hide notification content");
    expect(privacy).toContain("Auto-lock on idle");
    expect(privacy).toContain("Disable link previews");
    expect(privacy).toContain("IP-grabber protection");
    expect(privacy).toContain("Link reputation checks are not available in this build.");
    expect(privacy).toContain("Open links in your default browser");
    expect(privacy).toContain("External-link routing is not available in this build.");
    expect(privacy).toContain("Coming later");
    expect(privacy).not.toContain('id="decrypt-display"');
    expect(binding).toContain("changeScreenshotProtection");
    expect(binding).toContain('notificationPreviewContent = !setupPrivacyChoices.has("hide-notifications")');
  });

  it("shows Scrub account handles beside existing service logos and leaves the intro hero unringed", () => {
    const scrub = functionSource("scrubSetupContent", "scrubAccountSelections");
    const targets = functionSource("scrubAccountSelections", "previousSetupRoute");
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
    expect(binding).toMatch(/catch \(failure\)[\s\S]*?withNativeDeadline\(loadCoreIntegration\(\), "Check OSL account"/);
    expect(binding).toContain('readiness.bootstrapStatus === "ready" && readiness.unlocked');
    expect(binding).toContain('readiness.bootstrapStatus === "passwordRequired"');
    expect(binding).toContain('readiness.bootstrapStatus === "setupRequired" && readiness.identityLoaded');
    expect(binding).not.toContain("password setup did not finish");
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
