/**
 * TASK 7063 — make screenshot matching a converging, structural-first loop.
 *
 * A pass is intentionally an observed rebuild, not a claimed percentage.  The
 * production fixture path invokes TASK 7050 and TASK 7062 itself for every
 * pass, records the build/change that produced the capture, and has two hard
 * stops: a non-improving measurement or a finite pass limit.
 */
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { auditStructuralInventories } from "./task-7050-structural-inventory.mjs";
import { MEASUREMENT_VERSION, renderAndMeasure } from "./task-7062-pixel-difference.mjs";

export const DIFFERENCE_LIMIT_PERCENT = 1;
export const MINIMUM_MEASUREMENTS = 2;
export const DEFAULT_MAX_PASSES = 8;

function namedError(screen, detail) {
  return new Error(`TASK7063 NOT PASSED screen=${JSON.stringify(screen)}: ${detail}`);
}

function requireString(value, name, screen) {
  if (typeof value !== "string" || !value.trim()) throw namedError(screen, `${name} is missing`);
  return value.trim();
}

function validateMeasurement(measurement, screen) {
  if (!measurement || measurement.measurement !== MEASUREMENT_VERSION) {
    throw namedError(screen, `percentage did not come from TASK 7062 ${MEASUREMENT_VERSION}`);
  }
  if (!Number.isFinite(measurement.percent_different) || measurement.percent_different < 0) {
    throw namedError(screen, "TASK 7062 measurement has no finite percent_different");
  }
  if (!Number.isInteger(measurement.compared_pixel_count) || measurement.compared_pixel_count < 1) {
    throw namedError(screen, "TASK 7062 measurement has no compared pixel count");
  }
  return measurement;
}

function remainingDifferences(record) {
  const differences = [];
  if (record.structural.findings.length) differences.push(...record.structural.findings);
  if (record.measurement.percent_different >= DIFFERENCE_LIMIT_PERCENT) {
    differences.push(`pixel difference ${record.measurement.percent_different}% is not under ${DIFFERENCE_LIMIT_PERCENT}%`);
  }
  return differences;
}

function makeRecord(pass, attempt, structural, measurement, screen) {
  const build = requireString(pass.build, "build", screen);
  const changed = requireString(pass.changed, "what changed", screen);
  if (measurement.build !== build) {
    throw namedError(screen, `pass ${attempt} TASK 7062 measured build ${JSON.stringify(measurement.build)} instead of rebuilt ${JSON.stringify(build)}`);
  }
  return {
    attempt,
    build,
    changed,
    structural: { ok: structural.ok === true, findings: [...structural.findings] },
    measurement: {
      measurement: measurement.measurement,
      percent_different: measurement.percent_different,
      differing_pixel_count: measurement.differing_pixel_count,
      compared_pixel_count: measurement.compared_pixel_count,
      build: measurement.build,
      design_page: measurement.design_page,
      build_route: measurement.build_route,
    },
  };
}

function success(records) {
  const last = records.at(-1);
  return records.length >= MINIMUM_MEASUREMENTS
    && last.structural.ok
    && last.measurement.percent_different < DIFFERENCE_LIMIT_PERCENT
    && records.every((record, index) => index === 0 || record.measurement.percent_different < records[index - 1].measurement.percent_different);
}

/**
 * Run a bounded repair loop.  `rebuild` must return one newly built pass; its
 * change description is the durable audit trail for the repair applied before
 * that build.  `measure` is supplied by the caller so tests can exercise stop
 * paths, while the CLI below binds it only to TASK 7062's renderer.
 */
