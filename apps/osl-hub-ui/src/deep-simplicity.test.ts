import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it, vi } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

describe("radical simplicity on deep screens", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("Keep advanced implementation concepts behind detail surfaces", async () => {
    const { __oslHubUiTest } = await loadUi();
    const bannedMainSurfaceConcepts =
      /\b(?:keyservers?|ratchets?|browser profiles?|provider adapters?|protocol state|storage layout|automation internals|transport plumbing|service-adapter mechanics)\b/i;
    const primaryRoutes = ["home", "inbox", "people", "privacy", "activity", "connections"] as const;

    for (const route of primaryRoutes) {
      __oslHubUiTest.reset({ route });
      const html = __oslHubUiTest.renderWorkspaceContent(route);
      expect(html).not.toMatch(bannedMainSurfaceConcepts);
    }

    __oslHubUiTest.reset({ route: "settings" });
    const settings = __oslHubUiTest.renderWorkspaceContent("settings");
    expect(settings).not.toMatch(bannedMainSurfaceConcepts);
    expect(settings).toContain('<details class="recovery-import settings-disclosure">');
    expect(settings).toContain('<details class="license-card settings-disclosure">');

    __oslHubUiTest.reset({ route: "privacy" });
    const scrubSettings = __oslHubUiTest.renderSettingsSection("scrub");
    expect(scrubSettings).toContain('<details class="privacy-technical settings-disclosure">');
    expect(scrubSettings.indexOf("Review an export")).toBeLessThan(
      scrubSettings.indexOf('<details class="privacy-technical settings-disclosure">'),
    );

    const inverted = `${settings}<nav>keyservers ratchets browser profiles provider adapters</nav>`;
    expect(inverted).toMatch(bannedMainSurfaceConcepts);
  });

  it("uses one shared disclosure pattern for secondary settings", () => {
    const account = functionSource("identitySettingsContent", "activationSettingsContent");
    const apps = functionSource("serviceAccountsSettingsContent", "scanPrivacyExport");
    const notifications = functionSource("notificationSettingsContent", "identitySettingsContent");
    const about = functionSource("updateSettingsContent", "bindUpdateControls");

    expect(account).toContain("settings-disclosure");
    expect(apps).toContain("Browser for web apps");
    expect(apps).not.toContain("How sign-ins stay private");
    expect(notifications).toContain("Provider unread counts are not read");
    expect(about).toContain("Update privacy");
    expect(styles).toContain(".settings-disclosure");
  });

  it("keeps transport-gated Scrub automation behind the manual scan", async () => {
    const { __oslHubUiTest } = await loadUi();
    const scrub = __oslHubUiTest.renderSettingsSection("scrub");

    expect(scrub.indexOf('for="privacy-export-input"')).toBeLessThan(scrub.indexOf("autoscrub-disclosure"));
    expect(scrub).toContain("Before deleting anything");
    expect(scrub).toContain("Check the original app and delete each message yourself.");
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
