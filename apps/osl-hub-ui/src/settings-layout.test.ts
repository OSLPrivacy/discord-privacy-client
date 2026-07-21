import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("simplified truthful settings", () => {
  it("separates the six understandable settings destinations", () => {
    const start = source.indexOf("function settingsContent");
    const end = source.indexOf("function settingsSectionContent", start);
    const settings = source.slice(start, end);
    expect(settings).toContain('["account", "Account"]');
    expect(settings).toContain('["apps", "Apps"]');
    expect(settings).toContain('["scrub", "Scrub"]');
    expect(settings).toContain('["notifications", "Notifications"]');
    expect(settings).toContain('["appearance", "Appearance"]');
    expect(settings).toContain('["about", "About"]');
    expect(settings).not.toContain('["sending", "Sending"]');
    expect(settings).not.toContain('["privacy", "Privacy"]');
  });

  it("does not advertise automatic sending as functional", () => {
    expect(source).toContain("OSL encrypts and copies. You choose where and when to send.");
    expect(source).not.toContain("If OSL cannot prove the destination, it copies the encrypted text and sends nothing.");
    expect(source).toContain("data-settings-send-mode");
    expect(source).not.toMatch(/Advanced sending preview|data-placement/);
    expect(source).not.toContain("Composer adapter required");
  });

  it("does not let users create inactive stealth or burn passwords", () => {
    const start = source.indexOf("function passwordSecuritySettingsContent");
    const end = source.indexOf("function accountAdvancedSettingsContent", start);
    const security = source.slice(start, end);
    expect(security).toContain("if (!wired)");
    expect(security).toContain('aria-disabled="true"');
    expect(security).toContain("OSL will not let you create or rely on it.");
    expect(security.indexOf("if (!wired)")).toBeLessThan(security.indexOf('data-password-role="${role}"'));
  });

  it("keeps full local cleanup in collapsed Account advanced settings", () => {
    const accountStart = source.indexOf("function accountAdvancedSettingsContent");
    const accountEnd = source.indexOf("function serviceAccountsSettingsContent", accountStart);
    const scrubStart = source.indexOf("function privacySettingsContent");
    const scrubEnd = source.indexOf("function privacyScanResultsMarkup", scrubStart);
    expect(source.slice(accountStart, accountEnd)).toContain('<details class="account-advanced settings-disclosure">');
    expect(source.slice(accountStart, accountEnd)).toContain('id="full-cleanup-button"');
    expect(source.slice(scrubStart, scrubEnd)).not.toContain('id="full-cleanup-button"');
  });

  it("clears a newly created identity recovery phrase on navigation and backgrounding", () => {
    expect(source).toContain('if (route === "settings" && settingsSection === "account") newIdentityRecoveryPhrase = null;');
    expect(source).toContain('if (settingsSection === "account" && next !== "account") newIdentityRecoveryPhrase = null;');
    expect(source).toMatch(/document\.addEventListener\("visibilitychange"[\s\S]*?newIdentityRecoveryPhrase = null/);
    expect(source).toContain("It clears if you leave or hide OSL.");
  });

  it("reloads all identity-scoped state after switching or burning the account", () => {
    expect(source).toMatch(/async function refreshIdentityScopedState[\s\S]*?loadFriendProfile\(\)[\s\S]*?listHubPeople\(\)[\s\S]*?loadLinkedServices\(\)/);
    expect(source).toMatch(/async function switchIdentity[\s\S]*?refreshIdentityScopedState\(\)/);
    expect(source).toMatch(/async function executeBurn[\s\S]*?executeHubFullCleanup\(\)[\s\S]*?refreshIdentityScopedState\(\)/);
  });

  it("exposes only the bounded embedded service surface", () => {
    expect(source).toContain("openEmbeddedHomeApp");
    expect(source).toContain("closeEmbeddedServiceHost");
    expect(source).not.toContain("setServiceHostLayout");
  });

  it("lists isolated embedded profiles without shared-browser actions", () => {
    expect(source).toContain("Open each account in its own OSL profile.");
    expect(source).not.toContain('data-firefox-launch=');
    expect(source).not.toContain('data-install-firefox');
    expect(source).not.toContain("downloadSetupCsv");
  });

  it("keeps account-opening choices editable without duplicating onboarding browser import", () => {
    const start = source.indexOf("function serviceAccountsSettingsContent");
    const end = source.indexOf("function privacySettingsContent", start);
    const apps = source.slice(start, end);
    expect(apps).toContain("Account opening");
    expect(apps).toContain('data-saved-account-mode="use"');
    expect(apps).toContain('data-saved-account-mode="clean"');
    expect(apps).toContain('data-saved-native="${app.id}"');
    expect(apps).toContain("Use selected apps");
    expect(apps).toContain("Use web profiles");
    expect(apps).toContain("Only checked installed apps may open");
    expect(apps).not.toContain("data-browser-import");
    expect(apps).not.toContain("Prepare export in");
    expect(apps).not.toContain("data-browser-password-import");
  });

  it("leaves app ordering to Home edit mode", () => {
    const start = source.indexOf("function appearanceSettingsContent");
    const end = source.indexOf("function bindWorkspace", start);
    const appearance = source.slice(start, end);
    expect(appearance).toContain("Arrange apps with Edit on Home.");
    expect(appearance).not.toContain("data-sidebar-move");
    expect(appearance).not.toContain("data-sidebar-toggle");
  });
});
