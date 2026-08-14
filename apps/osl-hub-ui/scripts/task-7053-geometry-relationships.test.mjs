import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { PINNED_CANVAS, auditScreenRelationships } from "./task-7053-geometry-relationships.mjs";

const create = { name: "create", label: "Create account", destination: "onboarding/create" };
const restore = { name: "restore", label: "Use recovery phrase", destination: "onboarding/import" };
const page = "Create Account.dc.html";
const route = "onboarding/create";

function screen(controls) {
  return { canvas: PINNED_CANVAS, controls };
}

function fixture(buildControls, designControls = [
  { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
  { ...restore, bounds: { left: 340, top: 100, width: 200, height: 40 } },
]) {
  return {
    routes: [route],
    manifest: [{ kind: "routed", page, route }],
    designScreens: { [page]: screen(designControls) },
    buildScreens: { [route]: screen(buildControls) },
  };
}

test("TASK 7053 makes stacking a matching pair one advisory and exits zero", async () => {
  const input = fixture([
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 100, top: 180, width: 200, height: 40 } },
  ]);
  const result = auditScreenRelationships(input);
  assert.equal(result.ok, true);
  assert.equal(result.structuralFindings.length, 0);
  assert.equal(result.geometryFindings.length, 1);
  assert.match(result.geometryFindings[0], /share a row in design but are stacked/u);

  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7053-"));
  try {
    const file = path.join(directory, "stacked.json");
    await writeFile(file, JSON.stringify(input));
    const script = new URL("./task-7053-geometry-relationships.mjs", import.meta.url).pathname;
    const child = spawnSync(process.execPath, [script, "--fixture", file], { encoding: "utf8" });
    assert.equal(child.status, 0);
    assert.match(child.stderr, /TASK7053 GEOMETRY ADVISORY: .*share a row/u);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7053 leaves structural failure above advisory geometry and exits one", () => {
  const details = { name: "details", label: "More details", destination: "onboarding/details", bounds: { left: 100, top: 260, width: 200, height: 40 } };
  const input = fixture([
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 100, top: 180, width: 200, height: 40 } },
  ], [
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 340, top: 100, width: 200, height: 40 } },
    details,
  ]);
  const result = auditScreenRelationships(input);
  assert.equal(result.ok, false);
  assert.equal(result.structuralFindings.length, 1);
  assert.equal(result.geometryFindings.length, 1);
  assert.match(result.structuralFindings[0], /lacks it/u);
});

test("TASK 7053 prints structural failure before the lower-priority geometry section", async () => {
  const details = { name: "details", label: "More details", destination: "onboarding/details", bounds: { left: 100, top: 260, width: 200, height: 40 } };
  const input = fixture([
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 100, top: 180, width: 200, height: 40 } },
  ], [
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 340, top: 100, width: 200, height: 40 } },
    details,
  ]);
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7053-order-"));
  try {
    const file = path.join(directory, "structural-and-geometry.json");
    await writeFile(file, JSON.stringify(input));
    const script = new URL("./task-7053-geometry-relationships.mjs", import.meta.url).pathname;
    const child = spawnSync(process.execPath, [script, "--fixture", file], { encoding: "utf8" });
    assert.equal(child.status, 1);
    const structural = child.stderr.indexOf("TASK7053 STRUCTURAL DIFFERENCE:");
    const geometrySection = child.stderr.indexOf("TASK7053 GEOMETRY ADVISORIES:");
    assert.ok(structural >= 0 && geometrySection > structural, child.stderr);
    assert.match(child.stderr, /TASK7053 GEOMETRY ADVISORY: .*share a row/u);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7053 ignores a four-pixel whole-screen shift", () => {
  const input = fixture([
    { ...create, bounds: { left: 104, top: 104, width: 200, height: 40 } },
    { ...restore, bounds: { left: 344, top: 104, width: 200, height: 40 } },
  ]);
  const result = auditScreenRelationships(input);
  assert.equal(result.ok, true);
  assert.deepEqual(result.geometryFindings, []);
  assert.deepEqual(result.geometryUnavailable, []);
});

test("TASK 7053 fails closed when a pinned capture is unavailable", () => {
  const input = fixture([{ ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } }]);
  delete input.buildScreens[route].canvas;
  const result = auditScreenRelationships(input);
  assert.equal(result.ok, false);
  assert.match(result.geometryUnavailable.join("\n"), /GEOMETRY.*not pinned to 1280x800/u);
});

test("TASK 7053 compares reading order, relative width, and fold relationships", () => {
  const input = fixture([
    { ...create, bounds: { left: 100, top: 840, width: 100, height: 40 } },
    { ...restore, bounds: { left: 100, top: 100, width: 300, height: 40 } },
  ], [
    { ...create, bounds: { left: 100, top: 100, width: 200, height: 40 } },
    { ...restore, bounds: { left: 100, top: 180, width: 100, height: 40 } },
  ]);
  const result = auditScreenRelationships(input);
  assert.equal(result.ok, true);
  assert.equal(result.geometryFindings.length, 3);
  assert.match(result.geometryFindings.join("\n"), /fold/u);
  assert.match(result.geometryFindings.join("\n"), /top-to-bottom reading order/u);
  assert.match(result.geometryFindings.join("\n"), /relative-width relationship/u);
});
