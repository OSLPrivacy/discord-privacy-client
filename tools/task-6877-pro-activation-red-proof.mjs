#!/usr/bin/env node

/**
 * TASK 6877 — prove TASK 6876's activation morph is not a second page, a
 * local toggle, an extra control, or an unmeasured visual substitution.
 *
 * The source lane may be checked out elsewhere, so every trial is an isolated
 * archive of its finished commit.  The trial directories are always removed;
 * the restored, unmodified archive is the final green run.
 */
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SOURCE_REVISION = "93a3687a9"; // TASK 6876's completed lane commit.
const UI = path.join("apps", "osl-hub-ui");
const UNIT = path.join("src", "pro-ready-controls.test.ts");
const VISUAL = path.join("screenshots", "check-task-6876-pro-activation.mjs");
const VITEST = path.join(REPO, UI, "node_modules", "vitest", "vitest.mjs");
const starveIndex = process.argv.indexOf("--starve");
const STARVE = starveIndex === -1 ? null : process.argv[starveIndex + 1] ?? null;
const STARVED_MUTANT = STARVE?.startsWith("mutant:") ? STARVE.slice("mutant:".length) : null;

function command(command, args, options = {}) {
  return spawnSync(command, args, { encoding: "utf8", ...options });
}

function checked(commandName, args, options = {}) {
  const result = command(commandName, args, options);
  assert.equal(result.status, 0, `${commandName} ${args.join(" ")} failed:\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}

function replaceOnce(file, from, to, label) {
  const source = readFileSync(file, "utf8");
  const count = source.split(from).length - 1;
  assert.equal(count, 1, `${label}: mutation anchor expected once, found ${count}`);
  writeFileSync(file, source.replace(from, to));
}

const MUTANTS = [
  {
    name: "route",
    broken: "route: activation navigated to a second page",
    apply(copy) {
      replaceOnce(
        path.join(copy, UI, "src", "main.ts"),
        'return discordQaShell && candidate === "pro" ? "sending" : candidate;',
        'return proActivationView === "activated" && candidate === "pro" ? "forward-secrecy" : (discordQaShell && candidate === "pro" ? "sending" : candidate);',
        this.name,
      );
    },
  },
  {
    name: "issuer",
    broken: "issuer: a local boolean activated Pro without an issuer decision",
    apply(copy) {
      replaceOnce(
        path.join(copy, UI, "src", "main.ts"),
        'if (licenseState.access === "pro" || licenseState.access === "offlineGrace") {',
        "if (true) {",
        this.name,
      );
    },
  },
  {
    name: "input-control",
    broken: "control: activation-code input remained beside the activated box",
    apply(copy) {
      replaceOnce(
        path.join(copy, UI, "src", "main.ts"),
        '<span>Code activated</span></div>',
        '<span>Code activated</span><input id="activation-code" value="OSL-ABCD-EFGH-IJKL-MNOP"/></div>',
        this.name,
      );
    },
  },
  {
    name: "centering-measurement",
    broken: "measurement: activated centred group shifted by 4 px",
    apply(copy) {
      const stylesheet = path.join(copy, UI, "src", "styles.css");
      const source = readFileSync(stylesheet, "utf8");
      assert.ok(source.includes(".pro-code-screen {"), `${this.name}: pro screen CSS is absent`);
      writeFileSync(stylesheet, `${source}\n.pro-code-screen[data-pro-activation-state="activated"] { transform: translateY(-36px); }\n`);
    },
  },
  {
    name: "empty-control",
    broken: "control: empty activation input enabled Activate Pro",
    apply(copy) {
      replaceOnce(
        path.join(copy, UI, "src", "main.ts"),
        'type="submit" disabled aria-disabled="true"><span class="signin-unlock-label">${checking ? "Checking…" : "Activate Pro"}',
        'type="submit" aria-disabled="false"><span class="signin-unlock-label">${checking ? "Checking…" : "Activate Pro"}',
        this.name,
      );
    },
  },
  {
    name: "entitlement-text",
    broken: "control: activated box exposed entitlement id text",
    apply(copy) {
      replaceOnce(
        path.join(copy, UI, "src", "main.ts"),
        '<span>Code activated</span></div>',
        '<span>Code activated</span><span>Entitlement ID: ent-6877</span></div>',
        this.name,
      );
    },
  },
];

const REQUIRED_MUTANTS = MUTANTS.map((mutant) => mutant.name);
const ACTIVE_MUTANTS = REQUIRED_MUTANTS.includes(STARVED_MUTANT ?? "")
  ? MUTANTS.filter((mutant) => mutant.name !== STARVED_MUTANT)
  : MUTANTS;

function absent(label, condition) {
  assert.ok(condition, `absent ${label}`);
}

function validateHarness() {
  assert.equal(ACTIVE_MUTANTS.length, 6, `absent mutant: ${STARVED_MUTANT ?? "expected all six required mutations"}`);
  assert.equal(new Set(ACTIVE_MUTANTS.map((mutant) => mutant.name)).size, 6, "absent mutant: duplicate mutant name");
  for (const mutant of ACTIVE_MUTANTS) assert.equal(typeof mutant.apply, "function", `absent mutant: ${mutant.name}`);
  absent("prerequisite: local 6876 test runner", existsSync(VITEST));
  const unitSource = execFileSync("git", ["show", `${SOURCE_REVISION}:${UI}/${UNIT}`], { cwd: REPO, encoding: "utf8" });
  absent("route inventory", STARVE !== "route-inventory" && /routeInventory before=1 during=1 after=1 secondProRoutes=0/.test(unitSource));
  absent("issuer", STARVE !== "issuer" && /validate_hub_activation_code/.test(unitSource) && /issuerDecisionRequired=1/.test(unitSource));
  absent("fresh profile", STARVE !== "fresh-profile" && /__oslHubUiTest\.reset/.test(unitSource));
}

function makeFreshCopy(name) {
  const copy = mkdtempSync(path.join(os.tmpdir(), `osl-6877-${name}-`));
  const archive = execFileSync("git", ["archive", "--format=tar", SOURCE_REVISION], { cwd: REPO, encoding: "buffer", maxBuffer: 128 * 1024 * 1024 });
  const unpack = spawnSync("tar", ["-xf", "-", "-C", copy], { input: archive, encoding: "utf8" });
  assert.equal(unpack.status, 0, `fresh profile ${name}: cannot unpack archive: ${unpack.stderr}`);
  const copyModules = path.join(copy, UI, "node_modules");
  symlinkSync(path.join(REPO, UI, "node_modules"), copyModules, "dir");
  const profile = path.join(copy, ".task-6877-fresh-profile");
  mkdirSync(profile);
  writeFileSync(path.join(profile, "fresh"), `${name}\n`);
  return { copy, profile };
}

function run6876(copy, profile) {
  const cwd = path.join(copy, UI);
  const environment = {
    ...process.env,
    // A disposable XDG profile keeps the visual browser test away from any
    // operator profile while leaving the machine's live network untouched.
    XDG_CONFIG_HOME: path.join(profile, "config"),
    XDG_CACHE_HOME: path.join(profile, "cache"),
  };
  const unit = command("node", [VITEST, "run", UNIT, "--poolOptions.threads.singleThread=true"], { cwd, env: environment });
  if (unit.status !== 0) return { status: unit.status ?? 1, output: `${unit.stdout}${unit.stderr}`, stage: "unit" };
  const visual = command("node", [VISUAL], { cwd, env: environment });
  return { status: visual.status ?? 1, output: `${unit.stdout}${unit.stderr}${visual.stdout}${visual.stderr}`, stage: "visual" };
}

function mustBeRed(mutant) {
  const { copy, profile } = makeFreshCopy(mutant.name);
  try {
    mutant.apply(copy);
    const result = run6876(copy, profile);
    assert.notEqual(result.status, 0, `mutant ${mutant.name} survived 6876 at ${result.stage}\n${result.output}`);
    return { mutant: mutant.name, status: result.status, stage: result.stage, broken: mutant.broken };
  } finally {
    rmSync(copy, { recursive: true, force: true });
    assert.ok(!existsSync(copy), `restoration absent: mutant copy ${mutant.name} was not discarded`);
  }
}

function restoredGreen() {
  const { copy, profile } = makeFreshCopy("restored");
  try {
    const result = run6876(copy, profile);
    assert.equal(result.status, 0, `restoration absent: restored 6876 failed at ${result.stage}\n${result.output}`);
    assert.match(result.output, /routeInventory before=1 during=1 after=1 secondProRoutes=0/, "restoration absent: route inventory result missing");
    assert.match(result.output, /issuerDecisionRequired=1/, "restoration absent: issuer result missing");
    assert.match(result.output, /state=checking→activated input=1 result=1/, "restoration absent: one real activation result missing");
    return { status: result.status, oneRoute: 1, realActivation: 1, output: result.output };
  } finally {
    rmSync(copy, { recursive: true, force: true });
    assert.ok(!existsSync(copy), "restoration absent: restored copy was not discarded");
  }
}

function starvationProof() {
  const cases = [...REQUIRED_MUTANTS.map((name) => `mutant:${name}`), "route-inventory", "issuer", "fresh-profile", "restoration"];
  return cases.map((caseName) => {
    const result = command("node", [fileURLToPath(import.meta.url), "--starve", caseName], { cwd: REPO });
    const output = `${result.stdout}${result.stderr}`;
    assert.notEqual(result.status, 0, `absent starvation rejection: ${caseName}`);
    const expected = caseName.startsWith("mutant:") ? `absent mutant: ${caseName.slice("mutant:".length)}` : `absent ${caseName.replace(/-/g, " ")}`;
    assert.match(output, new RegExp(expected), `absent starvation name: ${caseName}`);
    return { starved: caseName, status: result.status };
  });
}

function main() {
  validateHarness();
  if (STARVE === "restoration") assert.fail("absent restoration");
  const red = ACTIVE_MUTANTS.map(mustBeRed);
  const restored = restoredGreen();
  const starvation = STARVE ? [] : starvationProof();
  console.log(JSON.stringify({ task: 6877, sourceRevision: SOURCE_REVISION, red, restored: { status: restored.status, oneRoute: restored.oneRoute, realActivation: restored.realActivation }, starvation, discardedCopies: 7 }, null, 2));
}

main();
