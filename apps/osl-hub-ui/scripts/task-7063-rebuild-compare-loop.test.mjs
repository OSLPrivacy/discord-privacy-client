import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { MEASUREMENT_VERSION } from "./task-7062-pixel-difference.mjs";
import { renderAndMeasure } from "./task-7062-pixel-difference.mjs";
import { runRebuildCompareLoop } from "./task-7063-rebuild-compare-loop.mjs";

const control = { name: "continue", label: "Continue", destination: "onboarding/continue" };

function structural(overrides = {}) {
  return {
    routes: ["onboarding/continue"],
    manifest: [{ kind: "routed", page: "Welcome.dc.html", route: "onboarding/continue" }],
    designInventories: { "Welcome.dc.html": [control] },
    buildInventories: { "onboarding/continue": [control] },
    ...overrides,
  };
}

function measurement(percent, build = "ui-build") {
  return {
    measurement: MEASUREMENT_VERSION,
    percent_different: percent,
    differing_pixel_count: Math.round(percent * 10_240),
    compared_pixel_count: 1_024_000,
    build,
    design_page: "Welcome.dc.html",
    build_route: "onboarding/continue",
  };
}

function pass(build, changed, percent, comparison = structural()) {
  return { build, changed, structural: comparison, measure: measurement(percent, build) };
}

async function run(screen, passes, options = {}) {
  let cursor = 0;
  return runRebuildCompareLoop({
    screen,
    maxPasses: options.maxPasses ?? passes.length,
    rebuild: async () => passes[cursor++],
    measure: async (candidate) => candidate.measure,
  });
}

test("TASK 7063 rebuilds, structurally compares, measures through 7062, and converges below one percent", async () => {
  const result = await run("Welcome", [
    pass("welcome-build-1", "aligned the heading and primary button", 2.4),
    pass("welcome-build-2", "corrected card spacing and background colour", 0.4),
  ]);
  assert.equal(result.passed, true);
  assert.equal(result.passes.length, 2);
  assert.deepEqual(result.passes.map((entry) => entry.measurement.percent_different), [2.4, 0.4]);
  assert.deepEqual(result.passes.map((entry) => entry.build), ["welcome-build-1", "welcome-build-2"]);
  assert.ok(result.passes.every((entry) => entry.changed && entry.measurement.measurement === MEASUREMENT_VERSION));
  console.log(`TASK7063_CONVERGED screen=${result.screen} measurements=${result.passes.length} percentages=${result.passes.map((entry) => entry.measurement.percent_different).join(",")} last_build=${result.passes.at(-1).build}`);
});

