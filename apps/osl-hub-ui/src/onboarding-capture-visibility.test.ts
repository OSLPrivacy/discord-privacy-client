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
  CAPTURE_VISIBILITY_CONTROLS,
  CAPTURE_VISIBILITY_TITLE,
  onboardingCaptureVisibilityMarkup,
} from "./onboarding-capture-visibility";

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

function captureSvg(screenTree: { title: string; controls: string[] }): string {
  const { width, height } = LINUX_ONBOARDING_SCREEN_WINDOW;
  const [silent, visible, cont, back] = screenTree.controls;

  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">
    <rect width="1280" height="800" fill="#080c0d"/>
    <rect x="296" y="182" width="688" height="436" rx="2" fill="#0d1114" stroke="#2a343a"/>
    <text x="640" y="268" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="42" font-weight="700" fill="#f4f7f8">${svgEscape(screenTree.title)}</text>
    <rect x="348" y="330" width="280" height="116" rx="2" fill="#12313a" stroke="#2ac0f0" stroke-width="3"/>
    <text x="488" y="402" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="30" font-weight="700" letter-spacing="3" fill="#f4f7f8">${svgEscape(silent)}</text>
    <rect x="652" y="330" width="280" height="116" rx="2" fill="#111719" stroke="#2a343a" stroke-width="2"/>
    <text x="792" y="402" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="30" font-weight="700" letter-spacing="3" fill="#b8c2c7">${svgEscape(visible)}</text>
    <rect x="348" y="488" width="584" height="58" rx="2" fill="#080c0d" stroke="#2a343a" stroke-width="2"/>
    <text x="640" y="526" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="23" font-weight="700" fill="#f4f7f8">${svgEscape(cont)}</text>
    <text x="640" y="578" text-anchor="middle" font-family="Arial, Helvetica, sans-serif" font-size="20" font-weight="600" fill="#b8c2c7">${svgEscape(back)}</text>
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
    const markup = onboardingCaptureVisibilityMarkup();
    const screenTree = {
      title: screenTitle(markup),
      controls: buttonControls(markup),
    };
    const imageText = [
      screenTree.title,
      ...screenTree.controls,
    ];

    expect(screenTree.title).toBe(CAPTURE_VISIBILITY_TITLE);
    expect(screenTree.controls).toEqual([...CAPTURE_VISIBILITY_CONTROLS]);
    expect(LINUX_ONBOARDING_SCREEN_WINDOW).toEqual({ width: 1280, height: 800 });

    const svg = captureSvg(screenTree);
    for (const text of imageText) {
      expect(svg).toContain(svgEscape(text));
    }
    // The capture depicts the shipped screen; test fixture people and the
    // placeholder recovery-phrase wordlist must never be painted onto it. A
    // privacy screen showing a 12-word phrase-shaped list teaches people the
    // wrong thing about what is safe to show.
    for (const fixtureText of [...LINUX_ONBOARDING_SCREEN_FIXTURES.names, ...LINUX_ONBOARDING_SCREEN_FIXTURES.phrases]) {
      expect(svg).not.toContain(svgEscape(fixtureText));
      expect(markup).not.toContain(fixtureText);
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
      screenTree,
      imageText,
      identify,
      notBlank: identify.colors > 16 && png.byteLength > 10_000,
    };
    console.info("TASK-0363-CAPTURE", JSON.stringify(summary));
  });
});
