/** TASK 7053b — mutation proof for the 7053 structural-first geometry gate. */
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const sourcePath = path.join(here, "task-7053-geometry-relationships.mjs");
const structuralPath = path.join(here, "task-7050-structural-inventory.mjs");
const manifestPath = path.join(here, "task-6890-shipping-manifest.mjs");
const routePath = path.join(here, "task-7049-route-design-manifest.mjs");
const stackedFixture = path.join(here, "fixtures/task-7053-stacked.json");
const missingFixture = path.join(here, "fixtures/task-7053-stacked-missing.json");
const shiftedFixture = path.join(here, "fixtures/task-7053-shifted.json");

function run(script, fixture) {
  return spawnSync(process.execPath, [script, "--fixture", fixture], { encoding: "utf8" });
}

function replaceOnce(source, before, after, name) {
  assert.ok(source.includes(before), `TASK7053B ${name}: mutation anchor is absent`);
  return source.replace(before, after);
}

async function main() {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7053b-"));
  try {
    const source = await readFile(sourcePath, "utf8");
    await writeFile(path.join(directory, "task-7050-structural-inventory.mjs"), await readFile(structuralPath, "utf8"));
    await writeFile(path.join(directory, "task-6890-shipping-manifest.mjs"), await readFile(manifestPath, "utf8"));
    await writeFile(path.join(directory, "task-7049-route-design-manifest.mjs"), await readFile(routePath, "utf8"));
    for (const fixture of [stackedFixture, missingFixture, shiftedFixture]) {
      await writeFile(path.join(directory, path.basename(fixture)), await readFile(fixture, "utf8"));
    }
    const clean = path.join(directory, "task-7053-geometry-relationships.mjs");
    await writeFile(clean, source);

    const shifted = run(clean, path.join(directory, path.basename(shiftedFixture)));
    assert.equal(shifted.status, 0, shifted.stderr);
    assert.match(shifted.stderr, /GEOMETRY PASS relationship findings=0/u);
    const stacked = run(clean, path.join(directory, path.basename(stackedFixture)));
    assert.equal(stacked.status, 0, stacked.stderr);
    assert.equal((stacked.stderr.match(/TASK7053 GEOMETRY ADVISORY:/gu) ?? []).length, 1, stacked.stderr);
    const missing = run(clean, path.join(directory, path.basename(missingFixture)));
    assert.equal(missing.status, 1, missing.stderr);
    assert.ok(missing.stderr.indexOf("TASK7053 STRUCTURAL DIFFERENCE:") < missing.stderr.indexOf("TASK7053 GEOMETRY ADVISORIES:"), missing.stderr);

    const ordering = path.join(directory, "ordering.mjs");
    await writeFile(ordering, replaceOnce(source,
      'console.error("TASK7053 STRUCTURAL FINDINGS:");',
      'console.error("TASK7053 GEOMETRY ADVISORIES:");\n  console.error("TASK7053 STRUCTURAL FINDINGS:");',
      "ordering"));
    const orderingResult = run(ordering, path.join(directory, path.basename(missingFixture)));
    assert.equal(orderingResult.status, 1, orderingResult.stderr);
    assert.ok(orderingResult.stderr.indexOf("TASK7053 GEOMETRY ADVISORIES:") < orderingResult.stderr.indexOf("TASK7053 STRUCTURAL DIFFERENCE:"), orderingResult.stderr);

    const promoted = path.join(directory, "promoted.mjs");
    await writeFile(promoted, replaceOnce(source,
      "if (!result.ok) process.exitCode = 1;",
      "if (!result.ok || result.geometryFindings.length) process.exitCode = 1;",
      "advisory promotion"));
    const promotedResult = run(promoted, path.join(directory, path.basename(stackedFixture)));
    assert.equal(promotedResult.status, 1, promotedResult.stderr);
    assert.match(promotedResult.stderr, /TASK7053 GEOMETRY ADVISORY:/u);

    const pixels = path.join(directory, "absolute-pixels.mjs");
    const pixelMutant = replaceOnce(source,
      "const common = [...design.controls.keys()].filter((name) => build.controls.has(name));",
      "const common = [...design.controls.keys()].filter((name) => build.controls.has(name));\n  // MUTANT: an exact position check is deliberately prohibited.\n  if (common.some((name) => design.controls.get(name).rect.left !== build.controls.get(name).rect.left)) findings.push(\"absolute pixel position changed\");",
      "absolute-pixel comparison");
    await writeFile(pixels, pixelMutant);
    const pixelResult = run(pixels, path.join(directory, path.basename(shiftedFixture)));
    assert.equal(pixelResult.status, 0, pixelResult.stderr);
    assert.match(pixelResult.stderr, /absolute pixel position changed/u);
    assert.match(await readFile(pixels, "utf8"), /rect\.left !== build\.controls\.get\(name\)\.rect\.left/u);

    console.log("TASK7053B PASS shifted_findings=0 stacked_advisories=1 stacked_exit=0 missing_exit=1 structural_first=true ordering_red=1 section=GEOMETRY promotion_red=1 section=GEOMETRY absolute_pixels_red=1 section=GEOMETRY");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

main().catch((error) => { console.error(`TASK7053B FAIL ${error.message}`); process.exitCode = 1; });
