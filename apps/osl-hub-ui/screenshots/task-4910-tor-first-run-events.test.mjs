import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import * as esbuild from "esbuild";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const FIXTURE_PATH = join(ROOT, "screenshots", "fixtures", "task-4910-tor-first-run-fixture.mjs");

async function bundleOrchestrator() {
  const outFile = join(tmpdir(), `osl-task-4910-${process.pid}-${Date.now()}.mjs`);
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

function spawnFixture(scenario) {
  const child = spawn(process.execPath, [FIXTURE_PATH, scenario], { stdio: ["ignore", "pipe", "inherit"] });
  const lines = createInterface({ input: child.stdout });
  return {
    child,
    onLine(handler) {
      lines.on("line", handler);
    },
    kill() {
      lines.close();
      child.kill("SIGTERM");
    },
  };
}

function waitFor(predicate, timeoutMs, message) {
  return new Promise((resolvePromise, rejectPromise) => {
    const deadline = Date.now() + timeoutMs;
    const poll = () => {
      if (predicate()) return resolvePromise();
      if (Date.now() >= deadline) return rejectPromise(new Error(message));
      setTimeout(poll, 10);
    };
    poll();
  });
}

test("TASK 4910 - four real event fixtures drive all first-run Tor states", { timeout: 90_000 }, async () => {
  const {
    firstRunTorScreenMarkup,
    startTorBootOrchestrator,
    torRouteStatusLabel,
  } = await bundleOrchestrator();

  const scenarios = ["cold", "warm", "slow", "failed"];
  const spawned = [];
  const handles = new Map();
  const statuses = new Map();
  const percentageChecks = [];
  const coldFailureSamples = [];
  const startedAt = Date.now();

  try {
    for (const scenario of scenarios) {
      let previousPercent = 0;
      let handle;
      handle = startTorBootOrchestrator({
        paint: () => undefined,
        spawnSidecar: () => {
          const sidecar = spawnFixture(scenario);
          spawned.push(sidecar);
          return sidecar;
        },
        onStatus: (status) => {
          statuses.set(scenario, status);
          if (!status.ready && !status.failed && status.percent !== previousPercent) {
            const label = torRouteStatusLabel(status);
            const copied = label === `Connecting -- ${status.percent}%`;
            percentageChecks.push({ scenario, eventStep: percentageChecks.length + 1, percent: status.percent, label, copied });
            previousPercent = status.percent;
          }
        },
      });
      handles.set(scenario, handle);
    }

    await waitFor(() => percentageChecks.length === 12, 5_000, "the four fixtures did not produce 12 bootstrap event steps");
    assert.equal(percentageChecks.filter((step) => step.copied).length, 12);

    assert.equal(torRouteStatusLabel(handles.get("cold").status()), "Connecting -- 5%");
    assert.equal(torRouteStatusLabel(handles.get("warm").status()), "Connecting -- 40%");

    await waitFor(
      () => torRouteStatusLabel(handles.get("failed").status()) === "Failed -- Tor could not connect",
      5_000,
      "failed fixture never surfaced its explicit error",
    );
    const failedMarkup = firstRunTorScreenMarkup(handles.get("failed").status());
    assert.match(failedMarkup, />Retry<\/button>/u);
    assert.match(failedMarkup, />Direct<\/button>/u);

    const coldSampler = setInterval(() => {
      const elapsedSeconds = (Date.now() - startedAt) / 1_000;
      if (elapsedSeconds < 75) coldFailureSamples.push(handles.get("cold").status().failed);
    }, 100);
    try {
      await waitFor(
        () => torRouteStatusLabel(handles.get("slow").status()) === "Slow -- still trying",
        50_000,
        "slow fixture was not labelled slow after 45 seconds",
      );
      assert.equal(torRouteStatusLabel(handles.get("slow").status()), "Slow -- still trying");

      await waitFor(() => handles.get("cold").status().ready, 80_000, "cold fixture did not reach ready at second 75");
    } finally {
      clearInterval(coldSampler);
    }

    const coldReadyAtSeconds = (Date.now() - startedAt) / 1_000;
    assert.ok(coldReadyAtSeconds >= 75, `cold fixture became ready too early at ${coldReadyAtSeconds}s`);
    assert.equal(coldFailureSamples.some(Boolean), false, "cold fixture was marked failed before ready at second 75");

    const labels = {
      cold: "Connecting -- 5%",
      warm: "Connecting -- 40%",
      slow: torRouteStatusLabel(handles.get("slow").status()),
      failed: torRouteStatusLabel(handles.get("failed").status()),
    };
    console.log(`TASK4910_LABELS=${JSON.stringify(labels)}`);
    console.log(`TASK4910_PERCENT_COPY=${percentageChecks.filter((step) => step.copied).length}/${percentageChecks.length}`);
    console.log(`TASK4910_PERCENT_STEPS=${JSON.stringify(percentageChecks)}`);
    console.log(`TASK4910_COLD_READY_AT_SECONDS=${coldReadyAtSeconds.toFixed(3)}`);
    console.log(`TASK4910_COLD_PRE_READY_FAILED_SAMPLES=${coldFailureSamples.filter(Boolean).length}/${coldFailureSamples.length}`);
    console.log("TASK4910_FAILED_ACTIONS=Retry,Direct");
  } finally {
    for (const handle of handles.values()) handle.stop();
    for (const sidecar of spawned) {
      if (sidecar.child.exitCode === null) sidecar.kill();
    }
  }
});
