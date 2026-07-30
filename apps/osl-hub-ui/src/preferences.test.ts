import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke,
}));

import { saveFirstRunOnboardingPreferences } from "./preferences";

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear() {
      values.clear();
    },
    getItem(key: string) {
      return values.get(key) ?? null;
    },
    key(index: number) {
      return [...values.keys()][index] ?? null;
    },
    removeItem(key: string) {
      values.delete(key);
    },
    setItem(key: string, value: string) {
      values.set(key, value);
    },
  };
}

describe("first-run onboarding preference persistence", () => {
  beforeEach(() => {
    invoke.mockReset();
    vi.unstubAllGlobals();
  });

  it("persists the selected preset and inherited manual send behavior in browser storage", async () => {
    const storage = memoryStorage();
    vi.stubGlobal("localStorage", storage);

    const saved = await saveFirstRunOnboardingPreferences({
      protectionPreset: "balanced",
      sendBehavior: "inherited",
    });

    expect(saved).toEqual({
      onboardingComplete: true,
      setup: {
        sendMode: "manual",
        placementMode: "atomic",
        acceptedRisk: false,
        acceptedRiskForMode: null,
      },
      showPlaintextPreview: true,
      windowCaptureEnabled: true,
    });
    expect(storage.getItem("osl-preview-onboarded")).toBe("true");
    expect(JSON.parse(storage.getItem("osl-preview-setup") ?? "{}")).toEqual(saved.setup);
    expect(storage.getItem("osl-preview-first-run-preset")).toBe("balanced");
    expect(JSON.parse(storage.getItem("osl-preview-first-run-send-behavior") ?? "{}")).toEqual({
      source: "inherited",
      preset: "balanced",
      mode: "manual",
    });
  });

  it("refuses Double Enter without explicit first-run acknowledgement before saving", async () => {
    const storage = memoryStorage();
    vi.stubGlobal("localStorage", storage);

    await expect(saveFirstRunOnboardingPreferences({
      protectionPreset: "maximum",
      sendBehavior: { mode: "double" },
    })).rejects.toThrow("requires explicit acknowledgement");

    expect(storage.length).toBe(0);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("refuses advanced Single Enter as a first-run send behavior", async () => {
    const storage = memoryStorage();
    vi.stubGlobal("localStorage", storage);

    await expect(saveFirstRunOnboardingPreferences({
      protectionPreset: "balanced",
      sendBehavior: { mode: "single" as never, acknowledgedExperimentalSendRisk: true },
    })).rejects.toThrow("send behavior was refused");

    expect(storage.length).toBe(0);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("sends the resolved first-run preference contract through the native boundary", async () => {
    const storage = memoryStorage();
    vi.stubGlobal("localStorage", storage);
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invoke.mockResolvedValueOnce({
      onboardingComplete: true,
      sendMode: "clipboard",
      placementMode: "atomic",
      showPlaintextPreview: true,
      windowCaptureEnabled: false,
      acknowledgeExperimentalSendRisk: false,
    });

    const saved = await saveFirstRunOnboardingPreferences({
      protectionPreset: "basic",
      sendBehavior: { mode: "clipboard" },
      windowCaptureEnabled: false,
    });

    expect(invoke).toHaveBeenCalledWith("save_onboarding_preferences", {
      preferences: {
        onboardingComplete: true,
        sendMode: "clipboard",
        placementMode: "atomic",
        showPlaintextPreview: true,
        windowCaptureEnabled: false,
        acknowledgeExperimentalSendRisk: false,
      },
    });
    expect(saved.setup.sendMode).toBe("clipboard");
    expect(storage.getItem("osl-preview-first-run-preset")).toBe("basic");
    expect(JSON.parse(storage.getItem("osl-preview-first-run-send-behavior") ?? "{}")).toEqual({
      source: "override",
      preset: "basic",
      mode: "clipboard",
    });
  });
});
