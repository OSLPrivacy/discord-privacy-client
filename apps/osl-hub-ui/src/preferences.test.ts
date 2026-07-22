import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  isTauriRuntime,
  loadOnboardingPreferences,
  saveOnboardingPreferences,
} from "./preferences";
import { defaultOnboardingPreferences } from "./state";

function fakeLocalStorage() {
  const store = new Map<string, string>();
  return {
    store,
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    setItem: (key: string, value: string) => {
      store.set(key, String(value));
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
    clear: () => store.clear(),
  };
}

describe("isTauriRuntime trust gate", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("is false with no window (the vitest node default) — the browser/untrusted path", () => {
    expect(typeof window).toBe("undefined");
    expect(isTauriRuntime()).toBe(false);
  });

  it("is false when a window exists but does not expose the Tauri internals bridge", () => {
    vi.stubGlobal("window", {});
    expect(isTauriRuntime()).toBe(false);
  });

  it("is true only when the injected Tauri internals bridge is present", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(isTauriRuntime()).toBe(true);
  });
});

describe("loadOnboardingPreferences runtime branching", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("outside Tauri and without storage, returns a fresh clone of the defaults and never touches IPC", async () => {
    const loaded = await loadOnboardingPreferences();
    expect(loaded).toEqual(defaultOnboardingPreferences);
    expect(loaded).not.toBe(defaultOnboardingPreferences);
    expect(loaded.setup).not.toBe(defaultOnboardingPreferences.setup);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("outside Tauri, reads onboarding + setup from local browser storage without IPC", async () => {
    const storage = fakeLocalStorage();
    storage.setItem("osl-preview-onboarded", "true");
    storage.setItem(
      "osl-preview-setup",
      JSON.stringify({ sendMode: "manual", placementMode: "compatibility", acceptedRisk: false, acceptedRiskForMode: null }),
    );
    vi.stubGlobal("window", {});
    vi.stubGlobal("localStorage", storage);

    const loaded = await loadOnboardingPreferences();

    expect(loaded.onboardingComplete).toBe(true);
    expect(loaded.setup.placementMode).toBe("compatibility");
    expect(loaded.showPlaintextPreview).toBe(true);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("inside Tauri, reads through the single IPC command and does not fall back to storage", async () => {
    const storage = fakeLocalStorage();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    vi.stubGlobal("localStorage", storage);
    mocks.invoke.mockResolvedValueOnce({
      onboardingComplete: true,
      sendMode: "manual",
      placementMode: "atomic",
      showPlaintextPreview: false,
      acknowledgeExperimentalSendRisk: false,
    });

    const loaded = await loadOnboardingPreferences();

    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_onboarding_preferences");
    expect(loaded.showPlaintextPreview).toBe(false);
    expect(loaded.onboardingComplete).toBe(true);
    expect(storage.store.size).toBe(0);
  });
});

describe("saveOnboardingPreferences runtime branching", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("inside Tauri, persists the sanitized wire shape through IPC and returns the parsed result", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    mocks.invoke.mockImplementationOnce(async (_cmd: string, args: { preferences: unknown }) => args.preferences);

    const saved = await saveOnboardingPreferences({
      ...defaultOnboardingPreferences,
      onboardingComplete: true,
      showPlaintextPreview: false,
    });

    expect(mocks.invoke).toHaveBeenCalledOnce();
    const [command, payload] = mocks.invoke.mock.calls[0];
    expect(command).toBe("save_onboarding_preferences");
    expect(payload).toHaveProperty("preferences.sendMode", "manual");
    expect(saved.showPlaintextPreview).toBe(false);
  });

  it("outside Tauri, writes both browser keys and returns without IPC", async () => {
    const storage = fakeLocalStorage();
    vi.stubGlobal("window", {});
    vi.stubGlobal("localStorage", storage);

    const saved = await saveOnboardingPreferences({
      ...defaultOnboardingPreferences,
      onboardingComplete: true,
    });

    expect(mocks.invoke).not.toHaveBeenCalled();
    expect(storage.getItem("osl-preview-onboarded")).toBe("true");
    expect(storage.getItem("osl-preview-setup")).toContain("\"sendMode\":\"manual\"");
    expect(saved.onboardingComplete).toBe(true);
  });
});
