import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";

import {
  PRIVACY_LEVEL_IDS,
  PRIVACY_LEVEL_LABELS,
  PRIVACY_LEVEL_RULES,
  privacyLevelEffectLines,
  renderPrivacyLevelScreen,
  type PrivacyLevelId,
} from "./privacy-level-screen";

// TASK 0720: the privacy level setting screen offers Basic, Balanced and
// Maximum, each with one short line per real effect. "Real" is enforced below:
// the screen's mirrored rule table must equal what the real
// `cmd_osl_read_privacy_level_rule_set` command returns, driven through the
// committed `task_0720_privacy_level_cli` example binary - not a copy of the
// numbers inside this test.

const repoRoot = fileURLToPath(new URL("../../..", import.meta.url));
const targetDir = process.env.CARGO_TARGET_DIR ?? join(repoRoot, "target");
const harnessBin = join(targetDir, "debug", "examples", "task_0720_privacy_level_cli");
const homeCargo = join(homedir(), ".cargo", "bin", "cargo");
const cargoBin = process.env.CARGO ?? (existsSync(homeCargo) ? homeCargo : "cargo");
const BUILD_BUDGET_MS = 600_000;

interface BackendLevel {
  level: string;
  label: string;
  beforeSendWarnings: boolean;
  attachmentCleaning: boolean;
  cleanupReviewDays: number;
  publicPostChecks: boolean;
  vpnRequiredActions: boolean;
  protectedContactsRequired: boolean;
}

let backendLevels: BackendLevel[] = [];

describe("TASK 0720 privacy level setting screen", () => {
  beforeAll(() => {
    execFileSync(
      cargoBin,
      ["build", "-p", "ipc", "--example", "task_0720_privacy_level_cli", "--locked"],
      { cwd: repoRoot, stdio: "inherit", timeout: BUILD_BUDGET_MS },
    );
    const stdout = execFileSync(harnessBin, ["levels"], { encoding: "utf8" });
    const parsed = JSON.parse(stdout) as { ok: boolean; levels: BackendLevel[] };
    expect(parsed.ok).toBe(true);
    backendLevels = parsed.levels;
  }, BUILD_BUDGET_MS);

  it("mirrors the real backend rule set for every level, byte for byte", () => {
    expect(backendLevels.map((entry) => entry.level)).toEqual([...PRIVACY_LEVEL_IDS]);
    for (const backend of backendLevels) {
      const id = backend.level as PrivacyLevelId;
      expect(PRIVACY_LEVEL_LABELS[id]).toBe(backend.label);
      expect(PRIVACY_LEVEL_RULES[id]).toEqual({
        beforeSendWarnings: backend.beforeSendWarnings,
        attachmentCleaning: backend.attachmentCleaning,
        cleanupReviewDays: backend.cleanupReviewDays,
        publicPostChecks: backend.publicPostChecks,
        vpnRequiredActions: backend.vpnRequiredActions,
        protectedContactsRequired: backend.protectedContactsRequired,
      });
    }
  });

  it("gives every level one short line per real effect, none omitted", () => {
    for (const id of PRIVACY_LEVEL_IDS) {
      const lines = privacyLevelEffectLines(PRIVACY_LEVEL_RULES[id]);
      expect(lines.map((entry) => entry.effect)).toEqual([
        "warnings",
        "public-posts",
        "attachments",
        "cleanup-review",
        "vpn",
        "contacts",
      ]);
      for (const entry of lines) {
        expect(entry.line.length).toBeGreaterThan(0);
        expect(entry.line.length).toBeLessThanOrEqual(60);
      }
    }
  });

  it("derives the effect lines from the rules, so on and off read differently", () => {
    const basic = privacyLevelEffectLines(PRIVACY_LEVEL_RULES.basic).map((entry) => entry.line);
    const balanced = privacyLevelEffectLines(PRIVACY_LEVEL_RULES.balanced).map((entry) => entry.line);
    const maximum = privacyLevelEffectLines(PRIVACY_LEVEL_RULES.maximum).map((entry) => entry.line);
    expect(basic).not.toEqual(balanced);
    expect(balanced).not.toEqual(maximum);
    expect(balanced.join(" ")).toContain("every 30 days");
    expect(maximum.join(" ")).toContain("every 7 days");
    expect(basic.join(" ")).toContain("No warnings before you send.");
  });

  it("renders the three choices with the selected one marked and its effects listed", () => {
    const markup = renderPrivacyLevelScreen("balanced");
    expect(markup).toContain('id="route-heading" tabindex="-1">Privacy level<');
    for (const id of PRIVACY_LEVEL_IDS) {
      expect(markup).toContain(`data-privacy-level-card="${id}"`);
      expect(markup).toContain(`data-privacy-level-choice="${id}"`);
      expect(markup).toContain(`<strong>${PRIVACY_LEVEL_LABELS[id]}</strong>`);
    }
    expect(markup.match(/ checked\//gu) ?? []).toHaveLength(1);
    expect(markup).toContain('data-privacy-level-card="balanced" data-selected="true"');
    expect(markup).toContain('data-privacy-level-card="basic" data-selected="false"');
    expect(markup).toContain('data-privacy-level-card="maximum" data-selected="false"');
    expect(markup.match(/privacy-level-selected-mark/gu) ?? []).toHaveLength(1);
    expect(markup).toContain("Offers a cleanup review every 30 days.");
    expect(markup.match(/data-privacy-level-effect=/gu) ?? []).toHaveLength(18);
    expect(markup).toContain('id="privacy-level-back"');
  });

  it("marks whichever level is selected, not a hard-coded one", () => {
    for (const id of PRIVACY_LEVEL_IDS) {
      const markup = renderPrivacyLevelScreen(id);
      expect(markup).toContain(`data-privacy-level-card="${id}" data-selected="true"`);
      expect(markup.match(/data-selected="true"/gu) ?? []).toHaveLength(1);
    }
  });
});
