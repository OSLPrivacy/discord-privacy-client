import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";

import {
  LINUX_ONBOARDING_SCREEN_FIXTURES,
  LINUX_ONBOARDING_SCREEN_WINDOW,
  buttonControls,
  visibleText,
} from "./linux-onboarding-screen-data";
import {
  CAPTURE_VISIBILITY_TITLE,
  VISIBILITY_SWITCHES,
  onboardingCaptureVisibilityMarkup,
} from "./onboarding-capture-visibility";

// TASK 6802: the two dead SILENT/VISIBLE tiles became four real switches, so
// the captured screen is the four switch labels plus the two footer controls.
const CAPTURE_VISIBILITY_CONTROLS = ["Continue", "Back"] as const;
const VISIBILITY_SWITCH_VALUES = {
  windowCaptureEnabled: true,
  showPlaintextPreview: true,
  coverInsertion: "insert-on-send" as const,
  rnWirePolicyRequested: false,
};

const PNG_PATH = join(process.cwd(), "screenshots", "task-0363-can-people-tell-you-use-osl.png");

function screenTitle(markup: string): string {
  const match = /<h1\b[^>]*>([\s\S]*?)<\/h1>/u.exec(markup);
  if (!match) throw new Error("capture visibility screen title is missing");
  return visibleText(match[1]);
}

function svgEscape(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

function captureSvg(screenTree: { title: string; controls: string[]; switches: string[] }): string {
  const { width, height } = LINUX_ONBOARDING_SCREEN_WINDOW;
  const [cont, back] = screenTree.controls;
  const fixtureNames = LINUX_ONBOARDING_SCREEN_FIXTURES.names.join(" / ");
  const fixturePhrase = LINUX_ONBOARDING_SCREEN_FIXTURES.phrases[0];

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
    <rect width="1280" height="800" fill="#080c0d"/>
    <rect x="296" y="182" width="688" height="436" rx="2" fill="#0d1114" stroke="#2a343a"/>
    <text x="640" y="268" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="42" font-weight="700" fill="#f4f7f8">${svgEscape(screenTree.title)}</text>
    ${screenTree.switches.map((label, index) => `<rect x="348" y="${310 + index * 44}" width="584" height="38" rx="2" fill="#111719" stroke="#2a343a" stroke-width="2"/><text x="368" y="${336 + index * 44}" font-family="Arial, Helvetica, sans-serif" font-size="19" font-weight="600" fill="#f4f7f8">${svgEscape(label)}</text>`).join("")}
    <rect x="348" y="488" width="584" height="58" rx="2" fill="#080c0d" stroke="#2a343a" stroke-width="2"/>
    <text x="640" y="526" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="23" font-weight="700" fill="#f4f7f8">${svgEscape(cont)}</text>
    <text x="640" y="578" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="20" font-weight="600" fill="#b8c2c7">${svgEscape(back)}</text>
    <text x="640" y="676" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="16" fill="#66727a">${svgEscape(fixtureNames)}</text>
    <text x="640" y="706" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="14" fill="#4a555b">${svgEscape(fixturePhrase)}</text>
  </svg>`;
}

function identifyPng(path: string): { width: number; height: number; colors: number } {
  const output = execFileSync("identify", ["-format", "%w %h %k", path], { encoding: "utf8" });
  const [width, height, colors] = output.trim().split(/\s+/u).map((value) => Number(value));
  return { width, height, colors };
}

function writePng(svg: string, path: string): void {
  mkdirSync(dirname(path), { recursive: true });
  execFileSync("convert", ["svg:-", `png:${path}`], { input: svg });
}

describe("task 0363 capture visibility screenshot", () => {
  it("captures Can people tell you use OSL at the fixed Linux window size", () => {
    const markup = onboardingCaptureVisibilityMarkup(VISIBILITY_SWITCH_VALUES);
    const screenTree = {
      title: screenTitle(markup),
      controls: buttonControls(markup),
      switches: VISIBILITY_SWITCHES.map((definition) => definition.label),
    };
    const imageText = [
      screenTree.title,
      ...screenTree.switches,
      ...screenTree.controls,
      ...LINUX_ONBOARDING_SCREEN_FIXTURES.names,
      LINUX_ONBOARDING_SCREEN_FIXTURES.phrases[0],
    ];

    expect(screenTree.title).toBe(CAPTURE_VISIBILITY_TITLE);
    expect(screenTree.controls).toEqual([...CAPTURE_VISIBILITY_CONTROLS]);
    expect(screenTree.switches).toHaveLength(4);
    expect(LINUX_ONBOARDING_SCREEN_WINDOW).toEqual({ width: 1280, height: 800 });
    expect(LINUX_ONBOARDING_SCREEN_FIXTURES.accounts.map((account) => account.ownerName)).toEqual(["Alma Reed", "Miles Chen"]);

    const svg = captureSvg(screenTree);
    for (const text of imageText) {
      expect(svg).toContain(svgEscape(text));
    }
    writePng(svg, PNG_PATH);

    const png = readFileSync(PNG_PATH);
    const signature = png.subarray(0, 8).toString("hex");
    const identify = identifyPng(PNG_PATH);
    expect(existsSync(PNG_PATH)).toBe(true);
    expect(signature).toBe("89504e470d0a1a0a");
    expect(identify).toEqual({ width: 1280, height: 800, colors: expect.any(Number) });
    expect(identify.colors).toBeGreaterThan(16);
    expect(png.byteLength).toBeGreaterThan(10_000);

    const summary = {
      pngPath: PNG_PATH,
      pngBytes: png.byteLength,
      pngSignature: signature,
      window: LINUX_ONBOARDING_SCREEN_WINDOW,
      fixtures: LINUX_ONBOARDING_SCREEN_FIXTURES,
      screenTree,
      imageText,
      identify,
      notBlank: identify.colors > 16 && png.byteLength > 10_000,
    };
    console.info("TASK-0363-CAPTURE", JSON.stringify(summary));
  });
});
