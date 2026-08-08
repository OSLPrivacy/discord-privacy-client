import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  getLanguage,
  getScreenWords,
  initLanguage,
  loadScreenWords,
  resetLanguageStoreForTest,
  setLanguage,
  subscribeLanguage,
} from "./language-store";
import { initialVerificationWarningScreenState, verificationWarningScreenMarkup } from "./verification-warning-screen";

const EN_VERIFICATION_WARNING = {
  heading: "Verification warning",
  intro: "intro-en",
  legend_label: "legend-en",
  what_this_does_label: "what-en",
  reset_button: "Reset",
  save_button: "Save",
  state_unsaved_template: "unsaved-en {choice}",
  state_saved_template: "saved-en {choice}",
  option_every_time_label: "Every time",
  option_every_time_effect: "effect-en",
  option_once_label: "Once",
  option_once_effect: "effect-en",
  option_before_sending_label: "Before sending",
  option_before_sending_effect: "effect-en",
  option_never_label: "Never",
  option_never_effect: "OSL never reminds you. You can still check a person yourself at any time.",
};

const ES_VERIFICATION_WARNING = {
  ...EN_VERIFICATION_WARNING,
  heading: "Advertencia de verificacion",
  option_never_effect: "OSL nunca te lo recuerda.",
};

function mockInvoke(languageBox: { current: string }) {
  return vi.fn(async (command: string, args?: Record<string, unknown>) => {
    if (command === "osl_get_language_choice") return languageBox.current;
    if (command === "osl_save_language_choice") {
      languageBox.current = args?.language as string;
      return languageBox.current;
    }
    if (command === "osl_read_screen_words") {
      const words = languageBox.current === "es" ? ES_VERIFICATION_WARNING : EN_VERIFICATION_WARNING;
      return { language: languageBox.current, screen: args?.screen, words };
    }
    throw new Error(`unexpected invoke: ${command}`);
  });
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => hoistedInvoke(...(args as [string, Record<string, unknown>?])),
}));

// vi.mock is hoisted above imports, so the mock body cannot close over a
// per-test variable directly; it calls this indirection instead.
let hoistedInvoke: ReturnType<typeof mockInvoke>;

describe("TASK 3161 language store", () => {
  beforeEach(() => {
    resetLanguageStoreForTest();
  });

  it("defaults to English until initLanguage reads the backend's saved choice", async () => {
    hoistedInvoke = mockInvoke({ current: "es" });
    expect(getLanguage()).toBe("en");
    const language = await initLanguage();
    expect(language).toBe("es");
    expect(getLanguage()).toBe("es");
  });

  it("caches a loaded screen's words and notifies subscribers", async () => {
    hoistedInvoke = mockInvoke({ current: "en" });
    let notified = 0;
    subscribeLanguage(() => notified++);

    expect(getScreenWords("verification_warning")).toBeUndefined();
    const words = await loadScreenWords("verification_warning");
    expect(words.heading).toBe("Verification warning");
    expect(getScreenWords("verification_warning")).toBe(words);
    expect(notified).toBe(1);
  });

  it("TASK 3161: changing the language changes what an already-rendered screen shows, with no restart", async () => {
    const box = { current: "en" };
    hoistedInvoke = mockInvoke(box);

    // Load once, in the SAME module instance a real screen would use.
    const englishWords = await loadScreenWords("verification_warning");
    const state = initialVerificationWarningScreenState("never");
    const englishMarkup = verificationWarningScreenMarkup(state, englishWords);
    expect(englishMarkup).toContain("Verification warning");

    let rerenderCount = 0;
    let latestMarkup = englishMarkup;
    const unsubscribe = subscribeLanguage(() => {
      rerenderCount++;
      const words = getScreenWords("verification_warning");
      if (words) latestMarkup = verificationWarningScreenMarkup(state, words);
    });

    // No page reload, no re-import, no new process: same store, same test.
    const newLanguage = await setLanguage("es");

    expect(newLanguage).toBe("es");
    expect(getLanguage()).toBe("es");
    expect(rerenderCount).toBeGreaterThan(0);
    expect(latestMarkup).toContain("Advertencia de verificacion");
    expect(latestMarkup).not.toBe(englishMarkup);

    unsubscribe();
  });

  it("does not re-notify a screen that was never loaded", async () => {
    hoistedInvoke = mockInvoke({ current: "en" });
    let notified = 0;
    subscribeLanguage(() => notified++);

    await setLanguage("es");
    // setLanguage still notifies once for the language change itself, but
    // no screen words were fetched for anything, since nothing was loaded.
    expect(notified).toBe(1);
    expect(getScreenWords("verification_warning")).toBeUndefined();
  });
});
