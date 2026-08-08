#!/usr/bin/env node
/**
 * TASK 4909's stand-in for the OSL-owned Tor sidecar (TASK 4905). It is
 * silent on stdout for its entire wait -- no progress lines, nothing -- and
 * only then reports bootstrap and ready, so a boot path that accidentally
 * waits for *any* sidecar output before painting is caught exactly the same
 * as one that waits for readiness itself.
 *
 * Usage: node task-4909-tor-sidecar-fixture.mjs [--wait-ms <n>]
 * Default wait is 45000ms, matching this task's cold-bootstrap bound.
 */
const waitArgIndex = process.argv.indexOf("--wait-ms");
const waitMs = waitArgIndex === -1 ? 45_000 : Number(process.argv[waitArgIndex + 1]);
if (!Number.isFinite(waitMs) || waitMs < 0) {
  throw new Error("--wait-ms must be a non-negative number");
}

const timer = setTimeout(() => {
  process.stdout.write(`${JSON.stringify({ event: "bootstrap", percent: 100 })}\n`);
  process.stdout.write(`${JSON.stringify({ event: "ready" })}\n`);
}, waitMs);

process.on("SIGTERM", () => {
  clearTimeout(timer);
  process.exit(0);
});