export async function runRebuildCompareLoop({ screen, rebuild, measure, maxPasses = DEFAULT_MAX_PASSES }) {
  const name = requireString(screen, "screen", "<unnamed>");
  if (typeof rebuild !== "function") throw namedError(name, "rebuild callback is missing");
  if (typeof measure !== "function") throw namedError(name, "TASK 7062 measurement callback is missing");
  if (!Number.isInteger(maxPasses) || maxPasses < MINIMUM_MEASUREMENTS) {
    throw namedError(name, `maxPasses must be an integer of at least ${MINIMUM_MEASUREMENTS}; an unbounded loop is forbidden`);
  }

  const passes = [];
  let stopReason = "pass limit reached";
  for (let attempt = 1; attempt <= maxPasses; attempt += 1) {
    const pass = await rebuild({ screen: name, attempt, previous: passes.at(-1) ?? null });
    if (!pass || typeof pass !== "object") {
      stopReason = "no further rebuilt pass was supplied";
      break;
    }
    // Structural comparison is deliberately before the 7062 measurement.
    if (!pass.structural || typeof pass.structural !== "object") throw namedError(name, `pass ${attempt} starved the structural comparison`);
    const structural = auditStructuralInventories(pass.structural);
    const measurement = validateMeasurement(await measure(pass, { screen: name, attempt, structural }), name);
    const record = makeRecord(pass, attempt, structural, measurement, name);
    passes.push(record);

    if (attempt > 1 && record.measurement.percent_different >= passes.at(-2).measurement.percent_different) {
      stopReason = "stopped improving";
      break;
    }
    if (success(passes)) {
      return { screen: name, passed: true, stopReason: "both bars passed", passes, remainingDifferences: [] };
    }
  }

  const last = passes.at(-1);
  if (!last) throw namedError(name, "no pass produced both a change and a TASK 7062 measurement");
  const remaining = remainingDifferences(last);
  if (passes.length < MINIMUM_MEASUREMENTS) {
    stopReason = "refused: a single measurement is not a loop";
    remaining.unshift("at least two TASK 7062 measurements are required");
  } else if (!last.structural.ok && !remaining.length) {
    remaining.push("structural comparison did not pass");
  } else if (last.measurement.percent_different < DIFFERENCE_LIMIT_PERCENT && !last.structural.ok) {
    stopReason = "structural difference remains";
  }
  return { screen: name, passed: false, stopReason, passes, remainingDifferences: remaining };
}

function report(result) {
  for (const pass of result.passes) {
    console.log(`TASK7063 PASS screen=${JSON.stringify(result.screen)} attempt=${pass.attempt} build=${JSON.stringify(pass.build)} changed=${JSON.stringify(pass.changed)} structural=${pass.structural.ok ? "PASS" : "NOT PASSED"} percent=${pass.measurement.percent_different} measurement=${pass.measurement.measurement}`);
    for (const finding of pass.structural.findings) console.error(`TASK7063 STRUCTURAL DIFFERENCE screen=${JSON.stringify(result.screen)} attempt=${pass.attempt}: ${finding}`);
  }
  if (result.passed) {
    const last = result.passes.at(-1);
    console.log(`TASK7063 PASSED screen=${JSON.stringify(result.screen)} measurements=${result.passes.length} last_percent=${last.measurement.percent_different} structural=PASS`);
    return;
  }
  const last = result.passes.at(-1);
  console.error(`TASK7063 NOT PASSED screen=${JSON.stringify(result.screen)} reason=${JSON.stringify(result.stopReason)} last_percent=${last.measurement.percent_different} remaining=${JSON.stringify(result.remainingDifferences)}`);
}

async function main() {
  const index = process.argv.indexOf("--fixture");
  if (index === -1 || !process.argv[index + 1]) throw new Error("TASK7063 usage: node scripts/task-7063-rebuild-compare-loop.mjs --fixture <JSON>");
  const fixture = JSON.parse(await readFile(path.resolve(process.cwd(), process.argv[index + 1]), "utf8"));
  let cursor = 0;
  const result = await runRebuildCompareLoop({
    screen: fixture.screen,
    maxPasses: fixture.maxPasses ?? fixture.passes?.length,
    rebuild: async () => {
      const pass = fixture.passes?.[cursor++];
      if (!pass) throw namedError(fixture.screen ?? "<unnamed>", "fixture has no next rebuilt pass");
      return pass;
    },
    measure: async (pass) => renderAndMeasure({
      designUrl: pass.designUrl,
      buildUrl: pass.buildUrl,
      buildId: pass.build,
      designPage: pass.designPage,
      buildRoute: pass.buildRoute,
      devicePixelRatio: pass.devicePixelRatio ?? 1,
    }),
  });
  report(result);
  if (!result.passed) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
