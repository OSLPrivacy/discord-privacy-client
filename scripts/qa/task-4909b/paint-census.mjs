/**
 * TASK 4909b - the paint-by-second-3 census.
 *
 * The 4909 harness asserts trial-by-trial and stops at the first bad trial, so
 * a broken build reports "trial 0" rather than a count. This census runs all
 * ten starts to completion with a genuine 3000ms wall-clock deadline each, so
 * the literal number in 4909b's finish line -- "N of 10 starts paint by second
 * 3" -- is measured, not inferred. It drives the SAME src/tor-boot-orchestrator.ts
 * (bundled with esbuild, not reimplemented) against the SAME real 45s sidecar
 * fixture process the 4909 harness uses.
 */
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { performance } from "node:perf_hooks";
import * as esbuild from "esbuild";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const FIXTURE_PATH = join(ROOT, "screenshots", "fixtures", "task-4909-tor-sidecar-fixture.mjs");
const LABEL = process.argv[2] ?? "UNLABELLED";
const DEADLINE_MS = 3000;
const TRIALS = 10;

async function bundleOrchestrator() {
  const outFile = join(tmpdir(), `osl-4909b-orchestrator-${process.pid}-${process.hrtime.bigint()}.mjs`);
  mkdirSync(tmpdir(), { recursive: true });
  await esbuild.build({
    entryPoints: [join(ROOT, "src", "tor-boot-orchestrator.ts")],
    outfile: outFile,
    bundle: true,
    format: "esm",
    platform: "node",
    write: true,
  });
  return import(outFile);
}

/** The real fixture process: silent for its full default 45000ms wait. */
function spawnRealSidecar() {
  const child = spawn(process.execPath, [FIXTURE_PATH], { stdio: ["ignore", "pipe", "inherit"] });
  const rl = createInterface({ input: child.stdout });
  const lines = [];
  rl.on("line", (line) => lines.push(line));
  return {
    child,
    lines,
    onLine(handler) {
      rl.on("line", handler);
    },
    kill() {
      rl.close();
      child.kill("SIGTERM");
    },
  };
}

const { startTorBootOrchestrator } = await bundleOrchestrator();
const spawned = [];
const paintTimesMs = [];
let sidecarsAliveAndSilent = 0;

try {
  for (let trial = 0; trial < TRIALS; trial += 1) {
    const trialStart = performance.now();
    let paintedAtMs = null;
    let sidecar = null;

    startTorBootOrchestrator({
      paint: () => {
        if (paintedAtMs === null) paintedAtMs = performance.now() - trialStart;
      },
      spawnSidecar: () => {
        sidecar = spawnRealSidecar();
        spawned.push(sidecar);
        return sidecar;
      },
      onStatus: () => undefined,
    });

    // Wait out the full three seconds unless the window paints sooner.
    while (paintedAtMs === null && performance.now() - trialStart < DEADLINE_MS) {
      await new Promise((r) => setTimeout(r, 5));
    }

    if (sidecar && sidecar.child.exitCode === null && sidecar.lines.length === 0) sidecarsAliveAndSilent += 1;
    paintTimesMs.push(paintedAtMs);
  }
} finally {
  for (const sidecar of spawned) sidecar.kill();
}

const painted = paintTimesMs.filter((ms) => ms !== null && ms < DEADLINE_MS).length;
console.log(`TASK4909B_CENSUS_LABEL=${LABEL}`);
console.log(`TASK4909B_PAINT_TIMES_MS=${JSON.stringify(paintTimesMs)}`);
console.log(`TASK4909B_SIDECARS_ALIVE_AND_SILENT=${sidecarsAliveAndSilent}/${TRIALS}`);
console.log(`TASK4909B_PAINTED_BY_SECOND_3=${painted} of ${TRIALS}`);
