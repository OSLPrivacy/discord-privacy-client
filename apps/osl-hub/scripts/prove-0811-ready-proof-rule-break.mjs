import { spawnSync } from "node:child_process";
import { readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, "..");
const repoRoot = resolve(packageDir, "../..");
const sourceTest = resolve(packageDir, "tests/peer_attachment_network_e2e.rs");
const breakTest = resolve(packageDir, "tests/peer_attachment_network_e2e_0811_break.rs");
const expectedTargetDir = resolve(repoRoot, ".cargo-target");

if (process.env.CARGO_TARGET_DIR !== expectedTargetDir) {
  throw new Error(`CARGO_TARGET_DIR must be ${expectedTargetDir}`);
}

const original = readFileSync(sourceTest, "utf8");
const broken = original.replace(
  'return ReadyDecision::NonReady("missing-second-person");',
  "return ReadyDecision::Ready;",
);

if (broken === original) {
  throw new Error("0811 break proof could not construct the test copy");
}

try {
  writeFileSync(breakTest, broken, { flag: "wx" });
  const proof = spawnSync(
    "cargo",
    [
      "test",
      "--manifest-path",
      "apps/osl-hub/Cargo.toml",
      "-p",
      "osl-hub",
      "--features",
      "core",
      "--test",
      "peer_attachment_network_e2e_0811_break",
      "--locked",
      "-j1",
      "task_0811_no_second_person_ready_decision_check_refuses_false_ready",
      "--",
      "--test-threads=1",
      "--nocapture",
    ],
    { cwd: repoRoot, encoding: "utf8", env: process.env },
  );
  process.stdout.write(proof.stdout);
  process.stderr.write(proof.stderr);

  const combined = `${proof.stdout}\n${proof.stderr}`;
  const decisionCheckExit = proof.status === 0 ? 0 : 1;
  console.log(`TASK_0811_CARGO_EXIT=${proof.status}`);
  console.log(`TASK_0811_DECISION_CHECK_EXIT=${decisionCheckExit}`);

  if (
    decisionCheckExit !== 1 ||
    !combined.includes("false Ready decision") ||
    !combined.includes("decision=Ready")
  ) {
    process.exitCode = 1;
  }
} finally {
  rmSync(breakTest, { force: true });
}
