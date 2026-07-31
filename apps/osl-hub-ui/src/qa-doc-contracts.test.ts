import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";
import { oslPrimaryDestinations } from "./state";

function readDoc(relativePath: string): string {
  return readFileSync(new URL(`../../../${relativePath}`, import.meta.url), "utf8");
}

function section(source: string, heading: string): string {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => line.trim() === heading);
  expect(start, `missing section ${heading}`).toBeGreaterThanOrEqual(0);
  const headingLevel = heading.match(/^#+/u)?.[0].length ?? 1;
  const end = lines.findIndex((line, index) => {
    if (index <= start) {
      return false;
    }
    const match = /^(#+)\s/u.exec(line);
    return match !== null && match[1]!.length <= headingLevel;
  });
  return lines.slice(start + 1, end < 0 ? undefined : end).join("\n");
}

function parseTable<T extends Record<string, string>>(
  source: string,
  expectedColumns: readonly string[],
): T[] {
  const rows = source
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.startsWith("|") && line.endsWith("|"))
    .map((line) => line.slice(1, -1).split("|").map((cell) => cell.trim().replace(/^`|`$/gu, "")));
  const headerIndex = rows.findIndex((row) => row.join("\0") === expectedColumns.join("\0"));
  expect(headerIndex, `missing table header ${expectedColumns.join(",")}`).toBeGreaterThanOrEqual(0);
  expect(rows[headerIndex + 1]?.every((cell) => /^-+$/u.test(cell))).toBe(true);
  const body: T[] = [];
  for (const cells of rows.slice(headerIndex + 2)) {
    if (cells.length !== expectedColumns.length) {
      break;
    }
    body.push(Object.fromEntries(expectedColumns.map((column, index) => [column, cells[index]!])) as T);
  }
  return body;
}

function fencedTextBlocks(source: string): string[] {
  return [...source.matchAll(/```text\n([\s\S]*?)\n```/gu)].map((match) => match[1]!);
}

function expectSharedMemoryCardsAcrossActiveAccounts(prompts: string): void {
  const rows = parseTable<{
    "Active account/window": string;
    "Prompt source": string;
    "Memory-card route": string;
    "Volatile-status rule": string;
  }>(section(prompts, "## Shared memory-card adoption contract"), [
    "Active account/window",
    "Prompt source",
    "Memory-card route",
    "Volatile-status rule",
  ]);

  expect(rows.map((row) => row["Active account/window"]).sort()).toEqual([
    "Coordinating Telegram `/osl` lane",
    "Existing Discord testing window",
    "Existing OSL Hub/UI window",
    "Existing Scrub window",
    "Existing two-way Opus test window",
    "New website/head-developer lane",
  ]);

  for (const row of rows) {
    expect(row["Memory-card route"].toLowerCase()).toMatch(/\b(?:memory-card|memory card)\b/u);
    expect(row["Memory-card route"].toLowerCase()).toMatch(/\b(?:common update|bootstrap|prompt [a-d]|handoff)\b/u);
    expect(row["Volatile-status rule"].toLowerCase()).toMatch(/\bnever copy\b/u);
    expect(row["Volatile-status rule"].toLowerCase()).toMatch(/\b(?:full master spec|full spec|volatile status)\b/u);
  }
}

function fencedJsonBlocks(source: string): unknown[] {
  return [...source.matchAll(/```json\n([\s\S]*?)\n```/gu)].map((match) =>
    JSON.parse(match[1]!),
  );
}

type UserFacingSurfaceReview =
  | {
    kind: "surface";
    visibleChoices: readonly string[];
    mainVisibleText: string;
  }
  | {
    kind: "refusal";
    mainVisibleText: string;
    consequence: string;
    safeAction: string;
  }
  | {
    kind: "advanced-export";
    mainVisibleText: string;
    machineFields: readonly string[];
  };