test("TASK 7063 records two real TASK 7062 browser measurements that fall below one percent", async () => {
  const design = "<!doctype html><html><head><style>html,body{margin:0;width:1280px;height:800px;background:#17222d}.panel{width:500px;height:500px;background:#263746}.patch{width:50px;height:50px;background:#263746}</style></head><body><main class=\"panel\"><div class=\"patch\"></div></main></body></html>";
  const buildOne = design.replace(".panel{width:500px;height:500px;background:#263746}", ".panel{width:500px;height:500px;background:#ddeeff}");
  const buildTwo = design.replace(".patch{width:50px;height:50px;background:#263746}", ".patch{width:50px;height:50px;background:#ddeeff}");
  const server = createServer((request, response) => {
    response.setHeader("content-type", "text/html; charset=utf-8");
    response.end(request.url === "/design" ? design : request.url === "/build-one" ? buildOne : buildTwo);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const base = `http://127.0.0.1:${address.port}`;
  try {
    const candidates = [
      { build: "browser-build-1", changed: "replaced the large panel colour", structural: structural(), designUrl: `${base}/design`, buildUrl: `${base}/build-one`, designPage: "Welcome.dc.html", buildRoute: "onboarding/continue" },
      { build: "browser-build-2", changed: "limited the remaining colour correction to the small patch", structural: structural(), designUrl: `${base}/design`, buildUrl: `${base}/build-two`, designPage: "Welcome.dc.html", buildRoute: "onboarding/continue" },
    ];
    let cursor = 0;
    const result = await runRebuildCompareLoop({
      screen: "Browser welcome",
      maxPasses: 2,
      rebuild: async () => candidates[cursor++],
      measure: async (candidate) => renderAndMeasure({ ...candidate, buildId: candidate.build }),
    });
    assert.equal(result.passed, true);
    assert.equal(result.passes.length, 2);
    assert.ok(result.passes[0].measurement.percent_different > result.passes[1].measurement.percent_different);
    assert.ok(result.passes[1].measurement.percent_different < 1);
    assert.ok(result.passes.every((entry) => entry.measurement.measurement === MEASUREMENT_VERSION));
    console.log(`TASK7063_BROWSER_LOOP measurements=${result.passes.length} percentages=${result.passes.map((entry) => entry.measurement.percent_different).join(",")} builds=${result.passes.map((entry) => entry.build).join(",")}`);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("TASK 7063 gives structural differences priority even when pixels are under one percent", async () => {
  const split = structural({
    routes: ["onboarding/create", "onboarding/import"],
    manifest: [{ kind: "routed", page: "Create.dc.html", shippingRoutes: ["onboarding/create", "onboarding/import"] }],
    designInventories: { "Create.dc.html": [{ name: "create", label: "Create", destination: "onboarding/create" }, { name: "restore", label: "Restore", destination: "onboarding/import" }] },
    buildInventories: { "onboarding/create": [{ name: "create", label: "Create", destination: "onboarding/create" }], "onboarding/import": [{ name: "restore", label: "Restore", destination: "onboarding/import" }] },
  });
  const result = await run("Create account", [
    pass("create-build-1", "rechecked combined onboarding page", 0.8, split),
    pass("create-build-2", "adjusted visual card padding", 0.2, split),
  ]);
  assert.equal(result.passed, false);
  assert.equal(result.passes.at(-1).measurement.percent_different, 0.2);
  assert.match(result.remainingDifferences.join("\n"), /has one page offering/u);
  console.log(`TASK7063_STRUCTURAL_RED screen=${result.screen} last_percent=${result.passes.at(-1).measurement.percent_different} finding=${JSON.stringify(result.remainingDifferences[0])}`);
});

test("TASK 7063 stops above one percent when rebuilding no longer improves the screenshot", async () => {
  const result = await run("Settings", [
    pass("settings-build-1", "matched panel padding", 3.2),
    pass("settings-build-2", "corrected the largest colour mismatch", 3.2),
  ]);
  assert.equal(result.passed, false);
  assert.equal(result.stopReason, "stopped improving");
  assert.equal(result.passes.at(-1).measurement.percent_different, 3.2);
  assert.match(result.remainingDifferences.join("\n"), /3.2% is not under 1%/u);
  console.log(`TASK7063_STALLED screen=${result.screen} last_percent=${result.passes.at(-1).measurement.percent_different} reason=${result.stopReason}`);
});

test("TASK 7063 refuses a single reading, missing repair record, and non-7062 percentage", async () => {
  const one = await run("One reading", [pass("one-build", "initial layout implementation", 0.2)], { maxPasses: 2 });
  assert.equal(one.passed, false);
  assert.match(one.stopReason, /single measurement is not a loop/u);
  await assert.rejects(() => run("No change", [{ ...pass("empty-change", "", 2.2) }], { maxPasses: 2 }), /screen="No change".*what changed is missing/u);
  await assert.rejects(() => run("Fake percent", [{ ...pass("fake-build", "implemented spacing", 2.2), measure: { ...measurement(2.2), measurement: "invented" } }], { maxPasses: 2 }), /screen="Fake percent".*did not come from TASK 7062/u);
  await assert.rejects(() => run("Wrong build", [{ ...pass("expected-build", "implemented spacing", 2.2), measure: measurement(2.2, "other-build") }], { maxPasses: 2 }), /screen="Wrong build".*measured build "other-build" instead of rebuilt "expected-build"/u);
  console.log(`TASK7063_REFUSALS single_measurement=${one.passes.length} missing_change=red fake_percentage=red`);
});

test("TASK 7063 CLI exits one with last percentage and remaining difference when a real fixture stalls", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7063-"));
  try {
    const design = "<!doctype html><html><head><style>html,body{margin:0;width:1280px;height:800px;background:#17222d}.panel{width:500px;height:500px;background:#263746}</style></head><body><main class=\"panel\"></main></body></html>";
    const changed = design.replace("#263746", "#ddeeff");
    const designUrl = `data:text/html,${encodeURIComponent(design)}`;
    const buildUrl = `data:text/html,${encodeURIComponent(changed)}`;
    const fixture = path.join(directory, "stalled.json");
    await writeFile(fixture, JSON.stringify({
      screen: "CLI stalled",
      maxPasses: 2,
      passes: [
        { build: "cli-build-1", changed: "changed the panel colour", structural: structural(), designUrl, buildUrl, designPage: "Welcome.dc.html", buildRoute: "onboarding/continue" },
        { build: "cli-build-2", changed: "rechecked the same largest mismatch", structural: structural(), designUrl, buildUrl, designPage: "Welcome.dc.html", buildRoute: "onboarding/continue" },
      ],
    }));
    const script = new URL("./task-7063-rebuild-compare-loop.mjs", import.meta.url).pathname;
    const child = spawnSync(process.execPath, [script, "--fixture", fixture], { encoding: "utf8" });
    assert.equal(child.status, 1);
    assert.match(child.stderr, /TASK7063 NOT PASSED screen="CLI stalled" reason="stopped improving" last_percent=24\.414063/u);
    assert.match(child.stderr, /pixel difference 24\.414063% is not under 1%/u);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
