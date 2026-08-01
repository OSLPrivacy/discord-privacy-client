import { describe, expect, it } from "vitest";
import {
  clearRecoveryKitUnsaved,
  markRecoveryKitUnsaved,
  RECOVERY_KIT_UNSAVED_STORAGE_KEY,
  recoveryKitUnsaved,
  resumeOnboardingRoute,
  type OnboardingResumeStorage,
} from "./onboarding-resume";

const RESUME_KEY = "osl-onboarding-resume-v1";

/** A storage that behaves like the real one across a "restart". */
function storage(seed: Record<string, string> = {}): OnboardingResumeStorage & { snapshot(): Record<string, string> } {
  const items = new Map(Object.entries(seed));
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => { items.set(key, value); },
    removeItem: (key) => { items.delete(key); },
    snapshot: () => Object.fromEntries(items),
  };
}

describe("T15-A8 a restart cannot skip the recovery step", () => {
  it("resumed at the next onboarding step and skipped recovery before the flag existed", () => {
    // The pre-fix behaviour, kept as the contrast: with nothing outstanding the
    // resume lands on the stored step and recovery is never reached.
    expect(resumeOnboardingRoute(storage({ [RESUME_KEY]: "pro" }), RESUME_KEY)).toBe("pro");
  });

  it("resumes at recovery while the kit is unsaved, whatever step was stored", () => {
    const disk = storage({ [RESUME_KEY]: "pro" });
    markRecoveryKitUnsaved(disk);

    // A relaunch reads the same storage.
    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBe("recovery");
  });

  it("keeps re-offering recovery on every launch until the kit is saved", () => {
    const disk = storage({ [RESUME_KEY]: "browser" });
    markRecoveryKitUnsaved(disk);

    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBe("recovery");
    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBe("recovery");
    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBe("recovery");

    clearRecoveryKitUnsaved(disk);
    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBe("browser");
  });

  it("does not throw away where the owner actually was", () => {
    const disk = storage({ [RESUME_KEY]: "tutorial" });
    markRecoveryKitUnsaved(disk);
    resumeOnboardingRoute(disk, RESUME_KEY);

    expect(disk.getItem(RESUME_KEY)).toBe("tutorial");
  });

  it("persists a flag and nothing else — no recovery secret is ever written", () => {
    const disk = storage();
    markRecoveryKitUnsaved(disk);

    expect(recoveryKitUnsaved(disk)).toBe(true);
    expect(disk.snapshot()).toEqual({ [RECOVERY_KIT_UNSAVED_STORAGE_KEY]: "1" });

    clearRecoveryKitUnsaved(disk);
    expect(recoveryKitUnsaved(disk)).toBe(false);
    expect(disk.snapshot()).toEqual({});
  });

  it("treats an unrecognised stored step as no step at all", () => {
    const disk = storage({ [RESUME_KEY]: "not-a-route" });

    expect(resumeOnboardingRoute(disk, RESUME_KEY)).toBeNull();
    expect(disk.getItem(RESUME_KEY)).toBeNull();
  });

  it("does not resume anywhere from an empty install", () => {
    expect(resumeOnboardingRoute(storage(), RESUME_KEY)).toBeNull();
  });
});
