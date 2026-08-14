import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { MEASUREMENT_VERSION } from "./task-7062-pixel-difference.mjs";
import { gradeScreenTaskTick } from "./task-7054-screen-parity-tick-gate.mjs";
import { auditStructuralInventories } from "./task-7050-structural-inventory.mjs";

const script = new URL("./task-7054-screen-parity-tick-gate.mjs", import.meta.url).pathname;
const taskId = "task-7054-screen-parity";
const build = "ui-build-7054-exact";
const route = "settings/account";
const page = "Settings Account.dc.html";
const control = { name: "save", label: "Save", destination: "settings/account" };

function structural(overrides = {}) {
  return {
    designInventories: { [page]: [control] },
    buildInventories: { [route]: [control] },
    ...overrides,
  };
}

function loop(percent = 0.4, overrides = {}) {
  const measurement = {
    measurement: MEASUREMENT_VERSION,
    percent_different: percent,
    differing_pixel_count: Math.round(percent * 10_240),
    compared_pixel_count: 1_024_000,
    build,
    design_page: page,
    build_route: route,
  };
  return {
    passed: true,
    stopReason: "both bars passed",
    passes: [
      { build: "ui-build-7054-earlier", structural: { ok: true, findings: [] }, measurement: { ...measurement, build: "ui-build-7054-earlier", percent_different: 2.2 } },
      { build, structural: { ok: true, findings: [] }, measurement },
    ],
    ...overrides,
  };
}

function loopWithoutPercentage() {
  const result = loop();
  delete result.passes.at(-1).measurement.percent_different;
  return result;
}

function receipt(overrides = {}) {
  return {
    task: { id: taskId, kind: "screen", build, routes: [route] },
    manifest: [{ kind: "routed", page, route }],
    structural: structural(),
    recorded7063: { [route]: loop() },
    ownerReview: { verdict: "PASS" },
    ...overrides,
  };
}

async function grade(value, overrides = {}) {
  const stored = [];
  const refused = [];
  const result = await gradeScreenTaskTick({
    receipt: value,
    structural: auditStructuralInventories,
    read7063: async (name) => value.recorded7063?.[name],
    storeVerdict: async (verdict) => stored.push(verdict),
    refuse: async (failure) => refused.push(failure),
    ...overrides,
  });
  return { result, stored, refused };
}

test("TASK 7054 ticks only after every route passes fresh structure, 7063, and owner-last review", async () => {
  const { result, stored, refused } = await grade(receipt());
  assert.equal(result.ok, true, result.findings.join("\n"));
  assert.equal(refused.length, 0);
  assert.equal(stored.length, 1);
  assert.deepEqual(stored[0], result.verdict);
  assert.equal(stored[0].task_id, taskId);
  assert.equal(stored[0].build, build);
  assert.equal(stored[0].screens[0].design_page, page);
  assert.equal(stored[0].screens[0].percent_different, 0.4);
  assert.match(result.reports[0], new RegExp(`route=${JSON.stringify(route)} design_page=${JSON.stringify(page)} structural=PASS percent=0\\.4`, "u"));
  console.log(`TASK7054_GREEN task=${taskId} build=${build} route=${route} design_page=${page} percent=${stored[0].screens[0].percent_different}`);
});

test("TASK 7054 refuses a named structural difference even under one percent and ignores owner PASS", async () => {
  const altered = receipt({
    structural: structural({ buildInventories: { [route]: [] } }),
    ownerReview: { verdict: "PASS" },
  });
  const { result, stored, refused } = await grade(altered);
  assert.equal(result.ok, false);
  assert.equal(refused.length, 1);
  assert.equal(stored[0].verdict, "REFUSED");
  assert.match(result.findings.join("\n"), /design page "Settings Account\.dc\.html" structural difference:.*design has control "Save"/u);
  assert.match(result.findings.join("\n"), /owner PASS is recorded but cannot override/u);
  assert.equal(stored[0].screens[0].percent_different, 0.4);
  console.log(`TASK7054_STRUCTURAL_RED task=${taskId} design_page=${page} percent=0.4 owner_pass_ignored=1`);
});

test("TASK 7054 refuses at or above one percent, including an owner PASS", async () => {
  const altered = receipt({ recorded7063: { [route]: loop(1) } });
  const { result, stored } = await grade(altered);
  assert.equal(result.ok, false);
  assert.equal(stored[0].screens[0].percent_different, 1);
  assert.match(result.findings.join("\n"), /pixel difference 1% is at or above 1%/u);
  assert.match(result.findings.join("\n"), /owner PASS is recorded but cannot override/u);
  console.log(`TASK7054_PIXEL_RED task=${taskId} percentage=1 threshold=1 owner_pass_ignored=1`);
});

