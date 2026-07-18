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
});

describe("fresh-account continuation", () => {
  it("separates app choice, detected apps, and missing-app installation", () => {
    const choice = functionSource("tutorialContent", "selectedNativeApps");
    const detected = functionSource("detectedAppsContent", "installMissingAppsContent");
    const install = functionSource("installMissingAppsContent", "onboardingAppsContent");
    expect(choice).toContain("Choose your apps");
    expect(choice).toContain('data-onboarding-app-choice="${app.id}"');
    expect(choice).toContain("does not sign in or discover accounts");
    expect(detected).toContain("Use installed apps");
    expect(detected).toContain('data-saved-native="${app.id}"');
    expect(detected).toContain("does not discover their accounts or sign you in");
    expect(install).toContain("Install missing apps");
    expect(install).toContain('data-first-install="${app.id}"');
    expect(install).toContain("Selected installs start through Windows after you continue");
  });

  it("queues selected first-run installs without delaying the next setup step", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const install = functionSource("startBackgroundInstall", "enqueueBackgroundInstalls");
    const queue = functionSource("enqueueBackgroundInstalls", "nativeHostFailureMessage");
    expect(binding).toMatch(/#continue-install-apps[\s\S]*?const selectedInstalls = \[\.\.\.selectedFirstInstallApps\];[\s\S]*?selectedFirstInstallApps\.clear\(\);[\s\S]*?enqueueBackgroundInstalls\(selectedInstalls\)[\s\S]*?onboardingRoute = "apps"/);
    expect(install).toContain('installNativeApp(appId)');
    expect(install).toContain('loadNativeApps().catch(() => nativeApps)');
    expect(install).toContain('savedNativeApps.add(appId)');
    expect(queue).toContain('const unique = [...new Set(appIds)].filter');
    expect(queue).toContain('for (const appId of unique) await startBackgroundInstall(appId)');
  });

  it("keeps saved-account migration on its own local-only page without account discovery claims", () => {
    const tutorial = functionSource("tutorialContent", "selectedNativeApps");
    const browser = functionSource("browserImportContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindBrowserImportControls", "importIdentityForm");
    expect(tutorial).not.toContain("data-browser-import");
    expect(browser).toContain("Import browser accounts");
    expect(browser).toContain("Local-only consent");
    expect(browser).toContain('id="import-saved-accounts"');
    expect(browser).toContain('id="install-firefox"');
    expect(browser).toContain('firefoxStatus.availability !== "installed"');
    expect(browser.match(/id="import-saved-accounts"/g)).toHaveLength(1);
    expect(browser).toContain("Advanced browser export");
    expect(browser).toContain("Prepare export in");
    expect(browser).toContain("OSL does not scrape or decrypt browser databases");
    expect(browser).toContain("does not scrape or decrypt browser databases, discover accounts");
    expect(browser).toContain("MFA and CAPTCHA still apply");
    expect(binding).toContain("beginBrowserAccountImport()");
    expect(binding).toContain('onboardingRoute = "mullvad"');
    expect(source).not.toContain("browserPasswordImportOptIn");
    expect(source).not.toContain("data-browser-password-import");
    expect(source).not.toContain("browser-password-import-opt-in");
  });

  it("shows the recovery title without the removed grey subtitle", () => {
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(recovery).toContain("Save your recovery kit");
    expect(recovery).not.toContain("OSL cannot retrieve these later");
    expect(recovery).not.toContain('class="compact-lead"');
  });

  it("continues from saved recovery material into the tutorial", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(binding).toMatch(/#recovery-continue[\s\S]*?recoveryBundle = null;[\s\S]*?onboardingRoute = "tutorial";[\s\S]*?render\(\)/);
  });

  it("offers an explicit manual-setup escape from the tutorial", () => {
    const onboardingRender = functionSource("renderOnboarding", "onboardingContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(onboardingRender).toContain('id="skip-onboarding"');
    expect(onboardingRender).toContain('id="onboarding-back"');
    expect(onboardingRender).toContain("Skip · manual setup");
    expect(onboardingRender).toContain('["tutorial", "detected", "install", "apps", "browser", "mullvad", "sending", "passwords", "burnpass", "privacy", "scrub"]');
    expect(binding).toContain('document.querySelector("#skip-onboarding")?.addEventListener("click"');
    expect(binding).toContain('document.querySelector("#onboarding-back")?.addEventListener("click"');
  });

  it("guides through one real app connection before saved accounts", () => {
    const apps = functionSource("onboardingAppsContent", "browserImportContent");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const workspaceBinding = functionSource("bindWorkspace", "openHomeAppFromLauncher");
    expect(apps).toContain("Connect one app");
    expect(apps).toContain('data-connect-app-choice="${app.id}"');
    expect(apps).toContain('id="continue-connect-app"');
    expect(binding).toMatch(/#continue-install-apps[\s\S]*?onboardingRoute = "apps"/);
    expect(binding).toMatch(/#continue-connect-app[\s\S]*?onboardingServiceSetup = true[\s\S]*?route = "service"/);
    expect(source).toMatch(/async function continueOnboardingFromService[\s\S]*?onboardingRoute = "browser"/);
    expect(workspaceBinding).toMatch(/#service-guide-finish[\s\S]*?onboardingRoute = "browser"/);
    expect(source).toContain("onboardingServiceSetup && (activeEmbeddedHost || activeNativeHostId)");
  });

  it("places featured local Scrub last before Home", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const completion = functionSource("completeOnboarding", "bindPasswordForm");
    const scrub = functionSource("onboardingScrubContent", "bindOnboarding");
    expect(binding).toMatch(/#continue-mullvad[\s\S]*?onboardingRoute = "sending"/);
    expect(binding).toMatch(/#skip-mullvad[\s\S]*?onboardingRoute = "sending"/);
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?canCompleteSetup\(setup\)[\s\S]*?onboardingRoute = "passwords"/);
    expect(source).toContain('data-password-role-next="${next}"');
    expect(binding).toContain('button.dataset.passwordRoleNext as OnboardingRoute');
    expect(binding).toMatch(/#continue-onboarding-privacy[\s\S]*?onboardingRoute = "scrub"/);
    expect(binding).toContain('document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding())');
    expect(source).toContain('id="route-heading" tabindex="-1">Initialize Scrub');
    expect(source).toContain("Nothing is uploaded or deleted.");
    expect(source).toContain('id="initialize-scrub"');
    expect(scrub).not.toContain('id="privacy-export-input"');
    expect(source).not.toContain('id="onboarding-start-scrub"');
    expect(completion.indexOf("await loadNativeApps()")).toBeGreaterThan(completion.indexOf("await saveOnboardingPreferences"));
    expect(completion).toContain('route = "home"');
  });

  it("offers an optional fixed Mullvad handoff without claiming tunnel access", () => {
    const content = functionSource("mullvadSetupContent", "scrubCategoryChooserMarkup");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(content).toContain("Optional. Connect before opening your apps.");
    expect(content).toContain('id="install-mullvad"');
    expect(content).toContain('id="open-mullvad"');
    expect(content).toContain('id="refresh-mullvad"');
    expect(content).toContain("OSL opens Mullvad but cannot read your account, traffic, settings, or connection.");
    expect(content).toContain('id="mullvad-connected"');
    expect(content).toContain('id="continue-mullvad"');
    expect(binding).toContain('runMullvadSetupAction("install")');
    expect(binding).toContain('runMullvadSetupAction("open")');
  });

  it("offers guarded sending choices without overstating placement support", () => {
    const content = functionSource("sendingSetupContent", "onboardingPasswordRoleContent");
    expect(content).toContain("Choose how to send");
    expect(content).toContain("manualSendingAnimationMarkup(selectedMode)");
    expect(content).toContain("Never presses Send");
    expect(content).toContain('option("double", "Double Enter"');
    expect(content).toContain('option("single", "Single Enter"');
    expect(content).toContain("If OSL cannot prove the destination");
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
    expect(privacy).toContain("Unavailable during setup");
    expect(privacy).not.toContain('id="decrypt-display"');
    expect(binding).toContain("changeScreenshotProtection");
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
