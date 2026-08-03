/**
 * NEW-3 — "Identity list could not be read" was a dead end that already held
 * the answer.
 *
 * Settings → Account showed that sentence seconds after a first-run account
 * creation that had demonstrably succeeded. The native side always refuses with
 * a specific reason — "OSL main password must be unlocked", "OSL identity
 * migration failed: ...", "OSL identity registry is unavailable" — and
 * `adapters.listHubIdentities` already journals it. The screen dropped it and
 * printed a sentence that named nothing, so the owner (and anyone debugging the
 * report) had no way to tell an unfinished first-run migration from a locked
 * session from a poisoned lock.
 */

import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  const store = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => { store.set(key, value); },
    removeItem: (key: string) => { store.delete(key); },
    clear: () => { store.clear(); },
  });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), querySelectorAll: vi.fn(() => []), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  const ui = await import("./main");
  const journal = await import("./backend-failure");
  return { ui, journal };
}

describe("a refused identity list says why it was refused", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("repeats the native refusal that the retry screen already recorded", async () => {
    const { ui, journal } = await loadUi();
    journal.clearBackendFailures();
    journal.recordBackendFailure("list_hub_identities", "OSL identity migration failed: os error 32");
    ui.__oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });

    const account = ui.__oslHubUiTest.renderSettingsSection("account");

    expect(account).toContain('data-identity-list="unavailable"');
    expect(account).toContain("OSL identity migration failed: os error 32");
    // The unactionable sentence stays as the lead; the reason is added to it.
    expect(account).toContain(ui.IDENTITY_LIST_UNAVAILABLE);
    expect(account).toContain("retry-identity-list");
  });

  it("names a locked session even while the screen claims the account is unlocked", async () => {
    const { ui, journal } = await loadUi();
    journal.clearBackendFailures();
    journal.recordBackendFailure("list_hub_identities", "OSL main password must be unlocked");
    ui.__oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });

    const account = ui.__oslHubUiTest.renderSettingsSection("account");

    // The contradiction is the diagnosis: the renderer's unlock predicate is
    // broader than the backend's, and now that is visible instead of silent.
    expect(account).toContain("Password configured and unlocked");
    // The word after "password" is redacted by the credential rule in
    // `sanitizeBackendMessage`, which is deliberate and stays that way — a
    // redaction control does not get loosened to make a message read better.
    // What survives still identifies the refusal.
    expect(account).toContain("OSL main password [redacted] be unlocked");
  });

  it("adds nothing when the refusal belongs to some other command", async () => {
    const { ui, journal } = await loadUi();
    journal.clearBackendFailures();
    journal.recordBackendFailure("list_hub_people", "OSL people registry is unavailable");
    ui.__oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });

    const account = ui.__oslHubUiTest.renderSettingsSection("account");

    expect(account).toContain('data-identity-list="unavailable"');
    expect(account).not.toContain("OSL people registry is unavailable");
    expect(account).toContain(ui.IDENTITY_LIST_UNAVAILABLE);
  });

  it("never renders a refusal as markup", async () => {
    const { ui, journal } = await loadUi();
    journal.clearBackendFailures();
    journal.recordBackendFailure("list_hub_identities", "<img src=x onerror=alert(1)>");
    ui.__oslHubUiTest.reset({ coreReady: true, hubIdentities: [], hubIdentitiesLoad: "unavailable" });

    const account = ui.__oslHubUiTest.renderSettingsSection("account");

    expect(account).not.toContain("<img src=x");
    expect(account).toContain("&lt;img src=x");
  });
});