function reviewComplexityHidingSurface(surface: UserFacingSurfaceReview): {
  pass: boolean;
  reasons: readonly string[];
} {
  const reasons: string[] = [];
  const visibleText = surface.kind === "surface"
    ? [...surface.visibleChoices, surface.mainVisibleText].join("\n")
    : surface.mainVisibleText;
  const implementationNouns =
    /\b(keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?|protocol state|storage layouts?|automation internals?|transport plumbing|service[- ]adapter mechanics)\b/iu;
  const productNouns =
    /\b(protection|protected|trusted people|people|connected accounts|accounts|private conversations|conversations|cleanup actions|cleanup|activity history|activity|home|inbox|privacy|connections|settings)\b/iu;

  if (implementationNouns.test(visibleText)) {
    reasons.push("visible surface exposes implementation machinery");
  }
  if (!productNouns.test(visibleText)) {
    reasons.push("visible surface does not use the product model");
  }

  if (surface.kind === "refusal") {
    if (surface.consequence.trim().length === 0) {
      reasons.push("refusal omits the plain consequence");
    }
    if (surface.safeAction.trim().length === 0) {
      reasons.push("refusal omits the next safe action");
    }
    if (implementationNouns.test(`${surface.consequence}\n${surface.safeAction}`)) {
      reasons.push("refusal makes the mechanism the answer");
    }
  }

  if (
    surface.kind === "advanced-export" &&
    surface.machineFields.some((field) => !implementationNouns.test(field))
  ) {
    reasons.push("advanced export fields are not clearly secondary machine fields");
  }

  return {
    pass: reasons.length === 0,
    reasons,
  };
}

type CodexCapacityCase = {
  name: string;
  historicalPoolLabel: string;
  currentCapacity: {
    fresh: boolean;
    activeSessionCount: number | null;
    blockedOrSleepingSessions: string[] | null;
    quotaAccountStatus: string;
    expectedTurnCapacity: string;
    machineHeadroom: string;
    ownedFileBound: boolean;
  };
  forbiddenSubstitute: string;
  expectedDecision: "dispatch" | "refuse" | "standby";
  expectedReason: string;
};

function isCodexCapacityExercise(value: unknown): value is { schema: string; cases: CodexCapacityCase[] } {
  return (
    typeof value === "object" &&
    value !== null &&
    "schema" in value &&
    (value as { schema?: unknown }).schema === "osl-codex-routing-capacity-v1" &&
    Array.isArray((value as { cases?: unknown }).cases)
  );
}

function routeCodexCapacity(
  exerciseCase: CodexCapacityCase,
): { decision: "dispatch" | "refuse" | "standby"; reason: string } {
  if (exerciseCase.forbiddenSubstitute !== "none") {
    return { decision: "refuse", reason: "forbidden-substitute" };
  }

  const capacity = exerciseCase.currentCapacity;
  const hasLiveCapacityRecord =
    capacity.fresh === true &&
    typeof capacity.activeSessionCount === "number" &&
    capacity.activeSessionCount >= 0 &&
    Array.isArray(capacity.blockedOrSleepingSessions) &&
    capacity.quotaAccountStatus.startsWith("verified-") &&
    capacity.ownedFileBound === true;
  if (!hasLiveCapacityRecord) {
    return { decision: "standby", reason: "missing-current-capacity" };
  }

  if (
    capacity.quotaAccountStatus !== "verified-enough" ||
    capacity.expectedTurnCapacity !== "enough" ||
    capacity.machineHeadroom !== "enough"
  ) {
    return { decision: "standby", reason: "insufficient-current-capacity" };
  }

  return { decision: "dispatch", reason: "fresh-capacity-and-owned-files" };
}

