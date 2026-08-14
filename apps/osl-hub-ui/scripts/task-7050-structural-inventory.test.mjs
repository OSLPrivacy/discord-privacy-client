import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { auditStructuralInventories, compareRouteInventory } from "./task-7050-structural-inventory.mjs";

const create = { name: "create", label: "Create account", destination: "onboarding/create" };
const restore = { name: "restore", label: "Use recovery phrase", destination: "onboarding/import" };

test("TASK 7050 names both build routes when one design page carries create and restore", () => {
  const result = auditStructuralInventories({
    routes: ["onboarding/create", "onboarding/import"],
    manifest: [{ kind: "routed", page: "Create Account.dc.html", shippingRoutes: ["onboarding/create", "onboarding/import"] }],
    designInventories: { "Create Account.dc.html": [create, restore] },
    buildInventories: { "onboarding/create": [create], "onboarding/import": [restore] },
  });
  assert.equal(result.ok, false);
  assert.deepEqual(result.findings, [
    'design page "Create Account.dc.html" has one page offering "Create account" and "Use recovery phrase"; build routes "onboarding/create" and "onboarding/import" reach them as 2 routes.',
  ]);
});

test("TASK 7050 exits one for the split page report and zero only for equal inventories", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7050-"));
  try {
    const script = new URL("./task-7050-structural-inventory.mjs", import.meta.url).pathname;
    const split = path.join(directory, "split.json");
    await writeFile(split, JSON.stringify({
      routes: ["onboarding/create", "onboarding/import"],
      manifest: [{ kind: "routed", page: "Create Account.dc.html", shippingRoutes: ["onboarding/create", "onboarding/import"] }],
      designInventories: { "Create Account.dc.html": [create, restore] },
      buildInventories: { "onboarding/create": [create], "onboarding/import": [restore] },
    }));
    const red = spawnSync(process.execPath, [script, "--fixture", split], { encoding: "utf8" });
    assert.equal(red.status, 1);
    assert.match(red.stderr, /design page "Create Account\.dc\.html" has one page offering "Create account" and "Use recovery phrase"; build routes "onboarding\/create" and "onboarding\/import" reach them as 2 routes\./u);

    const green = path.join(directory, "green.json");
    await writeFile(green, JSON.stringify({
      routes: ["onboarding/welcome"],
      manifest: [{ kind: "routed", page: "Create Account.dc.html", route: "onboarding/welcome" }],
      designInventories: { "Create Account.dc.html": [create, restore] },
      buildInventories: { "onboarding/welcome": [create, restore] },
    }));
    const pass = spawnSync(process.execPath, [script, "--fixture", green], { encoding: "utf8" });
    assert.equal(pass.status, 0);
    assert.match(pass.stdout, /TASK7050 STRUCTURAL PASS controls agree control-for-control/u);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("TASK 7050 reports every named single-route structural difference", () => {
  const page = "Example.dc.html";
  const route = "onboarding/example";
  const base = [
    { name: "first", label: "First", destination: "one" },
    { name: "second", label: "Second", destination: "two" },
  ];
  assert.match(compareRouteInventory({ page, route, designInventory: base, buildInventory: [base[0]] }).findings.join("\n"), /design has control "Second" and build route "onboarding\/example" lacks it/u);
  assert.match(compareRouteInventory({ page, route, designInventory: [base[0]], buildInventory: [...base, { name: "third", label: "Third", destination: "three" }] }).findings.join("\n"), /build route "onboarding\/example" has control "Third" and design lacks it/u);
  assert.match(compareRouteInventory({ page, route, designInventory: base, buildInventory: [base[1], base[0]] }).findings.join("\n"), /controls "First" and "Second" are in a different order/u);
  assert.match(compareRouteInventory({ page, route, designInventory: [base[0]], buildInventory: [{ ...base[0], label: "Changed" }] }).findings.join("\n"), /control "first" has a different label/u);
  assert.match(compareRouteInventory({ page, route, designInventory: [base[0]], buildInventory: [{ ...base[0], destination: "elsewhere" }] }).findings.join("\n"), /control "first" reaches a different destination/u);
});

test("TASK 7050 starves neither inventory nor manifest pairing silently", () => {
  const result = auditStructuralInventories({
    routes: ["onboarding/create"],
    manifest: [],
    designInventories: {},
    buildInventories: {},
  });
  assert.equal(result.ok, false);
  assert.match(result.findings.join("\n"), /build route "onboarding\/create" has no design-page pairing/u);
});
