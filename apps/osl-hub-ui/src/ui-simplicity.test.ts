import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("radically simple onboarding", () => {
  it("shows one truthful sending path instead of unavailable automation", () => {
    const sending = functionSource("sendingSetupContent", "scrubCategoryChooserMarkup");
    expect(sending).toContain("Send with copy & paste");
    expect(sending).toContain("manualSendingAnimationMarkup()");
    expect(sending).not.toMatch(/Single Enter|Double Enter|Keystrokes|Experimental|Recommended/);
  });
});

describe("plain-language friends", () => {
  it("keeps security verification while removing invite and scope jargon", () => {
    const people = functionSource("peopleListMarkup", "peopleDialogMarkup");
    const dialog = functionSource("friendsDialogMarkup", "serviceContent");
    expect(people).toContain('<details class="friend-security"><summary>Security details</summary>');
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
  it("opens the real service with minimal copy and a short privacy disclosure", () => {
    const guide = functionSource("serviceGuideContent", "settingsContent");
    expect(guide).not.toContain("Step ${step + 1} of 3");
    expect(guide).toContain("Open ${name} in OSL");
    expect(guide).toContain("Choose where to open it.");
    expect(guide).toContain("Open installed ${name}");
    expect(guide).toContain('<details class="guide-details"><summary>Sign-in privacy</summary>');
    expect(guide).not.toMatch(/adapter|scope|auto-whitelist/i);
  });
});

describe("quiet settings and status", () => {
  it("shows only Ready or Needs attention outside Home and moves detail to About", () => {
    const status = functionSource("simpleDeviceStatusMarkup", "trustedHeader");
    const about = functionSource("updateSettingsContent", "bindUpdateControls");
    expect(status).toContain('ready ? "Ready" : "Needs attention"');
    expect(status).not.toContain("coreReadinessLabel");
    expect(about).toContain("Diagnostics");
    expect(about).toContain("coreReadinessLabel(core.readiness)");
  });

  it("keeps only theme controls in Appearance", () => {
    const appearance = functionSource("appearanceSettingsContent", "bindWorkspace");
    expect(appearance).toContain("Arrange apps with Edit on Home.");
    expect(appearance).toContain("data-theme-choice");
    expect(appearance).not.toMatch(/data-sidebar|Move or hide apps|serviceRows/);
  });

  it("uses notification language people can understand", () => {
    const notifications = functionSource("notificationSettingsContent", "identitySettingsContent");
    expect(notifications).toContain("Unread access is not supported yet");
    expect(notifications).toContain("Suggest chat approval");
    expect(notifications).not.toMatch(/verified unread adapter|verified adapters|scope approval/i);
  });
});
