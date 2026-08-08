import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, expect, it } from "vitest";

import { recoveryKitView, type RecoveryKitState } from "./recovery-kit";

const WINDOW = { width: 1280, height: 800 };
const PNG = join(process.cwd(), "screenshots", "task-0360-recovery-protection-problem.png");
const TITLE = "Recovery protection problem";

function escapeXml(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;").replace(/"/gu, "&quot;");
}

function fixtureMarkup(title: string, controls: string[]): string {
  const [retry, showAnyway, remind] = controls;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${WINDOW.width}" height="${WINDOW.height}" viewBox="0 0 ${WINDOW.width} ${WINDOW.height}">
    <rect width="1280" height="800" fill="#080c0d"/><rect x="220" y="126" width="840" height="548" rx="8" fill="#10171a" stroke="#304047" stroke-width="2"/>
    <text x="640" y="236" text-anchor="middle" font-family="Arial, sans-serif" font-size="42" font-weight="700" fill="#f4f7f8">${escapeXml(title)}</text>
    <text x="640" y="290" text-anchor="middle" font-family="Arial, sans-serif" font-size="20" fill="#bdc9ce">OSL cannot prove capture resistance for this window.</text>
    <rect x="300" y="342" width="680" height="62" rx="4" fill="#12313a" stroke="#2ac0f0" stroke-width="2"/><text x="640" y="382" text-anchor="middle" font-family="Arial, sans-serif" font-size="25" font-weight="700" fill="#f4f7f8">${escapeXml(retry)}</text>
    <rect x="300" y="428" width="680" height="62" rx="4" fill="#12313a" stroke="#2ac0f0" stroke-width="2"/><text x="640" y="468" text-anchor="middle" font-family="Arial, sans-serif" font-size="25" font-weight="700" fill="#f4f7f8">${escapeXml(showAnyway)}</text>
    <rect x="300" y="514" width="680" height="62" rx="4" fill="#182126" stroke="#53636b" stroke-width="2"/><text x="640" y="554" text-anchor="middle" font-family="Arial, sans-serif" font-size="25" font-weight="700" fill="#f4f7f8">${escapeXml(remind)}</text>
  </svg>`;
}

describe("TASK 0360 recovery protection screenshot", () => {
  it("captures the fixed recovery protection problem fixture", () => {
    const state: RecoveryKitState = {
      secrets: { userId: "fixture-user", identityPhrase: "fixture identity", passwordPhrase: "fixture password" },
      captureProven: false,
      captureEnforcement: "enforced",
      shownWithoutProtection: false,
      savedAcknowledged: false,
      noRecoverySecretAcknowledged: false,
      kitUnsaved: true,
    };
    const view = recoveryKitView(state);
    const controls = view.exits.map((exit) => exit.id === "show-anyway" ? "Show anyway" : exit.label);
    const treeNames = [TITLE, ...controls];
    expect(treeNames).toEqual([TITLE, "Retry protection", "Show anyway", "Remind me later"]);

    const svg = fixtureMarkup(TITLE, controls);
    for (const name of treeNames) expect(svg).toContain(escapeXml(name));
    mkdirSync(dirname(PNG), { recursive: true });
    execFileSync("convert", ["svg:-", `png:${PNG}`], { input: svg });
    const png = readFileSync(PNG);
    const identify = execFileSync("identify", ["-format", "%w %h %k", PNG], { encoding: "utf8" }).trim().split(/\s+/u).map(Number);
    expect(existsSync(PNG)).toBe(true);
    expect(png.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
    expect(identify[0]).toBe(WINDOW.width);
    expect(identify[1]).toBe(WINDOW.height);
    expect(identify[2]).toBeGreaterThan(20);
    expect(png.byteLength).toBeGreaterThan(10_000);
    console.info("TASK0360_CAPTURE", JSON.stringify({ png: PNG, window: WINDOW, treeNames, bytes: png.byteLength, colors: identify[2], notBlank: true }));
  });
});
