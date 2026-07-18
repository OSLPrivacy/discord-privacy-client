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
  it("asks how accounts should open and shows explicit per-app install choices", () => {
    const tutorial = functionSource("tutorialContent", "persistSavedAccountPreferences");
    expect(tutorial).toContain("Choose how accounts open");
    expect(tutorial).toContain("Use existing account");
    expect(tutorial).toContain("Start fresh");
    expect(tutorial).toContain('<fieldset class="saved-account-advanced first-install-apps"><legend>Apps</legend>');
    expect(tutorial).toContain("Nothing opens or installs without your choice.");
    expect(tutorial).toContain('data-saved-native="${app.id}"');
    expect(tutorial).toContain('data-first-install="${app.id}"');
    expect(tutorial).toContain("Install in background");
  });

  it("queues selected first-run installs without delaying the next setup step", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const install = functionSource("startBackgroundInstall", "enqueueBackgroundInstalls");
    const queue = functionSource("enqueueBackgroundInstalls", "nativeHostFailureMessage");
    expect(binding).toMatch(/#continue-account-setup[\s\S]*?const selectedInstalls = \[\.\.\.selectedFirstInstallApps\];[\s\S]*?selectedFirstInstallApps\.clear\(\);[\s\S]*?enqueueBackgroundInstalls\(selectedInstalls\)[\s\S]*?onboardingRoute = "apps"/);
    expect(install).toContain('installNativeApp(appId)');
    expect(install).toContain('loadNativeApps().catch(() => nativeApps)');
    expect(install).toContain('savedNativeApps.add(appId)');
    expect(queue).toContain('const unique = [...new Set(appIds)].filter');
    expect(queue).toContain('for (const appId of unique) await startBackgroundInstall(appId)');
  });

  it("keeps browser credentials browser-owned instead of exposing a fake import toggle", () => {
    const tutorial = functionSource("tutorialContent", "persistSavedAccountPreferences");
    expect(tutorial).toContain("Browser passwords stay in your browser");
    expect(tutorial).toContain("Chrome, Edge, Firefox, Brave, Opera, and Vivaldi");
    expect(tutorial).toContain("OSL never reads their password files");
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
    expect(onboardingRender).toContain("Skip · manual setup");
    expect(binding).toContain('document.querySelector("#skip-onboarding")?.addEventListener("click"');
  });

  it("guides through one real app connection before sending setup", () => {
    const apps = functionSource("onboardingAppsContent", "persistSavedAccountPreferences");
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    expect(apps).toContain("Connect one app");
    expect(apps).toContain('data-onboarding-app="${app.id}"');
    expect(binding).toMatch(/#continue-account-setup[\s\S]*?onboardingRoute = "apps"/);
    expect(binding).toMatch(/\[data-onboarding-app\][\s\S]*?onboardingServiceSetup = true[\s\S]*?route = "service"/);
  });

  it("places featured local Scrub last before Home", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const completion = functionSource("completeOnboarding", "bindPasswordForm");
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?setup\.sendMode = "manual"[\s\S]*?onboardingRoute = "scrub"/);
    expect(binding).toContain('document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding())');
    expect(source).toContain('id="route-heading" tabindex="-1">Try Scrub');
    expect(source).toContain("Your messages never leave this device.");
    expect(source).toContain('id="privacy-export-input"');
    expect(source).not.toContain('id="onboarding-start-scrub"');
    expect(completion.indexOf("await loadNativeApps()")).toBeGreaterThan(completion.indexOf("await saveOnboardingPreferences"));
    expect(completion).toContain('route = "home"');
  });

  it("shows only the manual sending behavior that works in this build", () => {
    const content = functionSource("sendingSetupContent", "scrubCategoryChooserMarkup");
    expect(content).toContain("Send with copy & paste");
    expect(content).toContain("manualSendingAnimationMarkup()");
    expect(content).toContain("You review, copy, paste, and send it yourself.");
    expect(content).not.toMatch(/Single Enter|Double Enter|Choose how to send|Choose how text appears/);
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
