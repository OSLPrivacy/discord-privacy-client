/** TASK 7062b — a tolerance band must turn the D27(a) check red. */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { PINNED_CANVAS } from "./task-7053-geometry-relationships.mjs";
import { NEUTRALISATION_MODE, measureCapturedPair } from "./task-7062-pixel-difference.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const source = path.join(here, "task-7062-pixel-difference.mjs");
const image = path.join(here, "../screenshots/no-recovery-secret-1280x800.png");

function capture(png) {
  return {
    png,
    viewport: { ...PINNED_CANVAS }, devicePixelRatio: 1,
    region: { x: 0, y: 0, ...PINNED_CANVAS }, cropping: false,
    scale: 1, downscaled: false, blurRadius: 0, tolerance: 1,
    recordedPixelCount: 1_024_000,
    neutralisation: { applied: true, mode: NEUTRALISATION_MODE, digest: "same" },
  };
}

async function main() {
  const directory = await mkdtemp(path.join(os.tmpdir(), "task-7062b-"));
  try {
    const png = await readFile(image);
    assert.throws(() => measureCapturedPair({ design: capture(png), build: capture(png), buildId: "green-control", designPage: "Page", buildRoute: "route" }), /tolerance band/u);
    const mutant = (await readFile(source, "utf8")).replace("if (capture.tolerance !== 0)", "if (false)");
    const mutantPath = path.join(here, `.task-7062b-mutant-${process.pid}.mjs`);
    const probePath = path.join(directory, "probe.mjs");
    try {
      await writeFile(mutantPath, mutant);
      await writeFile(probePath, `import assert from 'node:assert/strict'; import { readFile } from 'node:fs/promises'; import { measureCapturedPair, NEUTRALISATION_MODE } from ${JSON.stringify(mutantPath)}; const png = await readFile(${JSON.stringify(image)}); const capture = { png, viewport:{width:1280,height:800}, devicePixelRatio:1, region:{x:0,y:0,width:1280,height:800}, cropping:false, scale:1, downscaled:false, blurRadius:0, tolerance:1, recordedPixelCount:1024000, neutralisation:{applied:true,mode:NEUTRALISATION_MODE,digest:'same'} }; assert.throws(() => measureCapturedPair({design:capture,build:capture,buildId:'mutant',designPage:'Page',buildRoute:'route'}), /tolerance band/u);`);
      const red = spawnSync(process.execPath, [probePath], { encoding: "utf8" });
      assert.equal(red.status, 1, `mutated tolerance guard unexpectedly stayed green: ${red.stdout}${red.stderr}`);
      assert.match(`${red.stdout}${red.stderr}`, /Missing expected exception/u);
      console.log(`TASK7062B_MUTANT_TOLERANCE_EXIT=${red.status} named=tolerance-band guard=removed`);
      console.log("TASK7062B_RESTORED_TOLERANCE_EXIT=0 named=tolerance-band");
    } finally {
      await rm(mutantPath, { force: true });
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

main().catch((error) => { console.error(error.stack || error.message); process.exitCode = 1; });