test("TASK 7054 refuses uncovered routes rather than skipping them", async () => {
  const missing = "settings/missing";
  const altered = receipt({ task: { id: taskId, kind: "screen", build, routes: [route, missing] } });
  const { result, stored } = await grade(altered);
  assert.equal(result.ok, false);
  assert.match(result.findings.join("\n"), new RegExp(`route ${JSON.stringify(missing)} has no design page`, "u"));
  assert.equal(stored[0].screens.length, 2);
  assert.equal(stored[0].screens[1].design_page, null);
  console.log(`TASK7054_UNCOVERED_RED task=${taskId} route=${missing} skipped=0`);
});

test("TASK 7054's CLI writes a task/build/percentage verdict and prints the per-screen report", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7054-"));
  try {
    const input = path.join(directory, "receipt.json");
    const output = path.join(directory, "verdict.json");
    await writeFile(input, JSON.stringify(receipt()));
    const child = spawnSync(process.execPath, [script, "--receipt", input, "--verdict", output], { encoding: "utf8" });
    assert.equal(child.status, 0, child.stderr);
    assert.match(child.stdout, /TASK7054 SCREEN task="task-7054-screen-parity" route="settings\/account" design_page="Settings Account\.dc\.html" structural=PASS percent=0\.4/u);
    assert.match(child.stdout, /TASK7054 TICKED task="task-7054-screen-parity" build="ui-build-7054-exact" screens=1/u);
    const verdict = JSON.parse(await readFile(output, "utf8"));
    assert.equal(verdict.task_id, taskId);
    assert.equal(verdict.build, build);
    assert.equal(verdict.screens[0].percent_different, 0.4);
    console.log(`TASK7054_CLI_GREEN task=${verdict.task_id} build=${verdict.build} percentage=${verdict.screens[0].percent_different}`);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7054's executable refuses structural, pixel, and override receipts with an on-disk verdict", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7054-red-"));
  try {
    const cases = [
      ["structural", receipt({ structural: structural({ buildInventories: { [route]: [] } }) }), /design page "Settings Account\.dc\.html" structural difference/u],
      ["pixel", receipt({ recorded7063: { [route]: loop(1.25) } }), /pixel difference 1\.25% is at or above 1%/u],
      ["override", receipt({ humanOverride: true }), /human override is forbidden/u],
    ];
    for (const [name, value, expected] of cases) {
      const input = path.join(directory, `${name}.json`);
      const output = path.join(directory, `${name}.verdict.json`);
      await writeFile(input, JSON.stringify(value));
      const child = spawnSync(process.execPath, [script, "--receipt", input, "--verdict", output], { encoding: "utf8" });
      assert.equal(child.status, 1, `${name}: ${child.stdout}${child.stderr}`);
      assert.match(child.stderr, expected, child.stderr);
      assert.match(child.stdout, /TASK7054 SCREEN .*design_page="Settings Account\.dc\.html"/u);
      const verdict = JSON.parse(await readFile(output, "utf8"));
      assert.equal(verdict.verdict, "REFUSED");
      assert.equal(verdict.task_id, taskId);
      assert.equal(verdict.build, build);
    }
    console.log(`TASK7054_CLI_RED structural_exit=1 pixel_exit=1 override_exit=1 task=${taskId}`);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7054 fails closed when a required gate component, comparison, verdict, or refusal is starved", async () => {
  const cases = [
    ["gate", receipt(), { structural: undefined }],
    ["manifest", receipt({ manifest: undefined }), {}],
    ["7063", receipt({ recorded7063: {} }), {}],
    ["store", receipt(), { storeVerdict: undefined }],
    ["refusal", receipt(), { refuse: undefined }],
    ["no-stored-verdict", receipt({ recorded7063: {} }), {}],
    ["no-build", receipt({ task: { id: taskId, kind: "screen", routes: [route] } }), {}],
    ["no-percentage", receipt({ recorded7063: { [route]: loopWithoutPercentage() } }), {}],
    ["7063-structure", receipt({ recorded7063: { [route]: loop(0.4, { passes: [
      { build: "ui-build-7054-earlier", structural: { ok: true, findings: [] }, measurement: { measurement: MEASUREMENT_VERSION, percent_different: 2, compared_pixel_count: 1_024_000, build: "ui-build-7054-earlier", design_page: page, build_route: route } },
      { build, structural: { ok: false, findings: ["named structural difference"] }, measurement: { measurement: MEASUREMENT_VERSION, percent_different: 0.4, compared_pixel_count: 1_024_000, build, design_page: page, build_route: route } },
    ] }) } }), {}],
  ];
  for (const [name, value, dependencies] of cases) {
    const { result } = await grade(value, dependencies);
    assert.equal(result.ok, false, `${name} starvation passed`);
    assert.match(result.findings.join("\n"), new RegExp(`task=${JSON.stringify(taskId)}`, "u"));
  }
  const human = await grade(receipt({ humanOverride: true }));
  assert.equal(human.result.ok, false);
  assert.match(human.result.findings.join("\n"), /human override is forbidden/u);
  console.log(`TASK7054_STARVATION_RED task=${taskId} gate=1 manifest=1 comparison=1 verdict=1 refusal=1 stored_verdict=1 build=1 percentage=1 recorded_structure=1 human_override=1`);
});
