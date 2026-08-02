import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { nextOnboardingRoute, previousOnboardingRoute } from "./onboarding-sequence";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), emitTo: vi.fn(), getCurrentWindow: vi.fn() }));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({ browserLogo: (id: string) => `<span>${id}</span>`, providerLogo: (id: string) => `<span>${id}</span>`, serviceLogo: (id: string) => `<span>${id}</span>` }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

async function loadUi() {
  vi.resetModules();
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
  vi.stubGlobal("document", { querySelector: vi.fn(() => null), createElement: vi.fn(() => ({})), documentElement: { classList: { add: vi.fn() }, dataset: {} }, addEventListener: vi.fn(), visibilityState: "visible" });
  vi.stubGlobal("window", { addEventListener: vi.fn(), matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })), setTimeout, confirm: vi.fn(() => false) });
  vi.stubGlobal("requestAnimationFrame", () => 1);
  vi.stubGlobal("cancelAnimationFrame", () => undefined);
  return import("./main");
}

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("review defaults onboarding", () => {
  const review = functionSource("reviewDefaultsOnboardingContent", "coverDraftSetupContent");

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows warnings, attachment cleaning, retention, cleanup, and inherited send behavior", () => {
    expect(review).toContain("Review defaults");
    expect(review).toContain("Warn before sending");
    expect(review).toContain("Clean attachments");
    expect(review).toContain("Keep protected drafts");
    expect(review).toContain("Delete or clean up history");
    expect(review).toContain("Send behavior");
    expect(review).toContain("formatSendMode(defaultSetup.sendMode)");
  });

  it("starts destructive automation off and requires later review plus confirmation", () => {
    expect(review).toContain("Timed deletion, bulk cleanup, and account cleanup do not run during onboarding.");
    expect(review).toContain('"Delete or clean up history", "Timed deletion, bulk cleanup, and account cleanup do not run during onboarding.", "Off"');
    expect(review).toContain("No destructive action starts from setup.");
    expect(review).toContain("Cleanup requires a separate review and confirmation.");
    expect(review).not.toContain("auto-retry");
    expect(review).not.toContain("retry automatically");
    expect(review).not.toContain("Single Enter");
  });

  it("keeps implementation concepts out of first-run copy", () => {
    expect(review).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter/iu);
  });

  it("is wired between protection presets, the explicit Tor choice, and send setup", async () => {
    const branches = { detected: false, install: false };
    const { reviewDefaultsOnboardingContent, sendingSetupContent } = await loadUi();

    expect(nextOnboardingRoute("privacy", branches)).toBe("defaults");
    expect(previousOnboardingRoute("defaults", branches)).toBe("privacy");
    expect(nextOnboardingRoute("defaults", branches)).toBe("tor");
    expect(previousOnboardingRoute("tor", branches)).toBe("defaults");
    expect(nextOnboardingRoute("tor", branches)).toBe("sending");
    expect(previousOnboardingRoute("sending", branches)).toBe("tor");
    expect(reviewDefaultsOnboardingContent()).toContain('id="continue-defaults-review"');
    expect(sendingSetupContent()).toContain("Choose how to send");
  });
});
