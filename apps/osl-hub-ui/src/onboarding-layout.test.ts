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

  it("places featured local Scrub last before starting the first app guide", () => {
    const binding = functionSource("bindOnboarding", "completeOnboarding");
    const completion = functionSource("completeOnboarding", "bindPasswordForm");
    expect(binding).toMatch(/onboardingRoute !== "sending"[\s\S]*?sendingSetupStep === "send"[\s\S]*?onboardingRoute = "scrub"/);
    expect(binding).toContain('document.querySelector("#complete-onboarding")?.addEventListener("click", () => void completeOnboarding(false))');
    expect(source).toContain('id="route-heading" tabindex="-1">Scrub');
    expect(source).toContain("Your messages never leave this device.");
    expect(completion.indexOf("await loadNativeApps()")).toBeGreaterThan(completion.indexOf("await saveOnboardingPreferences"));
    expect(completion).toMatch(/const selectedApp = homeAppsFromServices\(services\)[\s\S]*?const firstService = startGuide && selectedApp\?\.serviceId[\s\S]*?activeHomeAppId = selectedApp\.id;[\s\S]*?route = "service";[\s\S]*?serviceGuideStep = 0;[\s\S]*?persistServiceGuideState\(\)/);
  });

  it("reveals send behavior and text placement as two simple steps", () => {
    const content = functionSource("sendingSetupContent", "scrubCategoryChooserMarkup");
    expect(content).toContain('sendingSetupStep === "send"');
    expect(content).toContain("Choose how to send");
    expect(content).toContain("Choose how text appears");
    expect(content).toContain('id="sending-setup-back"');
  });

  it("recovers when identity creation succeeds before password setup fails", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding).toMatch(/catch \(failure\)[\s\S]*?core = await loadCoreIntegration\(\)\.catch\(\(\) => core\)/);
    expect(binding).toContain("The identity was created, but password setup did not finish.");
  });

  it("reloads the encrypted service registry after first password setup and recovery", () => {
    const createBinding = functionSource("bindPasswordForm", "bindImportForm");
    const importBinding = functionSource("bindImportForm", "renderWorkspace");
    expect(createBinding).toMatch(/setupHubMainPassword\(secret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
    expect(importBinding).toMatch(/setupHubMainPassword\(passwordSecret\)[\s\S]*?loadCoreIntegration\(\)[\s\S]*?loadLinkedServices\(\)/);
  });

  it("reloads password roles immediately after setup and unlock", () => {
    const binding = functionSource("bindPasswordForm", "bindImportForm");
    expect(binding.match(/loadHubPasswordRoleStatus\(\)/g)).toHaveLength(2);
  });
});
