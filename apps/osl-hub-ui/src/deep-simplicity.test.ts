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

describe("radical simplicity on deep screens", () => {
  it("uses one shared disclosure pattern for secondary settings", () => {
    const account = functionSource("identitySettingsContent", "activationSettingsContent");
    const apps = functionSource("serviceAccountsSettingsContent", "scanPrivacyExport");
    const notifications = functionSource("notificationSettingsContent", "identitySettingsContent");
    const about = functionSource("updateSettingsContent", "bindUpdateControls");

    expect(account).toContain("settings-disclosure");
    expect(apps).toContain("How sign-ins stay private");
    expect(notifications).toContain("For future unread support");
    expect(about).toContain("Update privacy");
    expect(styles).toContain(".settings-disclosure");
  });

  it("keeps transport-gated Scrub automation behind the manual scan", () => {
    const scrub = functionSource("privacySettingsContent", "clearPrivacyScanState");
    expect(scrub.indexOf('for="privacy-export-input"')).toBeLessThan(scrub.indexOf("autoscrub-disclosure"));
    expect(scrub).toContain("Before deleting anything");
    expect(scrub).toContain("Only a provider readback can verify removal within its stated coverage");
    expect(scrub).toContain("The default path reuses the account already signed in inside OSL");
    expect(scrub).toContain("Optional: use IMAP instead");
  });

  it("keeps Burn limits visible and secondary options collapsed", () => {
    const burn = functionSource("burnDialogMarkup", "ownedConfirmationMarkup");
    expect(burn.indexOf("Messages and history in the service remain")).toBeLessThan(burn.indexOf('<details class="burn-more">'));
    expect(burn).toContain('<summary>Other options</summary>');
    expect(burn).toContain('id="burn-confirm-submit" type="submit" disabled');
  });

  it("shows a compact friend row before management and security detail", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    expect(people).toContain('<details class="friend-management"><summary>Manage</summary>');
    expect(people).toContain('<details class="friend-security"><summary>Security details</summary>');
    expect(styles).toContain(".friend-management > summary");
  });
});
