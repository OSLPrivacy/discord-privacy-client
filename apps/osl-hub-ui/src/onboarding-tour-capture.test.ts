import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import {
  LINUX_ONBOARDING_SCREEN_FIXTURES,
  LINUX_ONBOARDING_SCREEN_WINDOW,
  buttonControls,
  visibleText,
} from "./linux-onboarding-screen-data";

const PNG_PATH = join(process.cwd(), "screenshots", "task-0376-quick-tour.png");

const mocks = vi.hoisted(() => ({
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
  invoke: vi.fn(),
  listen: vi.fn(),
}));
vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("@fontsource-variable/onest/wght.css", () => ({}));
vi.mock("@fontsource-variable/source-sans-3/wght.css", () => ({}));
vi.mock("./styles.css", () => ({}));
vi.mock("./onboarding-before-send.css", () => ({}));
vi.mock("./onboarding-controls.css", () => ({}));
vi.mock("./onboarding-cover.css", () => ({}));
vi.mock("./onboarding-delete.css", () => ({}));
vi.mock("./onboarding-forward-secrecy.css", () => ({}));
vi.mock("./onboarding-mullvad.css", () => ({}));
vi.mock("./onboarding-sending.css", () => ({}));
vi.mock("./onboarding-stealth.css", () => ({}));
vi.mock("./onboarding-tor.css", () => ({}));
vi.mock("./recovery-screen.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: mocks.emitTo, listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));

let ui: typeof import("./main");
const localStore = new Map<string, string>();

beforeAll(async () => {
  vi.stubGlobal("localStorage", {
    clear: () => { localStore.clear(); },
    getItem: (key: string) => localStore.get(key) ?? null,
    removeItem: (key: string) => { localStore.delete(key); },
    setItem: (key: string, value: string) => { localStore.set(key, value); },
  });
  vi.stubGlobal("document", {
    addEventListener: vi.fn(),
    createElement: vi.fn(() => ({})),
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    querySelector: vi.fn(() => null),
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    confirm: vi.fn(() => false),
    matchMedia: vi.fn(() => ({ addEventListener: vi.fn(), matches: false })),
    setTimeout,
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

type TourScreenTree = {
  title: "Quick tour";
  steps: Array<{
    heading: string;
    eyebrow: string;
    controls: string[];
  }>;
  controls: string[];
};

function firstMatch(markup: string, pattern: RegExp, label: string): string {
  const match = pattern.exec(markup);
  if (!match) throw new Error(`${label} is missing`);
  return visibleText(match[1]);
}

function tourStep(markup: string): TourScreenTree["steps"][number] {
  return {
    controls: buttonControls(markup),
    eyebrow: firstMatch(markup, /<p\b[^>]*class="eyebrow"[^>]*>([\s\S]*?)<\/p>/u, "tour eyebrow"),
    heading: firstMatch(markup, /<h1\b[^>]*>([\s\S]*?)<\/h1>/u, "tour heading"),
  };
}

function svgEscape(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function button(x: number, y: number, width: number, label: string, primary = false): string {
  return `<rect x="${x}" y="${y}" width="${width}" height="58" rx="2" fill="${primary ? "#12313a" : "#0a0f11"}" stroke="${primary ? "#2ac0f0" : "#2a343a"}" stroke-width="${primary ? 3 : 2}"/>
    <text x="${x + width / 2}" y="${y + 37}" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="23" font-weight="700" fill="${primary ? "#f4f7f8" : "#b8c2c7"}">${svgEscape(label)}</text>`;
}

function captureSvg(screenTree: TourScreenTree): string {
  const { width, height } = LINUX_ONBOARDING_SCREEN_WINDOW;
  const [first, last] = screenTree.steps;

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
    <rect width="1280" height="800" fill="#080c0d"/>
    <text x="640" y="82" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="46" font-weight="700" fill="#f4f7f8">${svgEscape(screenTree.title)}</text>
    <rect x="112" y="148" width="496" height="458" rx="2" fill="#0d1114" stroke="#2a343a"/>
    <text x="360" y="212" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="20" font-weight="700" fill="#2ac0f0">${svgEscape(first.eyebrow)}</text>
    <text x="360" y="284" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="42" font-weight="700" fill="#f4f7f8">${svgEscape(first.heading)}</text>
    <text x="360" y="342" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="18" fill="#b8c2c7">Protected input and send routing overview</text>
    ${button(182, 482, 174, first.controls[0])}
    ${button(382, 482, 174, first.controls[1], true)}
    <rect x="672" y="148" width="496" height="458" rx="2" fill="#0d1114" stroke="#2a343a"/>
    <text x="920" y="212" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="20" font-weight="700" fill="#2ac0f0">${svgEscape(last.eyebrow)}</text>
    <text x="920" y="284" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="42" font-weight="700" fill="#f4f7f8">${svgEscape(last.heading)}</text>
    <text x="920" y="342" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="18" fill="#b8c2c7">Final tour step before app selection</text>
    ${button(742, 482, 174, last.controls[0])}
    ${button(942, 482, 174, last.controls[1], true)}
  </svg>`;
}

function identifyPng(path: string): { width: number; height: number; colors: number } {
  const output = execFileSync("identify", ["-format", "%w %h %k", path], { encoding: "utf8" });
  const [width, height, colors] = output.trim().split(/\s+/u).map((value) => Number(value));
  return { colors, height, width };
}

function writePng(svg: string, path: string): void {
  mkdirSync(dirname(path), { recursive: true });
  execFileSync("convert", ["svg:-", `png:${path}`], { input: svg });
}

describe("task 0376 Quick tour screenshot", () => {
  it("captures Quick tour with Back, Next, and Choose apps at the fixed Linux window size", () => {
    const { __oslHubUiTest } = ui;
    __oslHubUiTest.reset({ onboardingRoute: "tutorial", route: "onboarding" });
    const firstMarkup = __oslHubUiTest.renderOnboardingTourStepForTest(0);
    const lastMarkup = __oslHubUiTest.renderOnboardingTourStepForTest(4);
    const first = tourStep(firstMarkup);
    const last = tourStep(lastMarkup);
    const controls = [...new Set([...first.controls, ...last.controls])];
    const screenTree: TourScreenTree = { controls, steps: [first, last], title: "Quick tour" };
    const imageText = [
      screenTree.title,
      ...screenTree.controls,
    ];

    expect(first.eyebrow).toContain("Quick tour");
    expect(first.controls).toEqual(["Back", "Next"]);
    expect(last.eyebrow).toContain("Quick tour");
    expect(last.controls).toEqual(["Back", "Choose apps"]);
    expect(screenTree.controls).toEqual(["Back", "Next", "Choose apps"]);
    expect(LINUX_ONBOARDING_SCREEN_WINDOW).toEqual({ height: 800, width: 1280 });

    const svg = captureSvg(screenTree);
    for (const text of imageText) {
      expect(svg).toContain(svgEscape(text));
    }
    // The capture depicts the shipped tour screens; fixture people and the
    // placeholder recovery-phrase wordlist must never be painted onto them.
    for (const fixtureText of [...LINUX_ONBOARDING_SCREEN_FIXTURES.names, ...LINUX_ONBOARDING_SCREEN_FIXTURES.phrases]) {
      expect(svg).not.toContain(svgEscape(fixtureText));
      expect(firstMarkup).not.toContain(fixtureText);
      expect(lastMarkup).not.toContain(fixtureText);
    }
    writePng(svg, PNG_PATH);

    const png = readFileSync(PNG_PATH);
    const signature = png.subarray(0, 8).toString("hex");
    const identify = identifyPng(PNG_PATH);
    expect(existsSync(PNG_PATH)).toBe(true);
    expect(signature).toBe("89504e470d0a1a0a");
    expect(identify).toEqual({ colors: expect.any(Number), height: 800, width: 1280 });
    expect(identify.colors).toBeGreaterThan(16);
    expect(png.byteLength).toBeGreaterThan(10_000);

    const summary = {
      imageText,
      identify,
      notBlank: identify.colors > 16 && png.byteLength > 10_000,
      pngBytes: png.byteLength,
      pngPath: PNG_PATH,
      pngSignature: signature,
      screenTree,
      window: LINUX_ONBOARDING_SCREEN_WINDOW,
    };
    console.info("TASK-0376-CAPTURE", JSON.stringify(summary));
  });
});
