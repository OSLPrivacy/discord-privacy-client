#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { lstat, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import {
  createScheme1ClientDeploymentPreflight,
  SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS,
  SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS,
  SCHEME1_FROZEN_SERVER_CONTRACT,
  SCHEME1_PREFLIGHT_ACTIONS,
} from "./scheme1-client-admission-contract.mjs";

const MAX_EVIDENCE_BYTES = 1024 * 1024;
const FULL_GIT_OBJECT = /^[0-9a-f]{40}$/u;
const LOWERCASE_UUID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/u;

function usage() {
  throw new Error(
    "usage: node scripts/create-scheme1-client-preflight.mjs " +
      "--action <migrate-0033-0034|activate-scheme1-worker> " +
      "--expected-server-commit <full-commit> " +
      "--expected-server-tree <full-tree> " +
      "--expected-client-commit <full-commit> " +
      "--expected-client-tree <full-tree> " +
      "--challenge <lowercase-uuid> --evidence </absolute/evidence.json>",
  );
}

export function parseScheme1ClientPreflightArgs(argv) {
  if (argv.length !== 14) usage();
  const accepted = new Set([
    "--action",
    "--challenge",
    "--evidence",
    "--expected-client-commit",
    "--expected-client-tree",
    "--expected-server-commit",
    "--expected-server-tree",
  ]);
  const values = {};
  for (let index = 0; index < argv.length; index += 2) {
    const flag = argv[index];
    const value = argv[index + 1];
    if (!accepted.has(flag) || Object.hasOwn(values, flag)) usage();
    values[flag] = value;
  }
  if (
    !SCHEME1_PREFLIGHT_ACTIONS.includes(values["--action"]) ||
    !FULL_GIT_OBJECT.test(values["--expected-server-commit"] ?? "") ||
    !FULL_GIT_OBJECT.test(values["--expected-server-tree"] ?? "") ||
    !FULL_GIT_OBJECT.test(values["--expected-client-commit"] ?? "") ||
    !FULL_GIT_OBJECT.test(values["--expected-client-tree"] ?? "") ||
    !LOWERCASE_UUID.test(values["--challenge"] ?? "") ||
    !path.isAbsolute(values["--evidence"] ?? "")
  ) {
    usage();
  }
  return {
    action: values["--action"],
    expectedServerCommit: values["--expected-server-commit"],
    expectedServerTree: values["--expected-server-tree"],
    expectedClientCommit: values["--expected-client-commit"],
    expectedClientTree: values["--expected-client-tree"],
    challenge: values["--challenge"],
    evidencePath: values["--evidence"],
  };
}

function git(repoRoot, args, encoding = "utf8") {
  return execFileSync("git", ["-C", repoRoot, ...args], {
    encoding,
    maxBuffer: 16 * 1024 * 1024,
  });
}

function exactGitObject(gitRun, repoRoot, expression, expected, label) {
  const actual = gitRun(
    repoRoot,
    ["rev-parse", "--verify", expression],
  ).trim();
  if (actual !== expected) {
    throw new Error(`${label} is not exact`);
  }
  return actual;
}

export async function loadScheme1ClientPreflightInputs(
  repoRoot,
  {
    expectedServerCommit,
    expectedServerTree,
    expectedClientCommit,
    expectedClientTree,
    evidencePath,
  },
  gitRun = git,
) {
  exactGitObject(
    gitRun,
    repoRoot,
    `${expectedServerCommit}^{commit}`,
    expectedServerCommit,
    "scheme-1 server source commit",
  );
  const head = gitRun(repoRoot, ["rev-parse", "HEAD"]).trim();
  const serverTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedServerCommit}^{tree}`],
  ).trim();
  if (head !== expectedServerCommit || serverTree !== expectedServerTree) {
    throw new Error(
      "scheme-1 server source commit/tree is not exact current HEAD",
    );
  }
  exactGitObject(
    gitRun,
    repoRoot,
    `${expectedClientCommit}^{commit}`,
    expectedClientCommit,
    "scheme-1 Rust client commit",
  );
  const clientTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedClientCommit}^{tree}`],
  ).trim();
  if (clientTree !== expectedClientTree) {
    throw new Error("scheme-1 Rust client tree is not exact");
  }
  const keyserverTree = gitRun(
    repoRoot,
    ["rev-parse", `${expectedServerCommit}:keyserver-cf`],
  ).trim();
  const fileValues = Object.fromEntries(
    SCHEME1_CLIENT_PREFLIGHT_SOURCE_PATHS.map((sourcePath) => [
      sourcePath,
      gitRun(
        repoRoot,
        ["show", `${expectedServerCommit}:${sourcePath}`],
        null,
      ),
    ]),
  );
  const frozenContractFileValues = Object.fromEntries(
    SCHEME1_FROZEN_CONTRACT_SOURCE_PATHS.map((sourcePath) => [
      sourcePath,
      gitRun(
        repoRoot,
        [
          "show",
          `${SCHEME1_FROZEN_SERVER_CONTRACT.commit}:${sourcePath}`,
        ],
        null,
      ),
    ]),
  );
  const frozenFixtureBytes =
    frozenContractFileValues[
      SCHEME1_FROZEN_SERVER_CONTRACT.fixture_path
    ];

  const metadata = await lstat(evidencePath);
  if (
    !metadata.isFile() ||
    metadata.isSymbolicLink() ||
    metadata.size <= 0 ||
    metadata.size > MAX_EVIDENCE_BYTES
  ) {
    throw new Error(
      "scheme-1 client evidence is not a bounded regular file",
    );
  }
  let clientEvidence;
  try {
    clientEvidence = JSON.parse(await readFile(evidencePath, "utf8"));
  } catch {
    throw new Error("scheme-1 client evidence is not JSON");
  }
  return {
    deploymentAnchor: {
      commit: expectedServerCommit,
      repository_tree: serverTree,
      keyserver_tree: keyserverTree,
    },
    expectedClientCommit,
    expectedClientTree: clientTree,
    fileValues,
    frozenFixtureBytes,
    frozenContractFileValues,
    clientEvidence,
  };
}

export async function runScheme1ClientPreflightCli(argv, dependencies = {}) {
  const args = parseScheme1ClientPreflightArgs(argv);
  const repoRoot =
    dependencies.repoRoot ??
    path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const loadInputs =
    dependencies.loadInputs ??
    ((options) => loadScheme1ClientPreflightInputs(repoRoot, options));
  const now = dependencies.now ?? (() => Date.now());
  const trustedProducers = dependencies.trustedProducers;
  const write =
    dependencies.write ?? ((text) => process.stdout.write(text));
  const inputs = await loadInputs(args);
  const receipt = createScheme1ClientDeploymentPreflight({
    action: args.action,
    expectedChallenge: args.challenge,
    ...inputs,
    nowMs: now(),
    ...(trustedProducers === undefined ? {} : { trustedProducers }),
  });
  write(`${JSON.stringify(receipt, null, 2)}\n`);
  return receipt;
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  runScheme1ClientPreflightCli(process.argv.slice(2)).catch((error) => {
    process.stderr.write(
      `${error instanceof Error ? error.message : error}\n`,
    );
    process.exitCode = 1;
  });
}