describe("QA documentation contracts", () => {
  it("warm_agent_baseline_snapshot_for_osl_azure_clients_1_and_2", () => {
    const workflow = readDoc("docs/testing/azure-vm-qa-workflow.md");
    const warmAgentBaseline = parseTable<{
      VM: string;
      "Resource group": string;
      Snapshot: string;
      "Lineage tag": string;
      "Required state": string;
      "Agent baseline": string;
      "Included setup": string;
    }>(section(workflow, "### WARM-agent baseline for the crypto A/B pair"), [
      "VM",
      "Resource group",
      "Snapshot",
      "Lineage tag",
      "Required state",
      "Agent baseline",
      "Included setup",
    ]);

    expect(warmAgentBaseline).toHaveLength(2);
    expect(warmAgentBaseline.map((row) => row.VM).sort()).toEqual([
      "OSL-Azure-Client-1",
      "OSL-Azure-Client-2",
    ]);
    for (const row of warmAgentBaseline) {
      expect(row["Resource group"]).toBe("OSL-TWO-CLIENT-LAB");
      expect(row.Snapshot).toBe(`${row.VM}-WARM-agent-baseline-20260726`);
      expect(row["Lineage tag"]).toBe("warm-iteration");
      expect(row["Required state"]).toBe("deallocated");
      expect(row["Agent baseline"]).toBe("registered-logon-task");
      expect(row["Included setup"].split(";").sort()).toEqual([
        "discord-signed-in",
        "osl-identity-created",
      ]);
      expect(row.Snapshot).not.toMatch(/COLD/u);
    }
  });

  it("scrub_tree_merge_plan_records_source_shas_base_tree_and_exclusive_files", () => {
    const plan = readDoc("docs/plans/scrub-tree-merge-plan.md");
    const sourceRows = parseTable<{
      Line: string;
      "Pinned SHA": string;
      Notes: string;
    }>(section(plan, "## Source SHAs"), ["Line", "Pinned SHA", "Notes"]);
    const sourceShas = new Map(sourceRows.map((row) => [row.Line, row["Pinned SHA"]]));
    expect(sourceShas).toEqual(new Map([
      ["main", "16778b297d3ec8d0358b7d3812a95f4f8443e462"],
      ["f1-footprint", "61933d3a4b50e410e3be1d5e05560d955ee72b4c"],
      ["osl-newest-integration", "403cfa2e090bf76ae4cb2950f3febcc72204fc59"],
    ]));

    const baseRows = parseTable<{
      Field: string;
      Value: string;
    }>(section(plan, "## Base Tree"), ["Field", "Value"]);
    const base = new Map(baseRows.map((row) => [row.Field, row.Value]));
    expect(base.get("Base commit")).toBe("16778b297d3ec8d0358b7d3812a95f4f8443e462");
    expect(base.get("Base tree")).toBe("e85e4bd48bfaab810845e307614ab8af238a33d3");

    const exclusiveRows = parseTable<{
      Path: string;
      "Unit coverage": string;
    }>(section(plan, "## Exclusive Files"), ["Path", "Unit coverage"]);
    expect(new Map(exclusiveRows.map((row) => [row.Path, row["Unit coverage"]]))).toEqual(new Map([
      ["apps/osl-hub/src/scrub_imap.rs", "f4, f5, f7, f12, f131"],
      ["crates/store/src/anchor.rs", "a48"],
      ["crates/ipc/src/secure_local_store.rs", "a140"],
      ["docs/plans/scrub-tree-merge-plan.md", "f10"],
      ["docs/design/build-order.md", "f83"],
      ["apps/osl-hub/src/cloud_autoscrub_envelope.rs", "f149"],
    ]));
  });

  it("Update routing decisions from real Codex capacity instead of stale pools.", () => {
    const plan = readDoc("docs/plans/osl-parallel-build-plan-2026-07-29.md");
    const exercise = fencedJsonBlocks(plan).find(isCodexCapacityExercise);
    expect(exercise).toBeDefined();

    const cases = exercise!.cases;
    expect(cases.map((entry) => entry.expectedDecision).sort()).toEqual([
      "dispatch",
      "refuse",
      "refuse",
      "refuse",
      "standby",
    ]);

    for (const exerciseCase of cases) {
      expect(routeCodexCapacity(exerciseCase), exerciseCase.name).toEqual({
        decision: exerciseCase.expectedDecision,
        reason: exerciseCase.expectedReason,
      });
    }

    const dispatchCase = cases.find((entry) => entry.expectedDecision === "dispatch");
    expect(dispatchCase).toBeDefined();
    expect(routeCodexCapacity({
      ...dispatchCase!,
      currentCapacity: {
        ...dispatchCase!.currentCapacity,
        fresh: false,
      },
    }).decision).toBe("standby");
    expect(routeCodexCapacity({
      ...dispatchCase!,
      forbiddenSubstitute: "borrowed-account",
    }).decision).toBe("refuse");
    expect(routeCodexCapacity({
      ...dispatchCase!,
      currentCapacity: {
        ...dispatchCase!.currentCapacity,
        ownedFileBound: false,
      },
    }).decision).toBe("standby");
  });

  it("docs/plans/osl-parallel-build-plan-2026-07-29.md", () => {
    const plan = readDoc("docs/plans/osl-parallel-build-plan-2026-07-29.md");
    const exercise = fencedJsonBlocks(plan).find(isCodexCapacityExercise);
    expect(exercise).toBeDefined();

    const dispatchCase = exercise!.cases.find((entry) => entry.expectedDecision === "dispatch");
    expect(dispatchCase).toBeDefined();
    expect(routeCodexCapacity(dispatchCase!)).toEqual({
      decision: "dispatch",
      reason: "fresh-capacity-and-owned-files",
    });

    expect(routeCodexCapacity({
      ...dispatchCase!,
      historicalPoolLabel: "available",
      currentCapacity: {
        ...dispatchCase!.currentCapacity,
        fresh: false,
      },
    })).toEqual({
      decision: "standby",
      reason: "missing-current-capacity",
    });
    expect(routeCodexCapacity({
      ...dispatchCase!,
      forbiddenSubstitute: "borrowed-account",
    })).toEqual({
      decision: "refuse",
      reason: "forbidden-substitute",
    });
    expect(routeCodexCapacity({
      ...dispatchCase!,
      currentCapacity: {
        ...dispatchCase!.currentCapacity,
        ownedFileBound: false,
      },
    })).toEqual({
      decision: "standby",
      reason: "missing-current-capacity",
    });
  });

  it("frontend_dist_is_embedded_after_frontend_build", () => {
    const output = execFileSync("bash", [
      "scripts/qa/osl-instance-b-build-wsl.sh",
      "--self-test",
    ], {
      cwd: new URL("../../../", import.meta.url),
      encoding: "utf8",
      env: {
        ...process.env,
        BUNDLE_A: "org.oslprivacy.hub",
      },
      stdio: ["ignore", "pipe", "pipe"],
    });

    expect(output).toBe("");
  }, 60_000);

  it("docs/design/osl-subjective-design-feel.md", () => {
    const primarySurface = reviewComplexityHidingSurface({
      kind: "surface",
      visibleChoices: oslPrimaryDestinations.map((destination) => destination.label),
      mainVisibleText: oslPrimaryDestinations
        .flatMap((destination) => [
          destination.userQuestion,
          destination.mainContent,
          destination.primaryAction,
        ])
        .join("\n"),
    });
    expect(primarySurface).toEqual({ pass: true, reasons: [] });

    const plainRefusal = reviewComplexityHidingSurface({
      kind: "refusal",
      mainVisibleText: "Protected send is not ready for this conversation.",
      consequence: "The message would leave OSL protection.",
      safeAction: "Verify the person first or send normally.",
    });
    expect(plainRefusal).toEqual({ pass: true, reasons: [] });

    const supportExport = reviewComplexityHidingSurface({
      kind: "advanced-export",
      mainVisibleText: "Protection state is unknown for this conversation.",
      machineFields: ["ratchet epoch", "keyserver binding", "provider adapter id"],
    });
    expect(supportExport).toEqual({ pass: true, reasons: [] });

    expect(reviewComplexityHidingSurface({
      kind: "surface",
      visibleChoices: ["Ratchet state", "Keyserver binding", "Provider adapter"],
      mainVisibleText: "Choose the transport plumbing for protection before sending.",
    })).toEqual({
      pass: false,
      reasons: ["visible surface exposes implementation machinery"],
    });

    expect(reviewComplexityHidingSurface({
      kind: "refusal",
      mainVisibleText: "Ratchet header is missing.",
      consequence: "Protocol state is invalid.",
      safeAction: "Open the keyserver diagnostics.",
    })).toEqual({
      pass: false,
      reasons: [
        "visible surface exposes implementation machinery",
        "visible surface does not use the product model",
        "refusal makes the mechanism the answer",
      ],
    });
  });

  it("Adopt shared memory cards across every active account.", () => {
    const prompts = readDoc("docs/design/osl-current-window-prompts-2026-07-26.md");
    expectSharedMemoryCardsAcrossActiveAccounts(prompts);

    const commonUpdate = fencedTextBlocks(section(prompts, "## One update prompt for every active OSL tab"));
    expect(commonUpdate).toHaveLength(1);
    expect(commonUpdate[0]!.toLowerCase()).toMatch(/if this ai\/account has never read it[\s\S]*compact memory card/u);
    expect(commonUpdate[0]!.toLowerCase()).toMatch(/if it already has[\s\S]*read only/u);
    expect(commonUpdate[0]!.toLowerCase()).toMatch(/never copy the giant spec or volatile status\s+into memory/u);

    const bootstrap = fencedTextBlocks(section(prompts, "## Reusable safe new-window bootstrap"));
    expect(bootstrap).toHaveLength(1);
    expect(bootstrap[0]!.toLowerCase()).toMatch(/load the compact\s+memory card/u);
    expect(bootstrap[0]!.toLowerCase()).toMatch(/before editing/u);
  });

  it("docs/design/osl-current-window-prompts-2026-07-26.md", () => {
    const prompts = readDoc("docs/design/osl-current-window-prompts-2026-07-26.md");
    expectSharedMemoryCardsAcrossActiveAccounts(prompts);
  });
});
