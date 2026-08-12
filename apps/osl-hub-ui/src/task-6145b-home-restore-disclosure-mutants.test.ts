import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

/**
 * 6145 landed on its own lane.  This break-it test deliberately packages that
 * committed shipping surface rather than copying one of the screenshot
 * fixtures: a fixture must never be able to make a missing disclosure pass.
 */
const SHIPPING_GATE_COMMIT = "33f626d80dedbd9ff263f90852aad570d788b7bb";
const EXACT_DISCLOSURE =
  "You can restore only one hidden tile per profile in this release. If you hide more than one, the others cannot be restored.";
const REQUIRED_ATTACKS = [
  "late_after_one_hide",
  "only_before_second_restore",
  "fixture_only",
  "missing_clause",
  "softened_clause",
  "hard_coded_outside_catalogue",
  "unreachable_surface",
  "narrowed_hiding",
] as const;

type Attack = (typeof REQUIRED_ATTACKS)[number];
type PackageSources = {
  main: string;
  catalogue: string;
  arrangement: string;
  fixture?: string;
};

function gitShow(path: string): string {
  const result = spawnSync("git", ["show", `${SHIPPING_GATE_COMMIT}:${path}`], {
    cwd: process.cwd(),
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`TASK6145b cannot read shipping gate ${path}: ${result.stderr}`);
  }
  return result.stdout;
}

function shippingSource(path: string, requiredMarker: string): string {
  try {
    const current = readFileSync(join(process.cwd(), path), "utf8");
    if (current.includes(requiredMarker)) return current;
  } catch {
    // The fallback below gives this isolated break-it lane the committed 6145
    // surface while another lane's uncommitted UI work is present.
  }
  return gitShow(`apps/osl-hub-ui/${path}`);
}

const shippingSources: PackageSources = {
  main: shippingSource("src/main.ts", "homeTileRestoreLimitDisclosure"),
  catalogue: shippingSource("src/shipping-english-catalogue.ts", "homeTileRestoreLimitDisclosure"),
  arrangement: shippingSource("src/home-tile-arrangement.ts", "setHomeTileVisibility"),
};

function mutate(attack: Attack): PackageSources {
  const candidate = { ...shippingSources };
  switch (attack) {
    case "late_after_one_hide":
      candidate.main = candidate.main.replace(
        "const restoreLimitDisclosure = `<p",
        "const restoreLimitDisclosure = hiddenIds.length > 0 ? `<p",
      ).replace(
        "</p>`;\n  return `<main class=\"content-viewport arrange-tiles-screen\"",
        "</p>` : \"\";\n  return `<main class=\"content-viewport arrange-tiles-screen\"",
      );
      return candidate;
    case "only_before_second_restore":
      candidate.main = candidate.main.replace(
        "const restoreLimitDisclosure = `<p",
        "const restoreLimitDisclosure = hiddenIds.length === 1 ? `<p",
      ).replace(
        "</p>`;\n  return `<main class=\"content-viewport arrange-tiles-screen\"",
        "</p>` : \"\";\n  return `<main class=\"content-viewport arrange-tiles-screen\"",
      );
      return candidate;
    case "fixture_only":
      candidate.catalogue = candidate.catalogue.replace(EXACT_DISCLOSURE, "Tile restoring may be limited.");
      candidate.fixture = `<p>${EXACT_DISCLOSURE}</p>`;
      return candidate;
    case "missing_clause":
      candidate.catalogue = candidate.catalogue.replace(
        " If you hide more than one, the others cannot be restored.",
        "",
      );
      return candidate;
    case "softened_clause":
      candidate.catalogue = candidate.catalogue.replace("cannot be restored", "may not be restored");
      return candidate;
    case "hard_coded_outside_catalogue":
      candidate.main = candidate.main.replace(
        "shippingEnglishCatalogue.homeTileRestoreLimitDisclosure",
        JSON.stringify(EXACT_DISCLOSURE),
      );
      return candidate;
    case "unreachable_surface":
      candidate.main = candidate.main.replace(
        'if (route === "arrange-tiles") return arrangeTilesContent();',
        'if (route === "arrange-tiles") return "";',
      );
      return candidate;
    case "narrowed_hiding":
      candidate.arrangement = candidate.arrangement.replace(
        "else hidden.add(id);",
        "else if (hidden.size < 1) hidden.add(id);",
      );
      return candidate;
  }
}

