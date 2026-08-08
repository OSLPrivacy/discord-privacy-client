#!/usr/bin/env node
/**
 * Four deterministic Tor bootstrap event streams for TASK 4910. Every
 * scenario writes three genuine NDJSON bootstrap events. Nothing increments a
 * percentage in the UI: the UI can only copy these values.
 *
 * The cold scenario intentionally emits ready at real second 75. Warm and slow
 * remain live until the test stops them. Failed ends with an explicit error.
 */
const scenario = process.argv[2];
const fixtures = {
  cold: { percentages: [1, 3, 5], terminal: "ready", terminalAfterMs: 75_000 },
  warm: { percentages: [14, 27, 40], terminal: null, terminalAfterMs: null },
  slow: { percentages: [2, 6, 9], terminal: null, terminalAfterMs: null },
  failed: { percentages: [4, 11, 23], terminal: "error", terminalAfterMs: 25 },
};

const fixture = fixtures[scenario];
if (!fixture) throw new Error(`unknown fixture ${JSON.stringify(scenario)}`);

for (const percent of fixture.percentages) {
  process.stdout.write(`${JSON.stringify({ event: "bootstrap", percent })}\n`);
}

let terminalTimer = null;
if (fixture.terminal === "ready") {
  terminalTimer = setTimeout(() => {
    process.stdout.write(`${JSON.stringify({ event: "ready" })}\n`);
  }, fixture.terminalAfterMs);
} else if (fixture.terminal === "error") {
  terminalTimer = setTimeout(() => {
    process.stdout.write(`${JSON.stringify({ event: "error", message: "Tor could not connect" })}\n`);
  }, fixture.terminalAfterMs);
}

// Keep every stream open until its consumer explicitly stops the sidecar.
const keepAlive = setInterval(() => undefined, 1_000);
process.on("SIGTERM", () => {
  if (terminalTimer !== null) clearTimeout(terminalTimer);
  clearInterval(keepAlive);
  process.exit(0);
});
