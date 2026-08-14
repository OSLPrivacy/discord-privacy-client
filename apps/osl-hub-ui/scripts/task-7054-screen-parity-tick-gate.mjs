/**
 * TASK 7054 — screen work cannot be ticked around the D27 parity bars.
 *
 * The command accepts one task receipt, grades every named route afresh with
 * TASK 7050, then validates the final recorded TASK 7063 observation for that
 * same route.  It writes a verdict even when it refuses.  The input is an
 * explicit hand-off boundary: task runners can call this unattended without
 * treating an owner's PASS as permission to skip a machine failure.
 */
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { auditStructuralInventories } from "./task-7050-structural-inventory.mjs";
import { DIFFERENCE_LIMIT_PERCENT, MINIMUM_MEASUREMENTS } from "./task-7063-rebuild-compare-loop.mjs";
import { MEASUREMENT_VERSION } from "./task-7062-pixel-difference.mjs";

const PREFIX = "TASK7054";

function taskName(value) {
  return typeof value === "string" && value.trim() ? value.trim() : "<missing task id>";
}

function quote(value) {
  return JSON.stringify(value);
}

function problem(task, detail) {
  return `${PREFIX} REFUSED task=${quote(task)}: ${detail}`;
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function routeList(task, taskId) {
  if (!Array.isArray(task?.routes) || !task.routes.length) return { routes: [], findings: [problem(taskId, "screen task has no routes")] };
  const routes = [];
  const findings = [];
  const seen = new Set();
  for (const raw of task.routes) {
    if (typeof raw !== "string" || !raw.trim()) {
      findings.push(problem(taskId, "screen task has an empty route"));
      continue;
    }
    const route = raw.trim();
    if (seen.has(route)) {
      findings.push(problem(taskId, `screen task names route ${quote(route)} more than once`));
      continue;
    }
    seen.add(route);
    routes.push(route);
  }
  return { routes, findings };
}

function pageForRoute(manifest, route, task) {
  if (!Array.isArray(manifest)) return { finding: problem(task, `manifest is unavailable for route ${quote(route)}`) };
  const matches = [];
  for (const row of manifest) {
    if (row?.kind !== "routed") continue;
    const routes = Array.isArray(row.shippingRoutes) ? row.shippingRoutes : [row.route];
    if (routes.some((candidate) => typeof candidate === "string" && candidate.trim() === route)) matches.push(row);
  }
  if (!matches.length) return { finding: problem(task, `route ${quote(route)} has no design page in the 7049 manifest`) };
  if (matches.length !== 1 || typeof matches[0].page !== "string" || !matches[0].page.trim()) {
    return { finding: problem(task, `route ${quote(route)} has an ambiguous or unnamed design page in the 7049 manifest`) };
  }
  return { row: matches[0], page: matches[0].page.trim() };
}

function runStructuralComparison({ manifest, structural, route, row, page, task }) {
  if (typeof structural !== "function") return { ok: false, findings: [problem(task, `structural comparison is starved for route ${quote(route)} and design page ${quote(page)}`)] };
  let result;
  try {
    // Preserve a multi-route row exactly: TASK 7050 must expose that structural
    // mismatch rather than making the selected route look independently green.
    result = structural({
      manifest: [{ ...row }],
      routes: [route],
    });
  } catch (error) {
    return { ok: false, findings: [problem(task, `structural comparison failed for route ${quote(route)} and design page ${quote(page)}: ${error.message}`)] };
  }
  if (!isObject(result) || typeof result.ok !== "boolean" || !Array.isArray(result.findings)) {
    return { ok: false, findings: [problem(task, `structural comparison is starved for route ${quote(route)} and design page ${quote(page)}`)] };
  }
  return result;
}

function final7063Record(loop, { route, page, build, task }) {
  const fail = (detail) => ({ ok: false, finding: problem(task, `TASK 7063 recorded result is starved for route ${quote(route)} and design page ${quote(page)}: ${detail}`) });
  if (!isObject(loop)) return fail("missing result");
  if (loop.passed !== true) return fail(`not passed${loop.stopReason ? ` (${loop.stopReason})` : ""}`);
  if (!Array.isArray(loop.passes) || loop.passes.length < MINIMUM_MEASUREMENTS) return fail(`requires at least ${MINIMUM_MEASUREMENTS} recorded measurements`);
  const last = loop.passes.at(-1);
  if (!isObject(last) || !isObject(last.structural) || last.structural.ok !== true || !Array.isArray(last.structural.findings)) return fail("its structural comparison is missing or not passed");
  const measurement = last.measurement;
  if (!isObject(measurement) || measurement.measurement !== MEASUREMENT_VERSION) return fail(`percentage is not a ${MEASUREMENT_VERSION} measurement`);
  if (measurement.build !== build || last.build !== build) return fail(`measured build ${quote(measurement.build ?? last.build ?? "missing")} instead of task build ${quote(build)}`);
  if (measurement.build_route !== route || measurement.design_page !== page) return fail(`measured ${quote(measurement.build_route ?? "missing")} against ${quote(measurement.design_page ?? "missing")} instead of route ${quote(route)} and design page ${quote(page)}`);
  if (!Number.isFinite(measurement.percent_different) || measurement.percent_different < 0) return fail("percentage is missing or invalid");
  if (!Number.isInteger(measurement.compared_pixel_count) || measurement.compared_pixel_count < 1) return fail("compared pixel count is missing");
  if (measurement.percent_different >= DIFFERENCE_LIMIT_PERCENT) return {
    ok: false,
    finding: problem(task, `route ${quote(route)} design page ${quote(page)} pixel difference ${measurement.percent_different}% is at or above ${DIFFERENCE_LIMIT_PERCENT}%`),
    measurement,
  };
  return { ok: true, measurement };
}

function screenReport({ task, route, page, structural, loop }) {
  const percent = loop?.measurement?.percent_different;
  return `${PREFIX} SCREEN task=${quote(task)} route=${quote(route)} design_page=${quote(page)} structural=${structural.ok ? "PASS" : "NOT PASSED"} percent=${Number.isFinite(percent) ? percent : "missing"}`;
}

async function writeVerdictFile(file, verdict) {
  await mkdir(path.dirname(file), { recursive: true });
  const temporary = `${file}.${process.pid}.tmp`;
  await writeFile(temporary, `${JSON.stringify(verdict, null, 2)}\n`);
  await rename(temporary, file);
}

/**
 * Grade a requested screen-task tick.  Dependencies are passed explicitly so
 * the gate can prove starvation: there is no hidden success fallback for a
 * missing manifest, fresh structure comparator, 7063 reader, verdict writer,
 * or refusal action.
 */
export async function gradeScreenTaskTick({ receipt, structural, read7063, storeVerdict, refuse }) {
  const task = taskName(receipt?.task?.id);
  const reports = [];
  const findings = [];
  const screens = [];
  const taskData = receipt?.task;
  const build = typeof taskData?.build === "string" && taskData.build.trim() ? taskData.build.trim() : null;

  if (!isObject(receipt) || !isObject(taskData)) findings.push(problem(task, "gate receipt is starved: task is missing"));
  if (task === "<missing task id>") findings.push(problem(task, "gate receipt is starved: task id is missing"));
  if (taskData?.kind !== "screen") findings.push(problem(task, "gate receipt is starved: task is not explicitly a screen task"));
  if (!build) findings.push(problem(task, "gate receipt is starved: exact build is missing"));
  if (typeof structural !== "function") findings.push(problem(task, "gate structural comparison is starved"));
  if (typeof read7063 !== "function") findings.push(problem(task, "gate TASK 7063 comparison is starved"));
  if (typeof storeVerdict !== "function") findings.push(problem(task, "gate verdict storage is starved"));
  if (typeof refuse !== "function") findings.push(problem(task, "gate refusal is starved"));

  const listed = routeList(taskData, task);
  findings.push(...listed.findings);
  for (const route of listed.routes) {
    const pairing = pageForRoute(receipt?.manifest, route, task);
    if (!pairing.row) {
      findings.push(pairing.finding);
      reports.push(`${PREFIX} SCREEN task=${quote(task)} route=${quote(route)} design_page="<missing>" structural=NOT PASSED percent=missing`);
      screens.push({ route, design_page: null, build, structural: { ok: false, findings: [pairing.finding] }, percent_different: null });
      continue;
    }
    const input = { ...(receipt.structural ?? {}), manifest: undefined, routes: undefined };
    const structuralResult = runStructuralComparison({
      manifest: receipt.manifest,
      structural: typeof structural === "function" ? (scope) => structural({ ...input, ...scope }) : structural,
      route,
      row: pairing.row,
      page: pairing.page,
      task,
    });
    if (!structuralResult.ok) findings.push(...structuralResult.findings.map((finding) => problem(task, `route ${quote(route)} design page ${quote(pairing.page)} structural difference: ${finding}`)));
    let loopResult;
    if (typeof read7063 === "function") {
      try { loopResult = await read7063(route); } catch (error) { findings.push(problem(task, `TASK 7063 recorded result is starved for route ${quote(route)} and design page ${quote(pairing.page)}: ${error.message}`)); }
    }
    const loop = final7063Record(loopResult, { route, page: pairing.page, build, task });
    if (!loop.ok) findings.push(loop.finding);
    reports.push(screenReport({ task, route, page: pairing.page, structural: structuralResult, loop }));
    screens.push({
      route,
      design_page: pairing.page,
      build,
      structural: { ok: structuralResult.ok, findings: [...structuralResult.findings] },
      percent_different: loop.measurement?.percent_different ?? null,
      compared_pixel_count: loop.measurement?.compared_pixel_count ?? null,
      measurement: loop.measurement?.measurement ?? null,
    });
  }

  const override = receipt?.ownerReview?.override === true || receipt?.humanOverride === true;
  if (override) findings.push(problem(task, "human override is forbidden; machine parity is the first check and cannot be bypassed"));
  const ownerPass = receipt?.ownerReview?.verdict === "PASS";
  if (!findings.length && !ownerPass) findings.push(problem(task, "owner review is last and must record PASS after the machine parity checks"));
  if (findings.length && ownerPass) findings.push(problem(task, "owner PASS is recorded but cannot override the machine failure above"));

  const verdict = {
    version: 1,
    task_id: task,
    build: build ?? null,
    verdict: findings.length ? "REFUSED" : "PASS",
    owner_review: ownerPass ? "PASS" : "MISSING_OR_NOT_PASS",
    screens,
    findings,
  };
  if (typeof storeVerdict === "function") {
    try { await storeVerdict(verdict); } catch (error) { findings.push(problem(task, `verdict storage failed: ${error.message}`)); verdict.verdict = "REFUSED"; verdict.findings = findings; }
  }
  const result = { ok: findings.length === 0, task, build, reports, findings, verdict };
  if (!result.ok && typeof refuse === "function") {
    try { await refuse(result); } catch (error) { result.findings.push(problem(task, `refusal failed: ${error.message}`)); result.verdict.verdict = "REFUSED"; result.verdict.findings = result.findings; }
  }
  return result;
}

function usage() {
  throw new Error(`${PREFIX} usage: node scripts/task-7054-screen-parity-tick-gate.mjs --receipt <JSON> --verdict <JSON>`);
}

async function main() {
  const receiptAt = process.argv.indexOf("--receipt");
  const verdictAt = process.argv.indexOf("--verdict");
  if (receiptAt === -1 || verdictAt === -1 || !process.argv[receiptAt + 1] || !process.argv[verdictAt + 1]) usage();
  const receipt = JSON.parse(await readFile(path.resolve(process.cwd(), process.argv[receiptAt + 1]), "utf8"));
  const verdictFile = path.resolve(process.cwd(), process.argv[verdictAt + 1]);
  const result = await gradeScreenTaskTick({
    receipt,
    structural: auditStructuralInventories,
    read7063: async (route) => receipt.recorded7063?.[route],
    storeVerdict: (verdict) => writeVerdictFile(verdictFile, verdict),
    refuse: async () => {},
  });
  for (const line of result.reports) console.log(line);
  if (result.ok) console.log(`${PREFIX} TICKED task=${quote(result.task)} build=${quote(result.build)} screens=${result.verdict.screens.length}`);
  else for (const finding of result.findings) console.error(finding);
  if (!result.ok) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