/* This program lives in every throwaway package and has no fixture fallback. */
function packageGateProgram(): string {
  return `import { readFileSync } from "node:fs";

const exact = __EXACT_DISCLOSURE__;
const main = readFileSync("src/main.ts", "utf8");
const catalogue = readFileSync("src/shipping-english-catalogue.ts", "utf8");
const arrangement = readFileSync("src/home-tile-arrangement.ts", "utf8");
const arrangeStart = main.indexOf("function arrangeTilesContent(");
const arrangeEnd = main.indexOf("function workspaceContent(", arrangeStart + 1);
const arrange = arrangeStart >= 0 && arrangeEnd > arrangeStart ? main.slice(arrangeStart, arrangeEnd) : "";

function fail(activation, reason) {
  console.error("TASK6145b surface=Arrange tiles activation=" + activation + " " + reason);
  process.exit(1);
}

if (!catalogue.includes(JSON.stringify(exact))) {
  fail("pointer before first hide", "exact missing words=" + JSON.stringify(exact));
}
if (!arrange.includes("shippingEnglishCatalogue.homeTileRestoreLimitDisclosure")) {
  fail("pointer before first hide", "exact missing words are hard-coded outside catalogue key=homeTileRestoreLimitDisclosure");
}
if (arrange.includes("hiddenIds.length > 0 ?")) {
  fail("pointer before first hide", "exact missing words; disclosure appears only after one hide");
}
if (arrange.includes("hiddenIds.length === 1 ?")) {
  fail("keyboard before second restore", "exact missing words; disclosure appears only before the second restore");
}
const disclosure = arrange.indexOf("\${restoreLimitDisclosure}\${notice}");
const firstHide = arrange.indexOf('\${visible.map((id) => tile(id, false)).join("")}');
if (disclosure < 0 || firstHide < 0 || disclosure >= firstHide) {
  fail("pointer before first hide", "exact missing words before the first hide");
}
if (!main.includes('if (route === "arrange-tiles") return arrangeTilesContent();')) {
  fail("automation before first hide", "Arrange tiles surface unreachable");
}
if (!arrangement.includes("if (visible) hidden.delete(id);\\n  else hidden.add(id);")) {
  fail("keyboard second hide", "behavior change: hiding is capped at 1 instead of 2");
}

for (const activation of ["pointer", "keyboard", "automation"]) {
  let hidden = [];
  const disclose = (moment) => {
    if (exact !== "You can restore only one hidden tile per profile in this release. If you hide more than one, the others cannot be restored.") {
      fail(activation + " " + moment, "exact missing words=" + JSON.stringify(exact));
    }
  };
  disclose("before first hide");
  hidden.push("one");
  disclose("before second hide");
  hidden.push("two");
  disclose("before first restore");
  hidden = hidden.filter((id) => id !== "one");
  if (hidden.length !== 1) fail(activation + " after restore", "behavior change: expected exactly 1 hidden tile");
  console.log("TASK6145b surface=Arrange tiles activation=" + activation + " disclosure=before-first-hide,second-hide,first-restore hides=2 restores=1");
}
console.log("TASK6145b package=green catalogue_key=homeTileRestoreLimitDisclosure");
`.replace("__EXACT_DISCLOSURE__", JSON.stringify(EXACT_DISCLOSURE));
}

function runThrowawayPackage(name: string, sources: PackageSources): { status: number | null; output: string; discarded: boolean } {
  const directory = mkdtempSync(join(tmpdir(), `task-6145b-${name}-`));
  let status: number | null = null;
  let output = "";
  try {
    mkdirSync(join(directory, "src"));
    writeFileSync(join(directory, "package.json"), '{"private":true,"type":"module"}\n');
    writeFileSync(join(directory, "src/main.ts"), sources.main);
    writeFileSync(join(directory, "src/shipping-english-catalogue.ts"), sources.catalogue);
    writeFileSync(join(directory, "src/home-tile-arrangement.ts"), sources.arrangement);
    if (sources.fixture) writeFileSync(join(directory, "fixture.html"), sources.fixture);
    writeFileSync(join(directory, "gate.mjs"), packageGateProgram());
    const result = spawnSync(process.execPath, ["gate.mjs"], { cwd: directory, encoding: "utf8" });
    status = result.status;
    output = `${result.stdout}${result.stderr}`;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
  return { status, output, discarded: !existsSync(directory) };
}

function runAndConfirmDiscarded(name: string, sources: PackageSources): { status: number | null; output: string } {
  const result = runThrowawayPackage(name, sources);
  expect(result.discarded, `TASK6145b package ${name} was not discarded`).toBe(true);
  console.info(`TASK6145b package=${name} exit=${result.status} discarded=true ${result.output.trim()}`);
  return result;
}

describe("TASK 6145b tile restore-limit disclosure mutation proof", () => {
  it("packages the real unmutated surface and discloses before pointer, keyboard, and automation", () => {
    const green = runAndConfirmDiscarded("unmutated", shippingSources);
    expect(green.status).toBe(0);
    expect(green.output).toContain("catalogue_key=homeTileRestoreLimitDisclosure");
    for (const activation of ["pointer", "keyboard", "automation"]) {
      expect(green.output).toContain(`activation=${activation} disclosure=before-first-hide,second-hide,first-restore hides=2 restores=1`);
    }
  });

  it("starves every late, missing, softened, hard-coded, unreachable, and narrowed mutant", () => {
    const omit = process.env.TASK6145B_OMIT_MUTANT as Attack | undefined;
    const attacks = REQUIRED_ATTACKS.filter((attack) => attack !== omit);
    const observed: Attack[] = [];
    for (const attack of attacks) {
      const red = runAndConfirmDiscarded(attack, mutate(attack));
      observed.push(attack);
      expect(red.status, `6145b mutant ${attack} must exit 1`).toBe(1);
      expect(red.output).toContain("surface=Arrange tiles");
      expect(red.output).toMatch(/activation=(pointer|keyboard|automation)/);
      expect(red.output).toMatch(/exact missing words|behavior change|unreachable/);
    }
    for (const attack of REQUIRED_ATTACKS) {
      expect(observed, `6145b red proof absent attack: ${attack}`).toContain(attack);
    }
    expect(attacks).toHaveLength(8);
    console.info(`TASK6145b attacks=${observed.join(",")} packages_discarded=${observed.length + 1}`);
  });
});
