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
    expect(source).toContain("Automatic sending is not available yet");
    expect(source).toContain("Use each app normally until OSL can confirm the right chat.");
    expect(source).not.toContain("Composer adapter required");
  });

  it("keeps full local cleanup in collapsed Account advanced settings", () => {
    const accountStart = source.indexOf("function accountAdvancedSettingsContent");
    const accountEnd = source.indexOf("function serviceAccountsSettingsContent", accountStart);
    const scrubStart = source.indexOf("function privacySettingsContent");
    const scrubEnd = source.indexOf("function privacyScanResultsMarkup", scrubStart);
    expect(source.slice(accountStart, accountEnd)).toContain('<details class="account-advanced">');
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
    expect(source).toContain("Each account has its own local sign-in profile inside OSL.");
    expect(source).not.toContain('data-firefox-launch=');
    expect(source).not.toContain('data-install-firefox');
    expect(source).not.toContain("downloadSetupCsv");
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
