import { readFileSync } from "node:fs";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

// D-251: `src/main.ts` is ~10k lines, and importing it costs seconds. This file
// used to do that inside the body of every `it()`, where vitest's default
// 5,000 ms `testTimeout` applies, so most of each test's budget went on module
// loading and a busy machine turned the file red with
// `Test timed out in 5000ms` -- without ever reaching an assertion.
//
// The module is now loaded ONCE, in a hook that carries its own budget, and the
// tests read it synchronously. That is safe here and was checked, not assumed:
// the first loading test drives the state it renders from through
// `__oslHubUiTest.reset(...)`, and the second loading test only reads the
// static "Before deleting anything" disclosure copy, which does not depend on
// any state `reset()` touches -- which is exactly the state a fresh import
// would have seen.
const localStore = new Map<string, string>();
let ui: typeof import("./main");

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => localStore.get(key) ?? null,
    setItem: (key: string, value: string) => { localStore.set(key, value); },
    removeItem: (key: string) => { localStore.delete(key); },
    clear: () => { localStore.clear(); },
  });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  vi.resetModules();
  ui = await import("./main");
}, 300_000);

afterAll(() => {
  vi.unstubAllGlobals();
});

beforeEach(() => {
  localStore.clear();
});

describe("radical simplicity on deep screens", () => {
  it("Keep advanced implementation concepts behind detail surfaces", () => {
    const { __oslHubUiTest } = ui;
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

  it("keeps transport-gated Scrub automation behind the manual scan", () => {
    const { __oslHubUiTest } = ui;
    const scrub = __oslHubUiTest.renderSettingsSection("scrub");

    expect(scrub.indexOf('for="privacy-export-input"')).toBeLessThan(scrub.indexOf("autoscrub-disclosure"));
    expect(scrub).toContain("Before deleting anything");
    expect(scrub).toContain("Only a service recheck can verify removal within its stated coverage");
    expect(scrub).toContain("Automatic deletion is unavailable in this build");
    expect(scrub).toContain("native one-shot reviewed-consent capability");
    expect(scrub).toContain("Connect IMAP for read-only verification");
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
