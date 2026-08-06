import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { SEND_OPTIONS, onboardingSendingMarkup, sendModeIsDangerous } from "./onboarding-sending";
import { defaultSetup } from "./state";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("radically simple onboarding", () => {
  // Protects: the safe path is the one the app recommends by defaulting to it and
  // listing it first, every mode is visible at the same level with its own
  // description, none is buried in a disclosure drawer, and no single mode's
  // animation stands in for the rest.
  it("keeps one recommended sending path and makes every mode visible", () => {
    const sending = onboardingSendingMarkup({ mode: "manual", riskAccepted: false, captureEnabled: false, captureApplied: false });
    expect(SEND_OPTIONS.map((option) => option.mode)).toEqual(["manual", "clipboard", "double", "single"]);
    // The recommendation is the default, and the default is never a risky mode.
    expect(defaultSetup.sendMode).toBe(SEND_OPTIONS[0]!.mode);
    expect(sendModeIsDangerous(defaultSetup.sendMode)).toBe(false);
    // Each mode is described in its own words -- no shared blurb, no silent duplicate.
    expect(new Set(SEND_OPTIONS.map((option) => option.described)).size).toBe(SEND_OPTIONS.length);
    for (const option of SEND_OPTIONS) {
      expect(option.described.length).toBeGreaterThan(0);
      expect(sending).toContain(`data-send-mode="${option.mode}"`);
      expect(sending).toContain(`<strong>${option.name}</strong>`);
    }
    expect(sending).not.toContain("<details");
    expect(source).not.toContain('<details class="send-mode-advanced"');
    expect(source).not.toContain("manualSendingAnimationMarkup");
  });
});

describe("plain-language friends", () => {
  it("keeps security verification while removing invite and scope jargon", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    const dialog = functionSource("friendsDialogMarkup", "serviceContent");
    expect(people).toContain('<details class="friend-security"><summary>Security details</summary>');
    expect(people).toContain('<details class="friend-management"><summary>Manage</summary>');
    expect(people).toContain("Verification code");
    expect(people).toContain("Approved chats");
    expect(people).not.toContain("Cryptographic OSL identity");
    expect(people).not.toContain("out of band");
    expect(dialog).toContain("Paste their invite");
    expect(dialog).toContain("approve each chat separately");
    expect(dialog).not.toMatch(/signed friend|signed OSL|scope approval/i);
  });
});

describe("one-step app guide", () => {
  it("opens the real service with two minimal account choices and no extra disclosure", () => {
    const guide = functionSource("serviceGuideContent", "settingsContent");
    expect(guide).not.toContain("Step ${step + 1} of 3");
    expect(guide).toContain('directNativeAccountChoice');
    expect(guide).toContain('directNativeAccountChoice || directBrowserAccountChoice ? "Open" : "Connect"');
    expect(guide).not.toContain('<details class="guide-details"><summary>Sign-in privacy</summary>');
    expect(guide).toContain('directNativeAccountChoice || directBrowserAccountChoice');
    expect(guide).not.toContain("Choose which account to use.");
    expect(guide).not.toMatch(/adapter|scope|auto-whitelist/i);
  });
});

describe("quiet settings and status", () => {
  it("shows only Ready or Needs attention outside Home and moves detail to About", () => {
    const status = functionSource("simpleDeviceStatusMarkup", "trustedHeader");
    const about = functionSource("updateSettingsContent", "bindUpdateControls");
    expect(status).toContain('ready ? "Ready" : "Needs attention"');
    expect(status).not.toContain("coreReadinessLabel");
    expect(about).toContain("Device status");
    expect(about).toContain("coreReadinessLabel(core.readiness)");
  });

  it("keeps only theme controls in Appearance", () => {
    const appearance = functionSource("appearanceSettingsContent", "bindWorkspace");
    expect(appearance).toContain("Arrange apps with Edit on Home.");
    expect(appearance).toContain("data-theme-choice");
    expect(appearance).not.toMatch(/data-sidebar|Move or hide apps|serviceRows/);
  });

  it("parses only supported theme choices and delegates first-run migration", () => {
    const parser = functionSource("parseTheme", "parseSavedAccountMode");
    expect(parser).toContain('? raw : "dark"');
    expect(parser).toContain('raw === "system"');
    expect(source).toContain("initializeThemePreference(localStorage)");
    expect(source).toContain("localStorage.setItem(themeStorageKey, next)");
  });

  it("uses notification language people can understand", () => {
    const notifications = functionSource("notificationSettingsContent", "identitySettingsContent");
    expect(notifications).toContain("Unread access is not supported yet");
    expect(notifications).toContain("Suggest chat approval");
    expect(notifications).not.toMatch(/verified unread adapter|verified adapters|scope approval/i);
  });
});
