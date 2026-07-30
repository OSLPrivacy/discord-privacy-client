#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { lstat, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  CANONICAL_ROLLOUT_SOURCE_PATHS,
  createCanonicalRolloutProvisioningAdmission,
} from "./canonical-rollout-admission-contract.mjs";

const MAX_EVIDENCE_BYTES = 1024 * 1024;

function usage() {
  throw new Error(
    "usage: node scripts/create-canonical-rollout-admission.mjs " +
      "--expected-commit <full-commit> --expected-tree <full-tree> " +
      "--evidence </absolute/predeploy-evidence.json>",
  );
}

export function parseCanonicalRolloutAdmissionArgs(argv) {
  if (argv.length !== 6) usage();
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (
      !["--expected-commit", "--expected-tree", "--evidence"].includes(flag) ||
      Object.hasOwn(values, flag)
    ) {
      usage();
    }
    values[flag] = value;
  }
  if (
    !/^[0-9a-f]{40}$/.test(values["--expected-commit"] ?? "") ||
    !/^[0-9a-f]{40}$/.test(values["--expected-tree"] ?? "") ||
    !path.isAbsolute(values["--evidence"] ?? "")
  ) {
    usage();
  }
  return {
    expectedCommit: values["--expected-commit"],
    expectedTree: values["--expected-tree"],
    evidencePath: values["--evidence"],
  };
}

function git(repoRoot, args, encoding = "utf8") {
  return execFileSync("git", ["-C", repoRoot, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

export async function loadCanonicalRolloutAdmissionInputs(
  repoRoot,
  { expectedCommit, expectedTree, evidencePath },
  gitRun = git,
) {
  const resolvedCommit = gitRun(
    repoRoot,
    ["rev-parse", "--verify", `${expectedCommit}^{commit}`],
  ).trim();
  const head = gitRun(repoRoot, ["rev-parse", "HEAD"]).trim();
  const repositoryTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedCommit}^{tree}`],
  ).trim();
  if (
    resolvedCommit !== expectedCommit ||
    head !== expectedCommit ||
    repositoryTree !== expectedTree
  ) {
    throw new Error("canonical rollout source commit/tree is not exact current HEAD");
  }
  const keyserverTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedCommit}:keyserver-cf`],
  ).trim();
  const fileValues = Object.fromEntries(
    CANONICAL_ROLLOUT_SOURCE_PATHS.map((sourcePath) => [
      sourcePath,
      gitRun(repoRoot, ["show", `${expectedCommit}:${sourcePath}`], null),
    ]),
  );
  const metadata = await lstat(evidencePath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size <= 0 ||
    metadata.size > MAX_EVIDENCE_BYTES
  ) {
    throw new Error("canonical rollout evidence is not a bounded regular file");
  }
  let evidence;
  try {
    evidence = JSON.parse(await readFile(evidencePath, "utf8"));
  } catch {
    throw new Error("canonical rollout evidence is not JSON");
  }
  return {
    anchor: {
      commit: expectedCommit,
      repository_tree: repositoryTree,
      keyserver_tree: keyserverTree,
    },
    fileValues,
    evidence,
  };
}

export async function runCanonicalRolloutAdmissionCli(
  argv,
  dependencies = {},
) {
  const args = parseCanonicalRolloutAdmissionArgs(argv);
  const repoRoot =
    dependencies.repoRoot ??
    path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const loadInputs =
    dependencies.loadInputs ??
    ((options) => loadCanonicalRolloutAdmissionInputs(repoRoot, options));
  const now = dependencies.now ?? (() => Date.now());
  const nonce = dependencies.randomUuid ?? randomUUID;
  const trustedProducers = dependencies.trustedProducers;
  const write =
    dependencies.write ?? ((text) => process.stdout.write(text));
  const inputs = await loadInputs(args);
  const receipt = createCanonicalRolloutProvisioningAdmission({
    ...inputs,
    nowMs: now(),
    receiptNonce: nonce(),
    ...(trustedProducers === undefined ? {} : { trustedProducers }),
  });
  write(`${JSON.stringify(receipt, null, 2)}\n`);
  return receipt;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  runCanonicalRolloutAdmissionCli(process.argv.slice(2)).catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : error}\n`);
    process.exitCode = 1;
  });
}
